use anyhow::{Context, Error, Result};
use async_trait::async_trait;
use log::*;
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};

pub struct ShellPty {
    state: ShellState,
    master_pty: Box<dyn portable_pty::MasterPty + Send>,
    pty_writer: Box<dyn std::io::Write + Send>,
    reader_task: Option<tokio::task::JoinHandle<()>>,
}

#[derive(Clone)]
struct ShellState {
    shell: Arc<Mutex<Box<dyn portable_pty::Child + Send>>>,
    exit_status: Arc<Mutex<Option<portable_pty::ExitStatus>>>,
}

#[async_trait]
pub trait ShellPtyHandler: Send {
    async fn exit(&mut self, code: u8);
    async fn stdout(&mut self, data: &[u8]);
}

impl ShellPty {
    pub fn new(
        term: &str,
        shell: Option<&str>,
        pty_size: PtySize,
        handler: impl ShellPtyHandler + 'static,
    ) -> Result<Self> {
        info!("creating shell pty");
        let pty_system = native_pty_system();

        let pty: portable_pty::PtyPair = pty_system
            .openpty(pty_size)
            .with_context(|| "could not open pty")?;

        let mut cmd = Self::get_default_shell(shell)?;
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

        let state = ShellState {
            shell: Arc::new(Mutex::new(shell)),
            exit_status: Arc::new(Mutex::new(None)),
        };

        let reader_task = Self::start_pty_reader_task(pty_reader, state.clone(), handler);

        info!("created shell pty");
        Ok(ShellPty {
            state,
            master_pty: pty.master,
            pty_writer,
            reader_task: Some(reader_task),
        })
    }

    fn get_default_shell(shell: Option<&str>) -> Result<CommandBuilder> {
        // TODO: windows support
        // Copied from portable_pty
        let shell =
            shell
                .and_then(|i| Some(i.to_owned()))
                .unwrap_or(std::env::var("SHELL").or_else(|_| {
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
                })?);

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
        state: ShellState,
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

                        state
                            .handle_exit(true)
                            .unwrap_or_else(|err| error!("Failed to exit shell: {}", err));

                        handler
                            .exit(if state.success().unwrap() { 0 } else { 1 })
                            .await;
                        break;
                    }

                    handler.stdout(&buff[..read]).await;
                }
            })
        })
    }

    #[allow(dead_code)]
    pub async fn wait_for_end_of_stream(&mut self) -> Result<()> {
        match self.reader_task.take().unwrap().await {
            Ok(_) => Ok(()),
            Err(err) => Err(Error::new(err)),
        }
    }

    fn exit_sync(&mut self) -> Result<()> {
        self.state.handle_exit(false)
    }
}

impl ShellState {
    fn handle_exit(&self, wait_for_exit: bool) -> Result<()> {
        let mut exit_status = self.exit_status.lock().unwrap();

        if exit_status.is_some() {
            return Ok(());
        }

        let mut shell = self.shell.lock().unwrap();
        let status = if wait_for_exit {
            match shell.wait() {
                Ok(status) => status,
                Err(err) => return Err(Error::new(err)),
            }
        } else {
            match shell.try_wait() {
                Ok(None) => {
                    shell.kill().expect("Failed to shutdown shell");
                    portable_pty::ExitStatus::with_exit_code(1)
                }
                Ok(Some(status)) => status,
                Err(err) => return Err(Error::new(err)),
            }
        };

        exit_status.replace(status);

        Ok(())
    }

    fn success(&self) -> Result<bool> {
        match self.exit_status.lock().unwrap().as_ref() {
            Some(status) => Ok(status.success()),
            None => Err(Error::msg("Shell has not exited")),
        }
    }
}

impl Drop for ShellPty {
    fn drop(&mut self) {
        self.exit_sync().expect("Failed to exit shell");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::Mutex;
    use std::sync::MutexGuard;

    #[test]
    fn test_new_shell_bash() {
        let cmd = ShellPty::get_default_shell(Some("/bin/bash")).unwrap();

        assert_eq!(
            format!("{:?}", cmd),
            "CommandBuilder { args: [\"/bin/bash\", \"--norc\"], envs: [], cwd: None }"
        );
    }

    #[test]
    fn test_new_shell_zsh() {
        let cmd = ShellPty::get_default_shell(Some("/bin/zsh")).unwrap();

        assert_eq!(
            format!("{:?}", cmd),
            "CommandBuilder { args: [\"/bin/zsh\", \"--no-rcs\"], envs: [], cwd: None }"
        );
    }

    #[test]
    fn test_new_shell_sh() {
        let cmd = ShellPty::get_default_shell(Some("/bin/sh")).unwrap();

        assert_eq!(
            format!("{:?}", cmd),
            "CommandBuilder { args: [\"/bin/sh\"], envs: [], cwd: None }"
        );
    }

    #[test]
    fn test_new_shell_no_env() {
        ShellPty::get_default_shell(None).unwrap();
    }

    struct MockPtyHandler {
        exit_code: Arc<Mutex<Option<u8>>>,
        output: Arc<Mutex<Vec<u8>>>,
    }

    impl MockPtyHandler {
        fn new() -> Self {
            Self {
                exit_code: Arc::new(Mutex::new(None)),
                output: Arc::new(Mutex::new(vec![])),
            }
        }

        fn exit_code(&mut self) -> MutexGuard<'_, Option<u8>> {
            self.exit_code.lock().unwrap()
        }

        fn output(&mut self) -> MutexGuard<'_, Vec<u8>> {
            self.output.lock().unwrap()
        }
    }

    #[async_trait]
    impl ShellPtyHandler for MockPtyHandler {
        async fn exit(&mut self, code: u8) {
            self.exit_code().replace(code);
        }

        async fn stdout(&mut self, data: &[u8]) {
            self.output().extend_from_slice(data);
        }
    }

    impl Clone for MockPtyHandler {
        fn clone(&self) -> Self {
            Self {
                exit_code: Arc::clone(&self.exit_code),
                output: Arc::clone(&self.output),
            }
        }
    }

    #[test]
    fn test_shell_pty_calls_handler_with_output() {
        let mut mock_handler = MockPtyHandler::new();

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let mut pty: ShellPty = ShellPty::new(
                "",
                Some("/bin/bash"),
                PtySize::default(),
                mock_handler.clone(),
            )
            .expect("Failed to initialise ShellPty");

            std::thread::sleep(std::time::Duration::from_millis(1000));

            pty.write("echo 'foobar'\n".as_bytes())
                .expect("Failed to write to shell");

            pty.write("exit\n".as_bytes())
                .expect("Failed to write to shell");

            pty.wait_for_end_of_stream()
                .await
                .expect("Failed to exit shell");
        });

        assert!(String::from_utf8(mock_handler.output().to_vec())
            .expect("Failed to parse stdout as utf8")
            .replace("\r\n", "\n")
            .contains("echo 'foobar'\nfoobar\n"));

        assert_eq!(mock_handler.exit_code().unwrap(), 0);
    }

    #[test]
    fn test_shell_pty_exit_on_error() {
        let mut mock_handler = MockPtyHandler::new();

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let mut pty: ShellPty = ShellPty::new(
                "",
                Some("/bin/bash"),
                PtySize::default(),
                mock_handler.clone(),
            )
            .expect("Failed to initialise ShellPty");

            std::thread::sleep(std::time::Duration::from_millis(10));

            pty.write("exit 1\n".as_bytes())
                .expect("Failed to write to shell");

            pty.wait_for_end_of_stream()
                .await
                .expect("Failed to exit shell");
        });

        assert_eq!(mock_handler.exit_code().unwrap(), 1);
    }
}
