use crate::{SshCredentials, TunnelStream};
use anyhow::{Context, Error, Result};
use crossterm;
use futures::channel::mpsc;
use futures::channel::mpsc::{Receiver, Sender};
use futures::future;
use futures::stream::StreamExt;
use log::*;
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::thread;
use std::thread::JoinHandle;
use thrussh::client::Session;

pub struct SshClient {
    config: Arc<thrussh::client::Config>,
}

impl SshClient {
    pub fn new() -> Result<SshClient> {
        let config = thrussh::client::Config::default();
        let config = Arc::new(config);

        Ok(SshClient { config })
    }

    pub async fn connect(
        self,
        stream: Box<dyn TunnelStream>,
        ssh_credentials: SshCredentials,
    ) -> Result<()> {
        info!("connecting to ssh server");
        let handler = SshClientHandler::new();
        let mut session: thrussh::client::Handle =
            thrussh::client::connect_stream(self.config.clone(), stream, handler)
                .await
                .with_context(|| "Failed to initialise ssh session")?;

        info!("ssh client attempting to authenticate");
        let authenticated = session
            .authenticate_password(ssh_credentials.username, ssh_credentials.password)
            .await
            .with_context(|| "Error while authenticated with ssh server")?;

        if !authenticated {
            return Err(Error::msg("Failed to authenticate with ssh server"));
        }

        info!("ssh authenticated");

        info!("ssh opening channel");
        let mut channel: thrussh::client::Channel = session
            .channel_open_session()
            .await
            .with_context(|| "Failed to open ssh channel")?;

        info!("configuring host shell");
        let host_shell = HostShell::new().with_context(|| "Failed to configure host shell")?;

        debug!("ssh requesting pty");
        let (col_width, row_height) = host_shell.size()?;
        channel
            .request_pty(
                false,
                &host_shell.term()?,
                col_width as u32,
                row_height as u32,
                0,
                0,
                &[],
            )
            .await
            .with_context(|| "Failed to request pty")?;

        info!("ssh pty requested");
        info!("starting shell stream");

        let (reader, writer) = host_shell.create_reader_writer()?;
        let done = Arc::new(Mutex::new(false));
        futures::try_join!(
            self.stream_stdin_to_channel(reader, channel.clone(), done.clone()),
            self.stream_channel_to_stdout(writer, &mut channel, done.clone()),
        )?;

        info!("session finished");

        Ok(())
    }

    async fn stream_stdin_to_channel(
        &self,
        mut host_shell_reader: HostShellStdin,
        mut channel_sender: thrussh::client::ChannelSender,
        done: Arc<Mutex<bool>>
    ) -> Result<()> {
        while !*done.lock().unwrap() {
            let mut buff = [0u8; 1024];
            let read = host_shell_reader.read(&mut buff).await?;
            info!("read {} bytes from stdin", read);

            if read == 0 {
                break;
            }

            channel_sender.data(&buff[..read]).await?;
            info!("wrote {} bytes to ssh channel", read);
        }

        Ok(())
    }

    async fn stream_channel_to_stdout(
        &self,
        mut host_shell_writer: HostShellStdout,
        channel: &mut thrussh::client::Channel,
        done: Arc<Mutex<bool>>
    ) -> Result<()> {
        loop {
            info!("waiting for ssh message");
            match channel.wait().await {
                Some(thrussh::ChannelMsg::Data { data }) => {
                    info!("received {} bytes from ssh channel", data.len());
                    host_shell_writer.write(&data[..]).await?;
                }
                Some(thrussh::ChannelMsg::ExitStatus { exit_status: _ }) => {
                    info!("ssh channel closed");
                    *done.lock().unwrap() = true;
                    return Err(Error::msg("shell exited"));
                }
                Some(_) => {}
                None => {
                    info!("ssh message stream ended");
                    break;
                }
            };
        }

        *done.lock().unwrap() = true;
        Ok(())
    }
}

struct SshClientHandler {}

impl SshClientHandler {
    fn new() -> Self {
        Self {}
    }
}

impl thrussh::client::Handler for SshClientHandler {
    type FutureUnit = future::Ready<Result<(Self, Session), anyhow::Error>>;
    type FutureBool = future::Ready<Result<(Self, bool), anyhow::Error>>;

    fn finished_bool(self, b: bool) -> Self::FutureBool {
        future::ready(Ok((self, b)))
    }

    fn finished(self, session: Session) -> Self::FutureUnit {
        future::ready(Ok((self, session)))
    }

    fn check_server_key(
        self,
        _server_public_key: &thrussh_keys::key::PublicKey,
    ) -> Self::FutureBool {
        self.finished_bool(true)
    }
}

