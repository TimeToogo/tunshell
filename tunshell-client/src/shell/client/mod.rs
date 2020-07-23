use super::{
    ShellClientMessage, ShellClientStream, ShellServerMessage, StartShellPayload, WindowSize,
};
use crate::{ShellKey, TunnelStream};
use anyhow::{Context, Error, Result};
use futures::stream::StreamExt;
use log::*;
use std::time::Duration;
use tokio::time;
use tokio_util::compat::*;

mod shell;
use shell::*;

pub struct ShellClient {}

type ShellStream = ShellClientStream<Compat<Box<dyn TunnelStream>>>;

impl ShellClient {
    pub(crate) fn new() -> Result<ShellClient> {
        Ok(ShellClient {})
    }

    pub(crate) async fn connect(self, stream: Box<dyn TunnelStream>, key: ShellKey) -> Result<u8> {
        info!("connecting to shell server");
        let mut stream = ShellStream::new(stream.compat());

        info!("shell client attempting to authenticate");
        self.authenticate(&mut stream, key)
            .await
            .with_context(|| "Error while authenticated with shell server")?;

        info!("shell client authenticated");

        info!("configuring host shell");
        let host_shell = HostShell::new().with_context(|| "Failed to configure host shell")?;

        debug!("requesting shell from server pty");
        stream
            .write(&ShellClientMessage::StartShell(StartShellPayload {
                term: host_shell.term().unwrap_or("".to_owned()),
                size: WindowSize::from(host_shell.size()?),
            }))
            .await?;

        info!("shell requested");
        info!("starting shell stream");

        let exit_code = self.stream_shell_io(host_shell, &mut stream).await?;

        info!("session finished");

        Ok(exit_code)
    }

    async fn authenticate(&self, stream: &mut ShellStream, key: ShellKey) -> Result<()> {
        stream.write(&ShellClientMessage::Key(key.key)).await?;

        let response = tokio::select! {
            message = stream.next() => match message {
                Some(Ok(message)) => message,
                Some(Err(err)) => return Err(Error::from(err).context("shell server returned invalid response")),
                None => return Err(Error::msg("did not receive authentication response"))
            },
            _ = time::delay_for(Duration::from_millis(3000)) =>  return Err(Error::msg("timed out while waiting for authentication"))
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

    async fn stream_shell_io(&self, host_shell: HostShell, stream: &mut ShellStream) -> Result<u8> {
        let mut buff = [0u8; 1024];
        let mut stdin = host_shell.stdin()?;
        let mut stdout = host_shell.stdout()?;
        let mut resize_watcher = host_shell.resize_watcher()?;

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
                        stdout.write(payload.as_slice())?;
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
        ShellClient::new().unwrap();
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

            ShellClient::new()
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
                ShellClient::new()
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
                ShellClient::new()
                    .unwrap()
                    .connect(Box::new(mock_stream), ShellKey::new("CorrectKey")),
            )
            .await
            .unwrap()
            .expect_err("should timeout");
        });
    }
}
