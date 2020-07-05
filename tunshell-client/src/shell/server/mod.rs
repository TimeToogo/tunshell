use super::{ShellClientMessage, ShellServerMessage, ShellServerStream};
use crate::{ShellKey, TunnelStream};
use anyhow::{Error, Result};
use futures::stream::StreamExt;
use log::*;
use portable_pty::PtySize;
use std::time::Duration;
use tokio::time;
use tokio_util::compat::*;

mod pty;

use pty::*;

type ShellStream = ShellServerStream<Compat<Box<dyn TunnelStream>>>;

pub struct ShellServer {}

impl ShellServer {
    pub fn new() -> Result<ShellServer> {
        Ok(ShellServer {})
    }

    pub async fn run(self, stream: Box<dyn TunnelStream>, key: ShellKey) -> Result<()> {
        let mut stream = ShellStream::new(stream.compat());

        info!("waiting for key");
        self.wait_for_key(&mut stream, key).await?;
        info!("successfully authenticated client");

        info!("waiting for shell request");
        let shell = self.start_shell(&mut stream).await?;
        info!("shell started");

        self.steam_shell_io(&mut stream, shell).await?;

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

    async fn start_shell(&self, stream: &mut ShellStream) -> Result<ShellPty> {
        let request = tokio::select! {
            message = stream.next() => match message {
                Some(Ok(ShellClientMessage::StartShell(request))) => request,
                Some(Ok(message)) => return Err(Error::msg(format!("received unexpected message from client: {:?}", message))),
                Some(Err(err)) => return Err(Error::from(err).context("received invalid message from client")),
                None => return Err(Error::msg("client did not send start shell message"))
            },
            _ = time::delay_for(Duration::from_millis(3000)) => return Err(Error::msg("timed out while waiting for shell request"))
        };

        ShellPty::new(
            request.term.as_ref(),
            None,
            PtySize {
                rows: request.size.0,
                cols: request.size.1,
                pixel_width: 0,
                pixel_height: 0,
            },
        )
    }

    async fn steam_shell_io(&self, stream: &mut ShellStream, mut shell: ShellPty) -> Result<()> {
        let mut buff = [0u8; 1024];

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
                        shell.write_all(payload.as_slice()).await?;
                        info!("wrote {} bytes to pty", payload.len());
                    }
                    Some(Ok(ShellClientMessage::Resize(size))) => {
                        info!("received window resize: {:?}", size);
                        shell.resize(PtySize {
                            rows: size.0,
                            cols: size.1,
                            pixel_width: 0,
                            pixel_height: 0,
                        })?;
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

    #[test]
    fn test_new_ssh_server() {
        ShellServer::new().unwrap();
    }
}
