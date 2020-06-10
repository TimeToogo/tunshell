use anyhow::{Context, Error, Result};
use async_trait::async_trait;
use log::*;
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::io::{Read, Write};

pub struct ShellPty {
    shell: Box<dyn portable_pty::Child + Send>,
    master_pty: Box<dyn portable_pty::MasterPty + Send>,
    pty_writer: Box<dyn std::io::Write + Send>,
}

#[async_trait]
pub trait ShellPtyHandler: Send {
    async fn exit(&mut self, code: u8);
    async fn stdout(&mut self, data: &[u8]);
}

impl ShellPty {
    pub fn new(
        term: &str,
        pty_size: PtySize,
        handler: impl ShellPtyHandler + 'static,
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

        let reader_handle = Self::start_pty_reader_task(pty_reader, handler);

        info!("created shell pty");
        Ok(ShellPty {
            shell,
            master_pty: pty.master,
            pty_writer,
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

    fn start_pty_reader_task(
        mut pty_reader: Box<dyn std::io::Read + Send>,
        mut handler: impl ShellPtyHandler + 'static,
    ) -> tokio::task::JoinHandle<()> {
        tokio::task::spawn_blocking(move || {
            tokio::runtime::Runtime::new().unwrap().block_on(async {
                let mut buff = [0u8; 1024];

                loop {
                    info!("reading from pty");
                    let read = pty_reader.read(&mut buff).expect("Failed to read from pty");
                    info!("read {} bytes from pty", read);

                    if read == 0 {
                        info!("finished reading from pty");
                        handler.exit(0).await;
                        break;
                    }

                    handler.stdout(&buff[..read]).await;
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockShellEnv {
        original: Option<String>,
    }
    impl MockShellEnv {
        fn new(shell: Option<&str>) -> Self {
            let original = match std::env::var("SHELL") {
                Ok(shell) => Some(shell),
                Err(_) => None,
            };

            match shell {
                Some(shell) => std::env::set_var("SHELL", shell),
                None => std::env::remove_var("SHELL"),
            }

            Self { original }
        }
    }

    impl Drop for MockShellEnv {
        fn drop(&mut self) {
            match self.original.take() {
                Some(shell) => std::env::set_var("SHELL", shell),
                None => std::env::remove_var("SHELL"),
            }
        }
    }

    #[test]
    fn test_new_shell_bash() {
        let _mock = MockShellEnv::new(Some("/bin/bash"));

        let cmd = ShellPty::get_default_shell().unwrap();

        assert_eq!(
            format!("{:?}", cmd),
            "CommandBuilder { args: [\"/bin/bash\", \"--norc\"], envs: [], cwd: None }"
        );
    }

    #[test]
    fn test_new_shell_zsh() {
        let _mock = MockShellEnv::new(Some("/bin/zsh"));

        let cmd = ShellPty::get_default_shell().unwrap();

        assert_eq!(
            format!("{:?}", cmd),
            "CommandBuilder { args: [\"/bin/zsh\", \"--no-rcs\"], envs: [], cwd: None }"
        );
    }

    #[test]
    fn test_new_shell_sh() {
        let _mock = MockShellEnv::new(Some("/bin/sh"));

        let cmd = ShellPty::get_default_shell().unwrap();

        assert_eq!(
            format!("{:?}", cmd),
            "CommandBuilder { args: [\"/bin/sh\"], envs: [], cwd: None }"
        );
    }

    #[test]
    fn test_new_shell_no_env() {
        let _mock = MockShellEnv::new(None);

        ShellPty::get_default_shell().unwrap();
    }

    // TODO: tests
}
