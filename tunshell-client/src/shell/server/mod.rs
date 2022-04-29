use super::{ShellClientMessage, ShellServerMessage, ShellServerStream, proto::ShellStartedPayload};
use crate::{ShellKey, TunnelStream};
use anyhow::{Error, Result};
use futures::stream::StreamExt;
use log::*;
use std::time::Duration;
use tokio::{io::AsyncWriteExt, time};
use tokio_util::compat::*;

mod fallback;
use fallback::*;

mod default;
pub(self) use default::*;

mod shell;
use shell::*;

cfg_if::cfg_if! {
    if #[cfg(all(not(target_os = "ios"), not(target_os = "android")))] {
        mod pty;
        use pty::*;
    }
}

cfg_if::cfg_if! {
    if #[cfg(all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64")))] {
        mod remote_pty;
        use remote_pty::RemotePtyShell;
    }
}

// mod remote_pty;

pub(super) type ShellStream = ShellServerStream<Compat<Box<dyn TunnelStream>>>;

pub(crate) struct ShellServer {
    conf: ShellServerConfig,
}

pub(crate) struct ShellServerConfig {
    pub(crate) echo_stdout: bool,
}

impl ShellServer {
    pub(crate) fn new(conf: ShellServerConfig) -> Result<ShellServer> {
        Ok(ShellServer { conf })
    }

    pub(crate) async fn run(self, stream: Box<dyn TunnelStream>, key: ShellKey) -> Result<()> {
        let mut stream = ShellStream::new(stream.compat());

        info!("waiting for key");
        self.wait_for_key(&mut stream, key).await?;
        info!("successfully authenticated client");

        info!("waiting for shell request");
        let (shell_type, mut shell) = self.start_shell(&mut stream).await?;
        info!("shell started");

        stream.write(&ShellServerMessage::ShellStarted(shell_type)).await?;

        if shell.custom_io_handling() {
            shell.stream_io(&mut stream).await?;
        } else {
            self.steam_shell_io(&mut stream, shell).await?;
        }

        // We keep the connection alive for some time to allow the receive
        // of any acknowledgement packets and so the client can continue to receive
        // the last message
        // Improvement: add trait method to TunnelStream wait for ack'd connection state
        time::delay_for(Duration::from_millis(500)).await;

        Ok(())
    }

    async fn wait_for_key(&self, stream: &mut ShellStream, key: ShellKey) -> Result<()> {
        let received_key = tokio::select! {
            message = stream.next() => match message {
                Some(Ok(ShellClientMessage::Key(key))) => key,
                Some(Ok(message)) => return Err(Error::msg(format!("received unexpected message from client: {:?}", message))),
                Some(Err(err)) => return Err(Error::from(err).context("received invalid message from client")),
                None => return Err(Error::msg("client did not sent key"))
            },
            _ = time::delay_for(Duration::from_millis(3000)) => return Err(Error::msg("timed out while waiting for key"))
        };

        // TODO: timing safe comparison
        if received_key == key.key() {
            stream.write(&ShellServerMessage::KeyAccepted).await?;
            return Ok(());
        } else {
            stream.write(&ShellServerMessage::KeyRejected).await?;
            return Err(Error::msg("client key rejected"));
        }
    }

