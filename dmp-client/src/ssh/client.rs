use crate::{SshCredentials, TunnelStream};
use anyhow::{Context, Error, Result};
use crossterm;
use futures::future;
use log::debug;
use std::io::{Read, Write};
use std::sync::Arc;
use thrussh::client::Session;
use thrussh::ChannelId;

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
        let mut handler = SshClientHandler::new();
        let mut session: thrussh::client::Handle =
            thrussh::client::connect_stream(self.config.clone(), stream, handler)
                .await
                .with_context(|| "Failed to initialise ssh session")?;

        let authenticated = session
            .authenticate_password(ssh_credentials.username, ssh_credentials.password)
            .await
            .with_context(|| "Error while authenticated with ssh server")?;

        if !authenticated {
            return Err(Error::msg("Failed to authenticate with ssh server"));
        }

        let mut channel: thrussh::client::Channel = session
            .channel_open_session()
            .await
            .with_context(|| "Failed to open ssh channel")?;

        let host_shell = HostShell::new().with_context(|| "Failed to configure host shell")?;

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
}

struct HostShell {
    stdin: std::io::Stdin,
    stdout: std::io::Stdout,
}

impl HostShell {
    fn new() -> Result<Self> {
        crossterm::terminal::enable_raw_mode()?;

        Ok(Self {
            stdin: std::io::stdin(),
            stdout: std::io::stdout(),
        })
    }

    fn write(&mut self, buff: &[u8]) -> Result<()> {
        match self.stdout.lock().write_all(buff) {
            Ok(_) => Ok(()),
            Err(err) => Err(Error::new(err)),
        }
    }

    fn read(&mut self, buff: &mut [u8]) -> Result<usize> {
        match self.stdin.lock().read(buff) {
            Ok(read) => Ok(read),
            Err(err) => Err(Error::new(err)),
        }
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