struct HostShellStdin {
    receiver: Receiver<Vec<u8>>,
    thread: Option<JoinHandle<()>>,
    buff: Vec<u8>,
}

struct HostShellStdout {
    sender: Sender<Vec<u8>>,
    thread: Option<JoinHandle<()>>,
}

struct HostShell {}

impl HostShellStdin {
    fn new() -> Result<Self> {
        let (receiver, thread) = Self::create_stdin_thread();

        Ok(Self {
            receiver,
            thread: Some(thread),
            buff: vec![],
        })
    }

    fn create_stdin_thread() -> (Receiver<Vec<u8>>, JoinHandle<()>) {
        let (mut tx, rx) = mpsc::channel::<Vec<u8>>(1000);

        let thread = thread::spawn(move || {
            let mut buff = [0u8; 1024];

            loop {
                info!("reading from stdin");
                match std::io::stdin().lock().read(&mut buff) {
                    Ok(0) => {
                        info!("stdin stream finished");
                        break;
                    }
                    Ok(read) => {
                        info!("read {} bytes from stdin", read);
                        tx.try_send(Vec::from(&buff[..read]))
                            .unwrap_or_else(|err| error!("Failed to send from stdin: {:?}", err))
                    }
                    Err(err) => {
                        error!("Failed to read from stdin: {:?}", err);
                        break;
                    }
                };
            }
        });

        (rx, thread)
    }

    async fn read(&mut self, buff: &mut [u8]) -> Result<usize> {
        if self.buff.len() == 0 {
            if let Some(buff) = self.receiver.next().await {
                self.buff.extend(buff);
            }
        }

        let read = std::cmp::min(self.buff.len(), buff.len());
        buff[..read].copy_from_slice(&self.buff[..read]);
        self.buff.drain(..read);

        Ok(read)
    }
}

impl Drop for HostShellStdin {
    fn drop(&mut self) {
        debug!("dropping shell stdin reader");
        // self.thread
        //     .take()
        //     .unwrap()
        //     .join()
        //     .expect("Failed to join stdin reader thread");
    }
}

impl HostShellStdout {
    fn new() -> Result<Self> {
        let (sender, thread) = Self::create_stdout_thread();

        Ok(Self {
            sender,
            thread: Some(thread),
        })
    }

    fn create_stdout_thread() -> (Sender<Vec<u8>>, JoinHandle<()>) {
        let (tx, mut rx) = mpsc::channel::<Vec<u8>>(1000);

        let thread = thread::spawn(move || {
            let stdout = std::io::stdout();

            loop {
                info!("waiting for next ssh data for stdout");
                match futures::executor::block_on(rx.next()) {
                    Some(data) => {
                        let mut stdout = stdout.lock();
                        match stdout.write_all(data.as_slice()) {
                            Ok(_) => {
                                info!("wrote {} bytes to stdout", data.len());
                                match stdout.flush() {
                                    Ok(_) => {
                                        info!("stdout flushed");
                                    }
                                    Err(err) => {
                                        error!("Failed to flush stdout: {:?}", err);
                                        break;
                                    }
                                };
                            }
                            Err(err) => {
                                error!("Failed to write to stdout: {:?}", err);
                                break;
                            }
                        }
                    }
                    None => {
                        error!("Failed to recv from stdout channel");
                        break;
                    }
                }
            }
        });

        (tx, thread)
    }

    async fn write(&mut self, buff: &[u8]) -> Result<()> {
        self.sender.try_send(Vec::from(buff))?;

        Ok(())
    }
}

impl Drop for HostShellStdout {
    fn drop(&mut self) {
        debug!("dropping shell stdout writer");
        // self.thread
        //     .take()
        //     .unwrap()
        //     .join()
        //     .expect("Failed to join stdout writer thread");
    }
}

impl HostShell {
    fn new() -> Result<Self> {
        crossterm::terminal::enable_raw_mode()?;
        Ok(Self {})
    }

    fn create_reader_writer(&self) -> Result<(HostShellStdin, HostShellStdout)> {
        Ok((HostShellStdin::new()?, HostShellStdout::new()?))
    }

    fn term(&self) -> Result<String> {
        match std::env::var("TERM") {
            Ok(term) => Ok(term),
            Err(err) => Err(Error::new(err)),
        }
    }

    fn size(&self) -> Result<(u16, u16)> {
        match crossterm::terminal::size() {
            Ok(size) => Ok(size),
            Err(err) => Err(Error::new(err)),
        }
    }
}

impl Drop for HostShell {
    fn drop(&mut self) {
        crossterm::terminal::disable_raw_mode()
            .unwrap_or_else(|err| debug!("Failed to disable terminal raw mode: {:?}", err));
    }
}
