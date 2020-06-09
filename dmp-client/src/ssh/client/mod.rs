use crate::{SshCredentials, TunnelStream};
use anyhow::{Context, Error, Result};
use futures::future;
use log::*;
use std::sync::Arc;
use thrussh::client::Session;

mod shell;
use shell::*;

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

        self.stream_shell_io(host_shell, &mut channel).await?;

        info!("session finished");

        Ok(())
    }

    async fn stream_shell_io(
        &self,
        host_shell: HostShell,
        channel: &mut thrussh::client::Channel,
    ) -> Result<()> {
        let mut buff = [0u8; 1024];
        let mut stdin = host_shell.stdin()?;
        let mut stdout = host_shell.stdout()?;
        let mut resize_watcher = host_shell.resize_watcher()?;

        loop {
            info!("waiting for ssh message");
            tokio::select! {
                stdin_result = stdin.read(&mut buff) => match stdin_result {
                   Ok(read) => {
                    info!("read {} bytes from stdin", read);
                    if read == 0 {
                        return Err(Error::msg("stdin closed"));
                    }
                    channel.data(&buff[..read]).await?;
                    info!("wrote {} bytes to ssh channel", read);
                   },
                   Err(err) => {
                       error!("error while reading from stdin: {}", err);
                       return Err(err);
                   }
                },
                message = channel.wait() => match message {
                    Some(thrussh::ChannelMsg::Data { data }) => {
                        info!("received {} bytes from ssh channel", data.len());
                        stdout.write(&data[..]).await?;
                    }
                    Some(thrussh::ChannelMsg::ExitStatus { exit_status: _ }) => {
                        info!("ssh channel closed: shell exited");
                        return Ok(());
                    }
                    Some(_) => {}
                    None => {
                        info!("ssh message stream ended");
                        break;
                    }
                },
                size = resize_watcher.next() => match size {
                    Ok(size) => channel.window_change(size.0 as u32, size.1 as u32, 0, 0).await?,
                    Err(err) => error!("Error received from terminal resize event: {}", err)
                }
            }
        }

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
