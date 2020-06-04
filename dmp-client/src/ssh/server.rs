use crate::SshCredentials;
use crate::TunnelStream;
use anyhow::{Context, Error, Result};
use futures::future;
use log::*;
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::sync::Arc;
use std::time::Duration;
use thrussh::server::{Auth, Session};
use thrussh::ChannelId;

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

    shell_pty: Option<ShellPty>,
}

struct ShellPty {
    shell: Box<dyn portable_pty::Child + Send>,
    master_pty: Box<dyn portable_pty::MasterPty + Send>,
    pty_writer: Box<dyn std::io::Write + Send>,
    reader_thread: Option<std::thread::JoinHandle<()>>,
}

impl ShellPty {
    fn new(
        term: &str,
        pty_size: PtySize,
        channel_id: ChannelId,
        session_handle: thrussh::server::Handle,
    ) -> Result<Self> {
        info!("creating shell pty");
        let pty_system = native_pty_system();

        let pty: portable_pty::PtyPair = pty_system
            .openpty(pty_size)
            .with_context(|| "could not open pty")?;

        let mut cmd = Self::get_default_shell()?;
        cmd.env("TERM", term);

        let shell = pty
            .slave
            .spawn_command(cmd)
            .with_context(|| "Failed to open system shell")?;

        let pty_reader = pty
            .master
            .try_clone_reader()
            .with_context(|| "Failed to clone pty reader")?;
        let pty_writer = pty
            .master
            .try_clone_writer()
            .with_context(|| "Failed to clone pty writer")?;

        let reader_thread = Self::_start_pty_reader(session_handle, channel_id, pty_reader);

        info!("created shell pty");
        Ok(ShellPty {
            shell,
            master_pty: pty.master,
            pty_writer,
            reader_thread: Some(reader_thread),
        })
    }

    fn get_default_shell() -> Result<CommandBuilder> {
        // TODO: windows support
        let shell = std::env::var("SHELL").unwrap_or("/bin/sh".to_owned());

        let mut cmd = CommandBuilder::new(shell.clone());

        if shell == "bash" || shell == "/bin/bash" {
            cmd.arg("--norc");
        }

        if shell == "zsh" || shell == "/bin/zsh" {
            cmd.arg("--no-rcs");
        }

        Ok(cmd)
    }

    fn resize(&self, pty_size: PtySize) -> Result<()> {
        self.master_pty
            .resize(pty_size)
            .with_context(|| "Failed to resize pty")
    }

    fn write(&mut self, buff: &[u8]) -> Result<()> {
        match self.pty_writer.write_all(buff) {
            Ok(_) => Ok(()),
            Err(err) => Err(Error::new(err)),
        }
    }

    fn _start_pty_reader(
        mut session_handle: thrussh::server::Handle,
        channel_id: ChannelId,
        mut pty_reader: Box<dyn std::io::Read + Send>,
    ) -> std::thread::JoinHandle<()> {
        std::thread::spawn(move || {
            futures::executor::block_on(async {
                let mut buff = [0u8; 1024];

                loop {
                    info!("reading from pty");
                    let read = pty_reader.read(&mut buff).expect("Failed to read from pty");
                    info!("read {} bytes from pty", read);

                    if read == 0 {
                        session_handle
                            .exit_status_request(channel_id, 0)
                            .await
                            .unwrap_or_else(|err| error!("failed to send exit status: {:?}", err));

                        break;
                    }

                    let wrote = session_handle
                        .data(channel_id, cryptovec::CryptoVec::from_slice(&buff[..read]))
                        .await;

                    match wrote {
                        Ok(_) => info!("wrote {} bytes to ssh channel", read),
                        Err(err) => {
                            error!("error while writing to ssh session: {:?}", err);
                            break;
                        }
                    }
                }
            })
        })
    }
}

impl Drop for ShellPty {
    fn drop(&mut self) {
        info!("shutting down shell");

        match self.shell.try_wait() {
            Ok(None) => self.shell.kill().expect("Failed to shutdown shell"),
            Ok(Some(_)) => (),
            Err(_) => (),
        }

        let thread = self.reader_thread.take().unwrap();
        thread.join().expect("Failed to shutdown pty reader thread");
    }
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
            return future::ready(Err(Error::msg(
                "Cannot change window size before requesting a pty",
            )));
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
        let pty_result = ShellPty::new(
            term,
            PtySize {
                rows: row_height as u16,
                cols: col_width as u16,
                pixel_width: pix_width as u16,
                pixel_height: pix_height as u16,
            },
            channel,
            session.handle().clone(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_ssh_server() {
        SshServer::new();
    }
}
