use crate::SshCredentials;
use crate::TunnelStream;
use anyhow::{Context, Error, Result};
use async_trait::async_trait;
use futures::future;
use log::*;
use portable_pty::PtySize;
use std::sync::Arc;
use thrussh::server::{Auth, Session};
use thrussh::ChannelId;

mod pty;

pub struct SshServer {
    config: Arc<thrussh::server::Config>,
}

impl SshServer {
    pub fn new() -> Result<SshServer> {
        let server_key = thrussh_keys::key::KeyPair::generate_ed25519()
            .with_context(|| "Failed to generate SSH key")?;

        let mut config = thrussh::server::Config::default();
        config.methods = thrussh::MethodSet::PASSWORD;
        config.keys.push(server_key);
        let config = Arc::new(config);

        Ok(SshServer { config })
    }

    pub async fn run(
        self,
        stream: Box<dyn TunnelStream>,
        credentials: SshCredentials,
    ) -> Result<()> {
        thrussh::server::run_stream(
            self.config.clone(),
            stream,
            SshServerHandler {
                credentials,
                shell_pty: None,
            },
        )
        .await
    }
}

pub struct SshServerHandler {
    credentials: SshCredentials,

    shell_pty: Option<pty::ShellPty>,
}

impl thrussh::server::Handler for SshServerHandler {
    type FutureAuth = future::Ready<Result<(Self, Auth), anyhow::Error>>;
    type FutureUnit = future::Ready<Result<(Self, Session), anyhow::Error>>;
    type FutureBool = future::Ready<Result<(Self, Session, bool), anyhow::Error>>;

    fn finished_auth(self, auth: thrussh::server::Auth) -> Self::FutureAuth {
        future::ready(Ok((self, auth)))
    }

    fn finished_bool(self, b: bool, session: Session) -> Self::FutureBool {
        future::ready(Ok((self, session, b)))
    }

    fn finished(self, session: Session) -> Self::FutureUnit {
        future::ready(Ok((self, session)))
    }

    fn auth_password(self, user: &str, password: &str) -> Self::FutureAuth {
        // TODO: Implement timing safe string comparisons
        info!("ssh auth attempt");
        if self.credentials.username == user && self.credentials.password == password {
            warn!("ssh auth succeeded");
            self.finished_auth(Auth::Accept)
        } else {
            info!("ssh auth rejected");
            self.finished_auth(Auth::Reject)
        }
    }

    fn data(mut self, _channel: ChannelId, data: &[u8], session: Session) -> Self::FutureUnit {
        info!("ssh received {} bytes of data", data.len());
        if let Some(shell_pty) = &mut self.shell_pty {
            match shell_pty.write(data) {
                Ok(_) => {
                    info!("wrote {} bytes to pty", data.len());
                    self.finished(session)
                }
                Err(err) => {
                    error!("received error while writing to pty: {:?}", err);
                    future::ready(Err(err))
                }
            }
        } else {
            return future::ready(Err(Error::msg("Data received before requesting a pty")));
        }
    }

    fn pty_request(
        mut self,
        channel: ChannelId,
        term: &str,
        col_width: u32,
        row_height: u32,
        pix_width: u32,
        pix_height: u32,
        _modes: &[(thrussh::Pty, u32)],
        session: Session,
    ) -> Self::FutureUnit {
        info!("ssh pty request");
        let pty_result = pty::ShellPty::new(
            term,
            None,
            PtySize {
                rows: row_height as u16,
                cols: col_width as u16,
                pixel_width: pix_width as u16,
                pixel_height: pix_height as u16,
            },
            SshPtyHandler::new(channel, session.handle()),
        );

        match pty_result {
            Ok(shell_pty) => {
                self.shell_pty.replace(shell_pty);
                self.finished(session)
            }
            Err(err) => future::ready(Err(err)),
        }
    }

    fn window_change_request(
        mut self,
        _channel: ChannelId,
        col_width: u32,
        row_height: u32,
        pix_width: u32,
        pix_height: u32,
        session: Session,
    ) -> Self::FutureUnit {
        info!("ssh window change request");
        if let Some(shell_pty) = &mut self.shell_pty {
            let result = shell_pty.resize(PtySize {
                rows: row_height as u16,
                cols: col_width as u16,
                pixel_width: pix_width as u16,
                pixel_height: pix_height as u16,
            });

            match result {
                Ok(_) => self.finished(session),
                Err(err) => future::ready(Err(err)),
            }
        } else {
            return future::ready(Err(Error::msg(
                "Cannot change window size before requesting a pty",
            )));
        }
    }
}

struct SshPtyHandler {
    channel: ChannelId,
    session_handle: thrussh::server::Handle,
}

impl SshPtyHandler {
    fn new(channel: ChannelId, session_handle: thrussh::server::Handle) -> Self {
        Self {
            channel,
            session_handle,
        }
    }
}

#[async_trait]
impl pty::ShellPtyHandler for SshPtyHandler {
    async fn exit(&mut self, code: u8) {
        self.session_handle
            .exit_status_request(self.channel, code as u32)
            .await
            .unwrap_or_else(|err| error!("failed to send exit status: {:?}", err));
    }

    async fn stdout(&mut self, data: &[u8]) {
        let mut crypto_vec = cryptovec::CryptoVec::from_slice(data);

        loop {
            match self.session_handle.data(self.channel, crypto_vec).await {
                Ok(_) => {
                    info!("wrote {} bytes to ssh channel", data.len());
                    break;
                }
                Err(vec) => {
                    error!("error while writing to ssh session, will retry");
                    crypto_vec = vec;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_ssh_server() {
        SshServer::new().unwrap();
    }
}
