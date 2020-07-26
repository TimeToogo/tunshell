use super::{get_default_shell, shell::Shell, DefaultShell};
use crate::shell::proto::WindowSize;
use anyhow::{Context, Error, Result};
use async_trait::async_trait;
use log::*;
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::io::{Read, Write};
use std::{
    panic,
    sync::{Arc, Mutex},
};
use tokio::runtime::Runtime;
use tokio::sync::mpsc::{
    channel,
    error::{TryRecvError, TrySendError},
    Receiver, Sender,
};
use tokio::task::JoinHandle;

pub struct PtyShell {
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

impl PtyShell {
    pub(super) fn new(term: &str, shell: Option<&str>, size: WindowSize) -> Result<Self> {
        info!("creating pty shell");
        let pty = panic::catch_unwind(|| {
            let pty_system = native_pty_system();

            pty_system
                .openpty(size.into())
                .with_context(|| "could not open pty")
        });

        if let Err(_) = pty {
            return Err(Error::msg("failed to init pty system"));
        }

        let pty = pty.unwrap();

        if let Err(_) = pty {
            return Err(Error::msg("failed to init pty"));
        }

        let pty = pty.unwrap();
        let mut cmd: CommandBuilder = get_default_shell(shell)?.into();
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
        Ok(PtyShell {
            state,
            master_pty: pty.master,
            reader_rx,
            recv_buff: vec![],
            writer_tx,
        })
    }

    fn start_pty_reader_task(
        mut pty_reader: Box<dyn std::io::Read + Send>,
        state: ShellState,
    ) -> (JoinHandle<()>, Receiver<Vec<u8>>) {
        let (mut tx, rx) = channel(10);

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
        let (tx, mut rx) = channel::<Option<Vec<u8>>>(10);

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

    fn exit_sync(&mut self) -> Result<()> {
        self.state.exit_shell(false)
    }
}

#[async_trait]
impl Shell for PtyShell {
    async fn read(&mut self, buff: &mut [u8]) -> Result<usize> {
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

        while let Ok(data) = self.reader_rx.try_recv() {
            self.recv_buff.extend_from_slice(data.as_slice());
        }

        let read = std::cmp::min(buff.len(), self.recv_buff.len());
        buff[..read].copy_from_slice(&self.recv_buff[..read]);
        self.recv_buff.drain(..read);

        Ok(read)
    }

    async fn write(&mut self, buff: &[u8]) -> Result<()> {
        self.writer_tx
            .send(Some(buff.to_vec()))
            .await
            .map_err(Error::from)
    }

    fn resize(&mut self, size: WindowSize) -> Result<()> {
        self.master_pty
            .resize(size.into())
            .with_context(|| "Failed to resize pty")
    }

    fn exit_code(&self) -> Result<u8> {
        let status = self.state.exit_status.lock().unwrap();

        let status = match status.as_ref() {
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

impl Into<PtySize> for WindowSize {
    fn into(self) -> PtySize {
        PtySize {
            cols: self.0,
            rows: self.1,
            pixel_width: 0,
            pixel_height: 0,
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
            // When the channel is empty we block until the we can receive the next value
            TryRecvError::Empty => match Runtime::new().unwrap().block_on(rx.recv()) {
                Some(value) => value,
                None => return Err(Error::msg("channel has closed")),
            },
        },
    };

    Ok(value)
}

impl ShellState {
    fn is_running(&self) -> bool {
        let exit_status = self.exit_status.lock().unwrap();

        exit_status.is_none()
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

        debug!("status: {:?}", status);
        self.exit_status.lock().unwrap().replace(status);
        info!("shell exited");

        Ok(())
    }
}

impl Drop for PtyShell {
    fn drop(&mut self) {
        self.exit_sync().expect("Failed to exit shell");
    }
}

impl Into<CommandBuilder> for DefaultShell {
    fn into(self) -> CommandBuilder {
        let mut cmd = CommandBuilder::new(self.path);
        cmd.args(self.args);
        cmd
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_shell_pty_exit_on_error() {
        Runtime::new().unwrap().block_on(async {
            let mut pty: PtyShell = PtyShell::new("", Some("/bin/bash"), WindowSize(80, 80))
                .expect("Failed to initialise ShellPty");

            tokio::time::delay_for(Duration::from_millis(10)).await;

            pty.write("exit 1\n\n".as_bytes())
                .await
                .expect("failed to write to shell");

            pty.exit_sync().unwrap();

            assert_eq!(pty.state.is_running(), false);
            assert_eq!(pty.exit_code().unwrap(), 1);
            assert_eq!(pty.exit_sync().unwrap(), ());
        });
    }
}
