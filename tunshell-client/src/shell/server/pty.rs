use anyhow::{Context, Error, Result};
use log::*;
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use tokio::runtime::Runtime;
use tokio::sync::mpsc::{
    channel,
    error::{TryRecvError, TrySendError},
    Receiver, Sender,
};
use tokio::task::JoinHandle;

pub struct ShellPty {
    state: ShellState,
    master_pty: Box<dyn portable_pty::MasterPty + Send>,
    reader_rx: Receiver<Vec<u8>>,
    recv_buff: Vec<u8>,
    writer_tx: Sender<Option<Vec<u8>>>,
}

#[derive(Clone)]
struct ShellState {
    shell: Arc<Mutex<Box<dyn portable_pty::Child + Send>>>,
    exit_status: Arc<Mutex<Option<portable_pty::ExitStatus>>>,
}

impl ShellPty {
    pub fn new(term: &str, shell: Option<&str>, pty_size: PtySize) -> Result<Self> {
        info!("creating shell pty");
        let pty_system = native_pty_system();

        let pty: portable_pty::PtyPair = pty_system
            .openpty(pty_size)
            .with_context(|| "could not open pty")?;

        let mut cmd = get_default_shell(shell)?;
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

        let (_, reader_rx) = Self::start_pty_reader_task(pty_reader, state.clone());
        let (_, writer_tx) = Self::start_pty_writer_task(pty_writer, state.clone());

        info!("created shell pty");
        Ok(ShellPty {
            state,
            master_pty: pty.master,
            reader_rx,
            recv_buff: vec![],
            writer_tx,
        })
    }

    pub fn resize(&self, pty_size: PtySize) -> Result<()> {
        self.master_pty
            .resize(pty_size)
            .with_context(|| "Failed to resize pty")
    }

    fn start_pty_reader_task(
        mut pty_reader: Box<dyn std::io::Read + Send>,
        state: ShellState,
    ) -> (JoinHandle<()>, Receiver<Vec<u8>>) {
        let (mut tx, rx) = channel(1);

        let task = tokio::task::spawn_blocking(move || {
            let mut buff = [0u8; 1024];

            loop {
                info!("reading from pty");
                let read = match pty_reader.read(&mut buff) {
                    Ok(0) => {
                        info!("finished reading from pty");
                        state
                            .exit_shell(true)
                            .unwrap_or_else(|err| error!("Failed to exit shell: {}", err));
                        break;
                    }
                    Ok(read) => read,
                    Err(err) => {
                        warn!("failed to read from pty: {}", err);
                        break;
                    }
                };

                info!("read {} bytes from pty", read);

                if let Err(err) = send_sync(&mut tx, buff[..read].to_vec()) {
                    warn!("error while sending to channel: {}", err);
                    break;
                }
            }
        });

        (task, rx)
    }

    fn start_pty_writer_task(
        mut pty_writer: Box<dyn std::io::Write + Send>,
        state: ShellState,
    ) -> (JoinHandle<()>, Sender<Option<Vec<u8>>>) {
        let (tx, mut rx) = channel::<Option<Vec<u8>>>(1);

        let task = tokio::task::spawn_blocking(move || {
            while let Ok(Some(data)) = recv_sync(&mut rx) {
                info!("writing to pty");
                match pty_writer.write_all(data.as_slice()) {
                    Ok(_) => {}
                    Err(err) => {
                        warn!("failed to write to pty {}", err);
                        state
                            .exit_shell(true)
                            .unwrap_or_else(|err| error!("Failed to exit shell: {}", err));
                        break;
                    }
                };
                info!("wrote {} bytes to pty", data.len());
            }
        });

        (task, tx)
    }

    pub(super) async fn write_all(&mut self, buff: &[u8]) -> Result<()> {
        self.writer_tx
            .send(Some(buff.to_vec()))
            .await
            .map_err(Error::from)
    }

    pub(super) async fn read(&mut self, buff: &mut [u8]) -> Result<usize> {
        while self.recv_buff.len() == 0 {
            let stdout = match self.reader_rx.recv().await {
                Some(data) => data,
                None => {
                    if !self.state.is_running() {
                        return Ok(0);
                    }

                    return Err(Error::msg("failed to read from pty ready channel"));
                }
            };

            self.recv_buff.extend_from_slice(stdout.as_slice());
        }

        let read = std::cmp::min(buff.len(), self.recv_buff.len());
        buff[..read].copy_from_slice(&self.recv_buff[..read]);
        self.recv_buff.drain(..read);

        Ok(read)
    }

    fn exit_sync(&mut self) -> Result<()> {
        self.state.exit_shell(false)
    }

    pub(super) fn exit_code(&self) -> Result<u8> {
        let status = self.state.exit_status.lock().unwrap();

        let status = match status.clone() {
            Some(status) => status,
            None => return Err(Error::msg("shell has not exited")),
        };

        if status.success() {
            Ok(0)
        } else {
            Ok(1)
        }
    }
}

