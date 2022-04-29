use crate::shell::proto::{
    RemotePtyDataPayload, RemotePtyEventPayload, ShellClientMessage, ShellServerMessage, WindowSize,
};

use super::shell::Shell;
use super::ShellStream;
use anyhow::{Context, Error, Result};
use async_trait::async_trait;
use futures::StreamExt;
use log::*;
use std::collections::HashMap;
use std::env;
use std::net::ToSocketAddrs;
use std::os::unix::prelude::PermissionsExt;
use std::pin::Pin;
use std::process::{ExitStatus, Stdio};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, WriteHalf};
use tokio::net::{TcpStream, UnixListener, UnixStream};
use tokio::process::{Child, Command};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio::{fs, prelude::*};
use tokio_rustls::client::TlsStream;
use tokio_rustls::TlsConnector;
use webpki::DNSNameRef;

pub struct RemotePtyShell {
    connections: HashMap<u32, WriteHalf<UnixStream>>,
    con_id: u32,
    read_tx: UnboundedSender<(u32, Result<Vec<u8>>)>,
    state: Option<StreamingState>,
}

struct StreamingState {
    proc: Child,
    sock_listener: UnixListener,
    read_rx: UnboundedReceiver<(u32, Result<Vec<u8>>)>,
}

impl RemotePtyShell {
    pub(super) async fn new(term: &str, color: bool) -> Result<Self> {
        info!("creating remote pty shell");

        let path = download_rpty_bash().await?;
        let (sock_path, sock_listener) = create_pty_sock().await?;

        let proc = Command::new(path)
            .env("RPTY_TRANSPORT", format!("unix:{sock_path}"))
            .env("TERM", term)
            .env("PS1", if color { 
                r"\[\e[0;38;5;242m\][rpty] \[\e[0;92m\]\u\[\e[0;92m\]@\[\e[0;92m\]\H\[\e[0m\]:\[\e[0;38;5;39m\]\w\[\e[0m\]\$ \[\e[0m\]" 
            } else {
                r"[rpty] \u@\H:\w\$ "
            })
            .arg("--noprofile")
            .arg("--norc")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .with_context(|| "Failed to start shell")?;

        let (read_tx, read_rx) = unbounded_channel();

        info!("created remote pty shell");
        Ok(Self {
            connections: HashMap::new(),
            con_id: 0,
            read_tx,
            state: Some(StreamingState {
                proc,
                sock_listener,
                read_rx,
            }),
        })
    }

    async fn do_stream_io(mut self: Pin<&mut Self>, stream: &mut ShellStream) -> Result<()> {
        let mut state = self.state.take().unwrap();
        loop {
            tokio::select! {
                new_con = state.sock_listener.accept() => match new_con {
                    Ok(new_con) => self.handle_new_connection(stream, new_con.0).await?,
                    Err(err) => return Err(err).context("failed to accept connection")
                },
                new_read = state.read_rx.recv() => match new_read {
                    Some(new_read) => self.handle_new_read(stream, new_read.0, new_read.1).await?,
                    None => return Err(Error::msg("failed to read message"))
                },
                msg = stream.next() => match msg {
                    Some(Ok(ShellClientMessage::RemotePtyData(payload))) => {
                        self.handle_data(stream, payload).await?;
                    }
                    Some(Ok(message)) => {
                        return Err(Error::msg(format!("received unexpected message from shell client {:?}", message)));
                    }
                    Some(Err(err)) => {
                        return Err(Error::from(err).context("received invalid message from shell client"));
                    }
                    None => {
                        warn!("client shell stream ended");
                        return Ok(());
                    }
                },
                res = &mut state.proc => match res {
                    Ok(exit_code) => {
                        self.handle_exit(stream, exit_code).await?;
                        return Ok(());
                    }
                    Err(err) => return Err(Error::from(err).context("failed to wait for proc to exit"))
                }
            };
        }
    }

    async fn handle_new_connection(
        &mut self,
        stream: &mut ShellStream,
        connection: UnixStream,
    ) -> Result<()> {
        let (mut rx, tx) = tokio::io::split(connection);

        let con_id = self.con_id;
        self.con_id += 1;
        self.connections.insert(con_id, tx);

        stream
            .write(&ShellServerMessage::RemotePtyEvent(
                RemotePtyEventPayload::Connect(con_id),
            ))
            .await?;

        let read_tx = self.read_tx.clone();
        tokio::task::spawn(async move {
            let mut buff = [0u8; 1024];
            loop {
                match rx.read(&mut buff).await {
                    Ok(0) => {
                        let _ = read_tx.send((con_id, Err(Error::msg("stream eof"))));
                        break;
                    }
                    Ok(n) => {
                        let _ = read_tx.send((con_id, Ok(buff[..n].to_vec())));
                    }
                    Err(err) => {
                        let _ =
                            read_tx.send((con_id, Err(err).context("failed to read from stream")));
                        break;
                    }
                };
            }
        });

        Ok(())
    }

    async fn handle_data(
        &mut self,
        stream: &mut ShellStream,
        payload: RemotePtyDataPayload,
    ) -> Result<()> {
        let con = match self.connections.get_mut(&payload.con_id) {
            Some(con) => con,
            None => return Err(Error::msg("unknown connection id")),
        };

        let res = con
            .write_all(&payload.data.as_slice())
            .await
            .context("failed to write to unix stream");

        if let Err(err) = res {
            stream
                .write(&ShellServerMessage::RemotePtyEvent(
                    RemotePtyEventPayload::Close(payload.con_id),
                ))
                .await?;
            return Err(err);
        }

        Ok(())
    }

