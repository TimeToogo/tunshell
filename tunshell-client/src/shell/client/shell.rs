use anyhow::{Error, Result};
use crossterm;
use futures::channel::mpsc;
use futures::channel::mpsc::{UnboundedReceiver};
use futures::stream::StreamExt;
use io::{AsyncWriteExt, AsyncReadExt};
use log::*;
use tokio::io;
use std::thread::{self, JoinHandle};

pub struct HostShellStdin {
    stdin: io::Stdin,
}

pub struct HostShellStdout {
    stdout: io::Stdout,
}

pub struct HostShellResizeWatcher {
    receiver: UnboundedReceiver<(u16, u16)>,
}

pub struct HostShell {}

impl HostShellStdin {
    pub fn new() -> Result<Self> {
        Ok(Self { stdin: io::stdin() })
    }

    pub async fn read(&mut self, buff: &mut [u8]) -> Result<usize> {
        self.stdin.read(buff).await.map_err(Error::from)
    }
}

impl HostShellStdout {
    pub fn new() -> Result<Self> {
        Ok(Self { stdout: io::stdout() })
    }

    pub async fn write(&mut self, buff: &[u8]) -> Result<()> {
        self.stdout.write_all(buff).await.map_err(Error::from)?;
        self.stdout.flush().await.map_err(Error::from)?;
        Ok(())
    }
}

impl HostShellResizeWatcher {
    pub fn new() -> Result<Self> {
        let (receiver, _thread) = Self::create_thread();

        Ok(Self { receiver })
    }

    fn create_thread() -> (UnboundedReceiver<(u16, u16)>, JoinHandle<()>) {
        let (tx, rx) = mpsc::unbounded::<(u16, u16)>();
        let mut prev_size = None;

        let thread = thread::spawn(move || loop {
            let size = match crossterm::terminal::size() {
                Ok(size) => size,
                Err(err) => {
                    error!("Error while receiving terminal size: {:?}", err);
                    break;
                }
            };

            if prev_size.is_some() && prev_size.unwrap() == size {
                std::thread::sleep(std::time::Duration::from_millis(500));
                continue;
            }

            prev_size = Some(size);
            info!("terminal size changed to {:?}", size);

            match tx.unbounded_send(size) {
                Ok(_) => info!("sent terminal size to channel"),
                Err(err) => {
                    error!("Failed to send resize event to channel: {:?}", err);
                    break;
                }
            }
        });

        (rx, thread)
    }

    pub async fn next(&mut self) -> Result<(u16, u16)> {
        match self.receiver.next().await {
            Some(size) => Ok(size),
            None => Err(Error::msg("Resize watcher has been disposed")),
        }
    }
}

impl HostShell {
    pub fn new() -> Result<Self> {
        Ok(Self {})
    }

    pub async fn println(&self, output: &str) {
        println!("{}\r", output);
    }

    pub fn enable_raw_mode(&mut self) -> Result<()> {
        debug!("enabling tty raw mode");
        crossterm::terminal::enable_raw_mode()?;
        Ok(())
    }

    pub fn disable_raw_mode(&mut self) -> Result<()> {
        debug!("disabling tty raw mode");
        crossterm::terminal::disable_raw_mode()?;
        Ok(())
    }

    pub fn stdin(&self) -> Result<HostShellStdin> {
        HostShellStdin::new()
    }

    pub fn stdout(&self) -> Result<HostShellStdout> {
        HostShellStdout::new()
    }

    pub fn resize_watcher(&self) -> Result<HostShellResizeWatcher> {
        HostShellResizeWatcher::new()
    }

    pub fn term(&self) -> Result<String> {
        match std::env::var("TERM") {
            Ok(term) => Ok(term),
            Err(err) => Err(Error::new(err)),
        }
    }

    pub async fn size(&self) -> Result<(u16, u16)> {
        crossterm::terminal::size().map_err(Error::new)
    }
}

impl Drop for HostShell {
    fn drop(&mut self) {
        self.disable_raw_mode()
            .unwrap_or_else(|err| debug!("Failed to disable terminal raw mode: {:?}", err));
    }
}

#[cfg(test)]
mod tests {
    // use super::*;
    // use tokio::runtime::Runtime;
}