fn send_sync<T>(tx: &mut Sender<T>, value: T) -> Result<()>
where
    T: Sized + Sync + Send + std::fmt::Debug + 'static,
{
    match tx.try_send(value) {
        Ok(_) => {}
        Err(err) => match err {
            TrySendError::Closed(_) => return Err(Error::msg("channel has closed")),
            // When the channel is full we block until the we can send the value
            // by creating a new executor
            TrySendError::Full(value) => Runtime::new()
                .unwrap()
                .block_on(tx.send(value))
                .map_err(Error::from)?,
        },
    }

    Ok(())
}

fn recv_sync<T>(rx: &mut Receiver<T>) -> Result<T>
where
    T: Sized + Sync + Send,
{
    let value = match rx.try_recv() {
        Ok(value) => value,
        Err(err) => match err {
            TryRecvError::Closed => return Err(Error::msg("channel has closed")),
            // When the channel is full we block until the we can send the value
            // by creating a new executor
            TryRecvError::Empty => match Runtime::new().unwrap().block_on(rx.recv()) {
                Some(value) => value,
                None => return Err(Error::msg("channel has closed")),
            },
        },
    };

    Ok(value)
}

fn get_default_shell(shell: Option<&str>) -> Result<CommandBuilder> {
    // TODO: windows support
    // Copied from portable_pty
    let shell = shell
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

impl ShellState {
    fn is_running(&self) -> bool {
        let exit_status = self.exit_status.lock().unwrap();

        match *exit_status {
            Some(_) => return false,
            None => return true,
        }
    }

    fn exit_shell(&self, wait_for_exit: bool) -> Result<()> {
        if !self.is_running() {
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

        self.exit_status.lock().unwrap().replace(status);
        info!("shell exited");

        Ok(())
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

    #[test]
    fn test_new_shell_bash() {
        let cmd = get_default_shell(Some("/bin/bash")).unwrap();

        assert_eq!(
            format!("{:?}", cmd),
            "CommandBuilder { args: [\"/bin/bash\", \"--norc\"], envs: [], cwd: None }"
        );
    }

    #[test]
    fn test_new_shell_zsh() {
        let cmd = get_default_shell(Some("/bin/zsh")).unwrap();

        assert_eq!(
            format!("{:?}", cmd),
            "CommandBuilder { args: [\"/bin/zsh\", \"--no-rcs\"], envs: [], cwd: None }"
        );
    }

    #[test]
    fn test_new_shell_sh() {
        let cmd = get_default_shell(Some("/bin/sh")).unwrap();

        assert_eq!(
            format!("{:?}", cmd),
            "CommandBuilder { args: [\"/bin/sh\"], envs: [], cwd: None }"
        );
    }

    #[test]
    fn test_new_shell_no_env() {
        get_default_shell(None).unwrap();
    }

    // #[test]
    // fn test_shell_pty_calls_handler_with_output() {
    //     let mut mock_handler = MockPtyHandler::new();

    //     tokio::runtime::Runtime::new().unwrap().block_on(async {
    //         let mut pty: ShellPty = ShellPty::new(
    //             "",
    //             Some("/bin/bash"),
    //             PtySize::default(),
    //             mock_handler.clone(),
    //         )
    //         .expect("Failed to initialise ShellPty");

    //         std::thread::sleep(std::time::Duration::from_millis(1000));

    //         pty.write("echo 'foobar'\n".as_bytes())
    //             .expect("Failed to write to shell");

    //         pty.write("exit\n".as_bytes())
    //             .expect("Failed to write to shell");

    //         pty.wait_for_end_of_stream()
    //             .await
    //             .expect("Failed to exit shell");
    //     });

    //     assert!(String::from_utf8(mock_handler.output().to_vec())
    //         .expect("Failed to parse stdout as utf8")
    //         .replace("\r\n", "\n")
    //         .contains("echo 'foobar'\nfoobar\n"));

    //     assert_eq!(mock_handler.exit_code().unwrap(), 0);
    // }

    // #[test]
    // fn test_shell_pty_exit_on_error() {
    //     let mut mock_handler = MockPtyHandler::new();

    //     tokio::runtime::Runtime::new().unwrap().block_on(async {
    //         let mut pty: ShellPty = ShellPty::new(
    //             "",
    //             Some("/bin/bash"),
    //             PtySize::default(),
    //             mock_handler.clone(),
    //         )
    //         .expect("Failed to initialise ShellPty");

    //         std::thread::sleep(std::time::Duration::from_millis(10));

    //         pty.write("exit 1\n".as_bytes())
    //             .expect("Failed to write to shell");

    //         pty.wait_for_end_of_stream()
    //             .await
    //             .expect("Failed to exit shell");
    //     });

    //     assert_eq!(mock_handler.exit_code().unwrap(), 1);
    // }
}