    async fn handle_new_read(
        &mut self,
        stream: &mut ShellStream,
        con_id: u32,
        res: Result<Vec<u8>>,
    ) -> Result<()> {
        match res {
            Ok(data) => {
                stream
                    .write(&ShellServerMessage::RemotePtyEvent(
                        RemotePtyEventPayload::Payload(RemotePtyDataPayload { con_id, data }),
                    ))
                    .await?;
            }
            Err(err) => {
                debug!("rpty read error: {}", err);
                self.connections.remove(&con_id);

                stream
                    .write(&ShellServerMessage::RemotePtyEvent(
                        RemotePtyEventPayload::Close(con_id),
                    ))
                    .await?;
            }
        }

        Ok(())
    }

    async fn handle_exit(&mut self, stream: &mut ShellStream, exit_code: ExitStatus) -> Result<()> {
        stream
            .write(&ShellServerMessage::Exited(
                exit_code.code().map(|i| i as u8).unwrap_or(0),
            ))
            .await
    }
}

#[async_trait]
impl Shell for RemotePtyShell {
    async fn read(&mut self, _buff: &mut [u8]) -> Result<usize> {
        unreachable!()
    }

    async fn write(&mut self, _buff: &[u8]) -> Result<()> {
        unreachable!()
    }

    fn resize(&mut self, _size: WindowSize) -> Result<()> {
        unreachable!()
    }

    fn exit_code(&self) -> Result<u8> {
        unreachable!()
    }

    fn custom_io_handling(&self) -> bool {
        true
    }

    async fn stream_io(&mut self, stream: &mut ShellStream) -> Result<()> {
        Pin::new(self).do_stream_io(stream).await
    }
}

async fn download_rpty_bash() -> Result<String> {
    let tmp_dir = get_temp_dir().await?;
    let local_path = format!("{tmp_dir}/bash-rpty");

    let mut file = fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .create(true)
        .open(local_path.clone())
        .await
        .map_err(|_| Error::msg("failed to open file"))?;

    let arch = if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        panic!("unknown cpu arch")
    };

    let url_path = format!("/bash-linux-{arch}-stripped");

    let hostname = "rpty-artifacts.tunshell.com";
    let sock_addr = (hostname, 443)
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| Error::msg(format!("could not resolve {hostname}")))?;

    let mut tls_config = tokio_rustls::rustls::ClientConfig::default();
    tls_config
        .root_store
        .add_server_trust_anchors(&webpki_roots::TLS_SERVER_ROOTS);

    let connector = TlsConnector::from(Arc::new(tls_config));

    debug!("connecting to rpty-artifacts.tunshel.com:443");
    let tcp = TcpStream::connect(sock_addr).await?;
    let mut tls = connector
        .connect(
            DNSNameRef::try_from_ascii(hostname.as_bytes()).unwrap(),
            tcp,
        )
        .await?;

    debug!("downloading rpty bash");
    tls.write_all(format!("GET {url_path} HTTP/1.1\nHost: {hostname}\n\n").as_bytes())
        .await?;

    let line = read_line(&mut tls).await?;

    if !line.contains("200 OK") {
        error!("unexpected response from server: {}", line);
        return Err(Error::msg("unexpected response from server"));
    }

    let mut content_length = 0;

    loop {
        let header = read_line(&mut tls).await?;

        if header.to_lowercase().starts_with("content-length:") {
            content_length = header
                .split_once(':')
                .unwrap()
                .1
                .trim()
                .parse::<u64>()
                .unwrap_or(0);
        }

        if header.is_empty() {
            // body starts now
            break;
        }
    }

    if content_length == 0 {
        return Err(Error::msg("failed to find content-length from response"));
    }

    debug!("saving file to tmp path");
    let mut buff = [0u8; 1024];
    let mut downloaded = 0;
    while downloaded < content_length {
        let n = tls.read(&mut buff).await.context("failed to read")?;
        if n == 0 {
            break;
        }

        file.write_all(&buff[..n])
            .await
            .context("failed to write")?;
        downloaded += n as u64;
    }

    if downloaded != content_length {
        return Err(Error::msg(format!(
            "download size {downloaded} was not the same as content length {content_length}"
        )));
    }

    debug!("finished downloading file");

    debug!("making executable");
    std::fs::set_permissions(local_path.clone(), std::fs::Permissions::from_mode(0o744)).unwrap();
    debug!("done");

    Ok(local_path)
}

async fn read_line(tls: &mut TlsStream<TcpStream>) -> Result<String> {
    let mut buff = vec![];

    loop {
        let char = tls.read_u8().await.context("failed to read")?;

        // ignore carriage return
        if char == 13 {
            continue;
        }

        // break on newline
        if char == 10 {
            break;
        }

        buff.push(char);
    }

    String::from_utf8(buff).context("failed to parse as utf8")
}

async fn get_temp_dir() -> Result<String> {
    let tmp_dir = env::current_exe().map_err(|_| Error::msg("could not get running exe path"))?;
    let tmp_dir = tmp_dir
        .parent()
        .ok_or_else(|| Error::msg("could not get path parent"))?;
    let tmp_dir = tmp_dir
        .to_str()
        .ok_or_else(|| Error::msg("failed to convert to str"))?;

    Ok(tmp_dir.to_string())
}

async fn create_pty_sock() -> Result<(String, UnixListener)> {
    let tmp_dir = get_temp_dir().await?;
    let pid = std::process::id();
    let sock_path = format!("{tmp_dir}/rpty.{pid}.sock");

    let listener =
        UnixListener::bind(sock_path.clone()).context("failed to create unix sock for rpty")?;

    Ok((sock_path, listener))
}

impl Drop for StreamingState {
    fn drop(&mut self) {
        // we dont want the shell hanging arou
        let _ = self.proc.kill();
    }
}

#[cfg(test)]
mod tests {}
