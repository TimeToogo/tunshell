use anyhow::{Context, Error, Result};
use log::*;
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::io::{Read, Write};
use thrussh::ChannelId;

pub struct ShellPty {
    shell: Box<dyn portable_pty::Child + Send>,
    master_pty: Box<dyn portable_pty::MasterPty + Send>,
    pty_writer: Box<dyn std::io::Write + Send>,
    reader_thread: Option<std::thread::JoinHandle<()>>,
}

impl ShellPty {
    pub fn new(
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

        let reader_thread = Self::start_pty_reader(session_handle, channel_id, pty_reader);

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
        // Copied from portable_pty
        let shell = std::env::var("SHELL").or_else(|_| {
            let ent = unsafe { libc::getpwuid(libc::getuid()) };

            if ent.is_null() {
                Ok("/bin/sh".into())
            } else {
                use std::ffi::CStr;
                use std::str;
                let shell = unsafe { CStr::from_ptr((*ent).pw_shell) };
                shell
                    .to_str()
                    .map(str::to_owned)
                    .context("failed to resolve shell")
            }
        })?;

        let mut cmd = CommandBuilder::new(shell.clone());

        if shell == "bash" || shell == "/bin/bash" {
            cmd.arg("--norc");
        }

        if shell == "zsh" || shell == "/bin/zsh" {
            cmd.arg("--no-rcs");
        }

        Ok(cmd)
    }

    pub fn resize(&self, pty_size: PtySize) -> Result<()> {
        self.master_pty
            .resize(pty_size)
            .with_context(|| "Failed to resize pty")
    }

    pub fn write(&mut self, buff: &[u8]) -> Result<()> {
        match self.pty_writer.write_all(buff) {
            Ok(_) => Ok(()),
            Err(err) => Err(Error::new(err)),
        }
    }

    fn start_pty_reader(
        mut session_handle: thrussh::server::Handle,
        channel_id: ChannelId,
        mut pty_reader: Box<dyn std::io::Read + Send>,
    ) -> std::thread::JoinHandle<()> {
        std::thread::spawn(move || {
            tokio::runtime::Runtime::new().unwrap().block_on(async {
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

                    let mut crypto_vec = cryptovec::CryptoVec::from_slice(&buff[..read]);

                    loop {
                        match session_handle.data(channel_id, crypto_vec).await {
                            Ok(_) => {
                                info!("wrote {} bytes to ssh channel", read);
                                break;
                            }
                            Err(vec) => {
                                error!("error while writing to ssh session, will retry");
                                crypto_vec = vec;
                            }
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

#[cfg(test)]
mod tests {
    use super::*;
}
