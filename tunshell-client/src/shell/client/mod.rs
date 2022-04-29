use super::{
    ShellClientMessage, ShellClientStream, ShellServerMessage, StartShellPayload, WindowSize,
};
use crate::{
    remote_pty_supported, shell::proto::ShellStartedPayload, util::delay::delay_for, ShellKey,
    TunnelStream,
};
use anyhow::{Context, Error, Result};
use futures::stream::StreamExt;
use log::*;
use std::time::Duration;
use tokio_util::compat::*;

cfg_if::cfg_if! {
    if #[cfg(target_arch = "wasm32")] {
        mod xtermjs;
        pub use xtermjs::*;
    } else if #[cfg(integration_test)] {
        mod test;
        pub use test::*;
    } else {
        mod shell;
        pub use shell::*;
    }
}
cfg_if::cfg_if! {
    if #[cfg(all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64")))] {
        mod remote_pty;
        use remote_pty::start_remote_pty_master;
    }
}

pub struct ShellClient {
    pub(crate) host_shell: HostShell,
}

type ShellStream = ShellClientStream<Compat<Box<dyn TunnelStream>>>;

impl ShellClient {
    pub(crate) fn new(host_shell: HostShell) -> Result<ShellClient> {
        Ok(ShellClient { host_shell })
    }

    pub(crate) async fn connect(
        &mut self,
        stream: Box<dyn TunnelStream>,
        key: ShellKey,
    ) -> Result<u8> {
        info!("connecting to shell server");
        let mut stream = ShellStream::new(stream.compat());

        info!("shell client attempting to authenticate");
        self.authenticate(&mut stream, key)
            .await
            .with_context(|| "Error while authenticating with shell server")?;

        info!("shell client authenticated");

        debug!("requesting shell from server");
        stream
            .write(&ShellClientMessage::StartShell(StartShellPayload {
                term: self.host_shell.term().unwrap_or("".to_owned()),
                color: self.host_shell.color().unwrap_or(false),
                size: WindowSize::from(self.host_shell.size().await?),
                remote_pty_support: remote_pty_supported(),
            }))
            .await?;

        info!("shell requested");

        let response = tokio::select! {
            message = stream.next() => match message {
                Some(Ok(ShellServerMessage::ShellStarted(res))) => res,
                Some(Ok(_)) => return Err(Error::msg("shell server returned an unexpected response")),
                Some(Err(err)) => return Err(Error::from(err).context("shell server returned an error")),
                None => return Err(Error::msg("did not receive shell started response"))
            },
            _ = delay_for(Duration::from_millis(30000)) =>  return Err(Error::msg("timed out while waiting for shell"))
        };

        let exit_code = if let ShellStartedPayload::RemotePty = response {
            cfg_if::cfg_if! {
                if #[cfg(not(all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))] {
                    return Err(Error::msg("shell server started remote pty when not supported on local"));
                } else {
                    info!("starting remote pty master");
                    start_remote_pty_master(stream).await
                }
            }
        } else {
            info!("starting shell stream");
            self.host_shell.enable_raw_mode()?;
            let exit_code = self.stream_shell_io(&mut stream).await;
            self.host_shell.disable_raw_mode()?;
            exit_code
        };
        info!("session finished");

        Ok(exit_code?)
    }

    async fn authenticate(&self, stream: &mut ShellStream, key: ShellKey) -> Result<()> {
        stream.write(&ShellClientMessage::Key(key.key)).await?;
        debug!("sent shell key to peer");

        let response = tokio::select! {
            message = stream.next() => match message {
                Some(Ok(message)) => message,
                Some(Err(err)) => return Err(Error::from(err).context("shell server returned invalid response")),
                None => return Err(Error::msg("did not receive authentication response"))
            },
            _ = delay_for(Duration::from_millis(3000)) =>  return Err(Error::msg("timed out while waiting for authentication"))
        };

        match response {
            ShellServerMessage::KeyAccepted => return Ok(()),
            ShellServerMessage::KeyRejected => {
                return Err(Error::msg("shell key rejected by server"))
            }
            message @ _ => {
                return Err(Error::msg(format!(
                    "unexpected message returned from server: {:?}",
                    message
                )))
            }
        }
    }

    async fn stream_shell_io(&mut self, stream: &mut ShellStream) -> Result<u8> {
        let mut buff = [0u8; 1024];
        let mut stdin = self.host_shell.stdin()?;
        let mut stdout = self.host_shell.stdout()?;
        let mut resize_watcher = self.host_shell.resize_watcher()?;

        loop {
            info!("waiting for shell message");
            tokio::select! {
                result = stdin.read(&mut buff) => match result {
                    Ok(read) => {
                        info!("read {} bytes from stdin", read);
                        if read == 0 {
                            return Err(Error::msg("stdin closed"));
                        }
                        stream.write(&ShellClientMessage::Stdin(buff[..read].to_vec())).await?;
                        info!("sent {} bytes to remote shell", read);
                    },
                    Err(err) => {
                        error!("error while reading from stdin: {}", err);
                        return Err(err);
                    }
                },
                message = stream.next() => match message {
                    Some(Ok(ShellServerMessage::Stdout(payload))) => {
                        info!("received {} bytes from remote shell", payload.len());
                        stdout.write(payload.as_slice()).await?;
                    }
                    Some(Ok(ShellServerMessage::Exited(code))) => {
                        info!("remote shell exited with code {}", code);
                        return Ok(code);
                    }
                    Some(Ok(message)) => {
                        return Err(Error::msg(format!("received unexpected message from shell server {:?}", message)));
                    }
                    Some(Err(err)) => {
                        return Err(Error::from(err).context("received invalid message from shell server"));
                    }
                    None => {
                        warn!("remote shell stream ended");
                        return Err(Error::msg("shell server stream closed unexpectedly"));
                    }
                },
                size = resize_watcher.next() => match size {
                    Ok(size) => stream.write(&ShellClientMessage::Resize(WindowSize::from(size))).await?,
                    Err(err) => error!("Error received from terminal resize event: {}", err)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::io::Cursor;
    use tokio::runtime::Runtime;
    use tokio::time::timeout;
    use tunshell_shared::Message;

    #[test]
    fn test_new_shell_client() {
        ShellClient::new(HostShell::new().unwrap()).unwrap();
    }

    #[test]
    fn test_rejected_key() {
        Runtime::new().unwrap().block_on(async {
            let mut mock_data = Vec::<u8>::new();

            mock_data.extend_from_slice(
                ShellServerMessage::KeyRejected
                    .serialise()
                    .unwrap()
                    .to_vec()
                    .as_slice(),
            );

            let mock_stream = Cursor::new(mock_data).compat();

            ShellClient::new(HostShell::new().unwrap())
                .unwrap()
                .connect(Box::new(mock_stream), ShellKey::new("MyKey"))
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
                ShellClient::new(HostShell::new().unwrap())
                    .unwrap()
                    .connect(Box::new(mock_stream), ShellKey::new("CorrectKey")),
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
                ShellServerMessage::KeyAccepted
                    .serialise()
                    .unwrap()
                    .to_vec()
                    .as_slice(),
            );

            let mock_stream = Cursor::new(mock_data).compat();

            timeout(
                Duration::from_millis(5000),
                ShellClient::new(HostShell::new().unwrap())
                    .unwrap()
                    .connect(Box::new(mock_stream), ShellKey::new("CorrectKey")),
            )
            .await
            .unwrap()
            .expect_err("should timeout");
        });
    }
}