    async fn start_shell(&self, stream: &mut ShellStream) -> Result<(ShellStartedPayload, Box<dyn Shell + Send>)> {
        let request = tokio::select! {
            message = stream.next() => match message {
                Some(Ok(ShellClientMessage::StartShell(request))) => request,
                Some(Ok(message)) => return Err(Error::msg(format!("received unexpected message from client: {:?}", message))),
                Some(Err(err)) => return Err(Error::from(err).context("received invalid message from client")),
                None => return Err(Error::msg("client did not send start shell message"))
            },
            _ = time::delay_for(Duration::from_millis(3000)) => return Err(Error::msg("timed out while waiting for shell request"))
        };

        #[cfg(all(not(target_os = "ios"), not(target_os = "android")))]
        {
            debug!("initialising pty shell");
            let pty_shell = PtyShell::new(request.term.as_ref(), None, request.size.clone());

            if let Ok(pty_shell) = pty_shell {
                return Ok((ShellStartedPayload::LocalPty, Box::new(pty_shell)));
            }

            warn!("failed to init pty shell: {:?}", pty_shell.err().unwrap());
        }

        #[cfg(all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64")))]
        if request.remote_pty_support {
            debug!("initialising remote pty shell");
            let rpty_shell = RemotePtyShell::new(&request.term, request.color).await;

            if let Ok(rpty_shell) = rpty_shell {
                return Ok((ShellStartedPayload::RemotePty, Box::new(rpty_shell)));
            }

            warn!("failed to init remote pty shell: {:?}", rpty_shell.err().unwrap());
        }

        debug!("falling back to in-built shell");
        let fallback_shell = FallbackShell::new(request.term.as_ref(), request.size.clone());

        Ok((ShellStartedPayload::Fallback, Box::new(fallback_shell)))
    }

