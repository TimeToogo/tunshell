use crate::SshCredentials;
use crate::TunnelStream;
use anyhow::{Error, Result};
use futures::future;
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use thrussh::server::{Auth, Session};
use thrussh::ChannelId;
use std::cell::RefCell;

pub struct SshServer {
    config: Arc<thrussh::server::Config>,
}

impl SshServer {
    pub fn new() -> SshServer {
        let server_key =
            thrussh_keys::key::KeyPair::generate_ed25519().expect("Failed to generate SSH key");

        let mut config = thrussh::server::Config::default();
        config.methods = thrussh::MethodSet::PASSWORD;
        config.connection_timeout = Some(Duration::from_secs(10));
        config.keys.push(server_key);
        let config = Arc::new(config);

        SshServer { config }
    }

    pub async fn run<'a>(
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

impl Drop for ShellPty {
    fn drop(&mut self) {
        println!("Shutting down shell");

        match self.shell.try_wait() {
            Ok(None) => self.shell.kill().expect("Failed to shutdown shell"),
            Ok(Some(_)) => (),
            Err(_) => (),
        }

        let thread = self.reader_thread.take().unwrap();
        thread.join();
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
        if self.credentials.username == user && self.credentials.password == password {
            self.finished_auth(Auth::Accept)
        } else {
            self.finished_auth(Auth::Reject)
        }
    }

    fn data(self, channel: ChannelId, data: &[u8], session: Session) -> Self::FutureUnit {
        self.finished(session)
    }

    fn pty_request(
        mut self,
        channel: ChannelId,
        term: &str,
        col_width: u32,
        row_height: u32,
        pix_width: u32,
        pix_height: u32,
        modes: &[(thrussh::Pty, u32)],
        session: Session,
    ) -> Self::FutureUnit {
        let pty_system = native_pty_system();

        let pty: portable_pty::PtyPair = pty_system
            .openpty(PtySize {
                rows: row_height as u16,
                cols: col_width as u16,
                pixel_width: pix_width as u16,
                pixel_height: pix_height as u16,
            })
            .expect("Could not create pty");

        let mut cmd = CommandBuilder::new_default_prog();
        cmd.env("TERM", term);

        let shell = pty
            .slave
            .spawn_command(cmd)
            .expect("Failed to open system shell");

        let mut pty_reader = pty
            .master
            .try_clone_reader()
            .expect("Failed to clone pty reader");
        let mut pty_writer = pty
            .master
            .try_clone_writer()
            .expect("Failed to clone pty writer");

        let reader_thread = self.start_pty_reader(session.handle(), channel, pty_reader);

        self.shell_pty.replace(ShellPty {
            shell,
            master_pty: pty.master,
            pty_writer,
            reader_thread: Some(reader_thread),
        });

        self.finished(session)
    }

    fn window_change_request(
        mut self,
        channel: ChannelId,
        col_width: u32,
        row_height: u32,
        pix_width: u32,
        pix_height: u32,
        session: Session,
    ) -> Self::FutureUnit {
        if let Some(shell_pty) = &mut self.shell_pty {
            let result = shell_pty.master_pty.resize(PtySize {
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

    fn env_request(
        self,
        channel: ChannelId,
        variable_name: &str,
        variable_value: &str,
        session: Session,
    ) -> Self::FutureUnit {
        std::env::set_var(variable_name, variable_value);
        self.finished(session)
    }
}

impl SshServerHandler {
    fn start_pty_reader(
        &self,
        mut session_handle: thrussh::server::Handle,
        channel_id: ChannelId,
        mut pty_reader: Box<dyn std::io::Read + Send>,
    ) -> std::thread::JoinHandle<()> {
        std::thread::spawn(move || {
            futures::executor::block_on(async {
                let mut buff = [0u8; 1024];

                loop {
                    let read = pty_reader.read(&mut buff).expect("Failed to read from pty");

                    if read == 0 {
                        break;
                    }

                    let wrote = session_handle
                        .data(channel_id, cryptovec::CryptoVec::from_slice(&buff[..read]))
                        .await;

                    match wrote {
                        Ok(_) => (),
                        Err(_) => {
                            eprintln!("Error while writing to ssh session");
                            break;
                        }
                    }
                }
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::runtime::Runtime;

    #[test]
    fn test_new_ssh_server() {
        SshServer::new();
    }
}