    async fn steam_shell_io<'a>(
        &self,
        stream: &mut ShellStream,
        mut shell: Box<dyn Shell + Send + 'a>,
    ) -> Result<()> {
        let mut buff = [0u8; 1024];
        let mut host_stdout = if self.conf.echo_stdout {
            Some(tokio::io::stdout())
        } else {
            None
        };

        loop {
            info!("waiting for shell message");
            tokio::select! {
                result = shell.read(&mut buff) => match result {
                    Ok(0) => {
                        let code = shell.exit_code().unwrap();
                        info!("shell has exited with status {}", code);
                        stream.write(&ShellServerMessage::Exited(code)).await?;
                        info!("send exit code status");
                        break;
                    },
                    Ok(read) => {
                        info!("read {} bytes from stdout", read);

                        if let Some(host_stdout) = host_stdout.as_mut() {
                            host_stdout.write_all(&buff[..read]).await?;
                            host_stdout.flush().await?;
                        }

                        stream.write(&ShellServerMessage::Stdout(buff[..read].to_vec())).await?;
                        info!("sent {} bytes to client shell", read);
                    },
                    Err(err) => {
                        error!("error while reading from stdout: {}", err);
                        return Err(err);
                    }
                },
                message = stream.next() => match message {
                    Some(Ok(ShellClientMessage::Stdin(payload))) => {
                        info!("received {} bytes from client shell", payload.len());
                        shell.write(payload.as_slice()).await?;
                        info!("wrote {} bytes to shell", payload.len());
                    }
                    Some(Ok(ShellClientMessage::Resize(size))) => {
                        info!("received window resize: {:?}", size);
                        shell.resize(size)?;
                    }
                    Some(Ok(message)) => {
                        return Err(Error::msg(format!("received unexpected message from shell client {:?}", message)));
                    }
                    Some(Err(err)) => {
                        return Err(Error::from(err).context("received invalid message from shell client"));
                    }
                    None => {
                        warn!("client shell stream ended");
                        break;
                    }
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell::proto::{StartShellPayload, WindowSize};
    use futures::io::Cursor;
    use tokio::runtime::Runtime;
    use tokio::time::timeout;
    use tunshell_shared::Message;

    fn new_shell_server() -> ShellServer {
        ShellServer::new(ShellServerConfig { echo_stdout: true }).unwrap()
    }

    #[test]
    fn test_new_shell_server() {
        new_shell_server();
    }

    #[test]
    fn test_rejected_key() {
        Runtime::new().unwrap().block_on(async {
            let mut mock_data = Vec::<u8>::new();

            mock_data.extend_from_slice(
                ShellClientMessage::Key("Invalid".to_owned())
                    .serialise()
                    .unwrap()
                    .to_vec()
                    .as_slice(),
            );

            let mock_stream = Cursor::new(mock_data).compat();
            new_shell_server()
                .run(Box::new(mock_stream), ShellKey::new("MyKey"))
                .await
                .expect_err("client key should be rejected");
        });
    }

    #[test]
    fn test_key_timeout() {
        Runtime::new().unwrap().block_on(async {
            let mock_data = Vec::<u8>::new();

            let mock_stream = Cursor::new(mock_data).compat();

            timeout(
                Duration::from_millis(5000),
                new_shell_server().run(Box::new(mock_stream), ShellKey::new("CorrectKey")),
            )
            .await
            .unwrap()
            .expect_err("should timeout");
        });
    }

    #[test]
    fn test_start_shell_timeout() {
        Runtime::new().unwrap().block_on(async {
            let mut mock_data = Vec::<u8>::new();

            mock_data.extend_from_slice(
                ShellClientMessage::Key("CorrectKey".to_owned())
                    .serialise()
                    .unwrap()
                    .to_vec()
                    .as_slice(),
            );

            let mock_stream = Cursor::new(mock_data).compat();

            timeout(
                Duration::from_millis(5000),
                new_shell_server().run(Box::new(mock_stream), ShellKey::new("CorrectKey")),
            )
            .await
            .unwrap()
            .expect_err("should timeout");
        });
    }

    #[test]
    fn test_start_connect_to_shell() {
        Runtime::new().unwrap().block_on(async {
            let mut mock_data = Vec::<u8>::new();

            mock_data.extend_from_slice(
                ShellClientMessage::Key("CorrectKey".to_owned())
                    .serialise()
                    .unwrap()
                    .to_vec()
                    .as_slice(),
            );

            mock_data.extend_from_slice(
                ShellClientMessage::StartShell(StartShellPayload {
                    term: "TERM".to_owned(),
                    color: true,
                    size: WindowSize(50, 50),
                    remote_pty_support: false
                })
                .serialise()
                .unwrap()
                .to_vec()
                .as_slice(),
            );

            mock_data.extend_from_slice(
                ShellClientMessage::Stdin("echo \"hello\"\n".as_bytes().to_vec())
                    .serialise()
                    .unwrap()
                    .to_vec()
                    .as_slice(),
            );

            mock_data.extend_from_slice(
                ShellClientMessage::Resize(WindowSize(100, 80))
                    .serialise()
                    .unwrap()
                    .to_vec()
                    .as_slice(),
            );

            mock_data.extend_from_slice(
                ShellClientMessage::Stdin("exit\n".as_bytes().to_vec())
                    .serialise()
                    .unwrap()
                    .to_vec()
                    .as_slice(),
            );

            let mock_stream = Cursor::new(mock_data).compat();
            let server = new_shell_server();

            server
                .run(Box::new(mock_stream), ShellKey::new("CorrectKey"))
                .await
                .unwrap();
        });
    }

    #[test]
    fn test_start_connect_to_shell_then_error() {
        Runtime::new().unwrap().block_on(async {
            let mut mock_data = Vec::<u8>::new();

            mock_data.extend_from_slice(
                ShellClientMessage::Key("CorrectKey".to_owned())
                    .serialise()
                    .unwrap()
                    .to_vec()
                    .as_slice(),
            );

            mock_data.extend_from_slice(
                ShellClientMessage::StartShell(StartShellPayload {
                    term: "TERM".to_owned(),
                    color: true,
                    size: WindowSize(50, 50),
                    remote_pty_support: false,
                })
                .serialise()
                .unwrap()
                .to_vec()
                .as_slice(),
            );

            mock_data.extend_from_slice(
                ShellClientMessage::Error("some error occurred".to_owned())
                    .serialise()
                    .unwrap()
                    .to_vec()
                    .as_slice(),
            );

            let mock_stream = Cursor::new(mock_data).compat();
            let server = new_shell_server();

            server
                .run(Box::new(mock_stream), ShellKey::new("CorrectKey"))
                .await
                .expect_err("should return error");
        });
    }
}
