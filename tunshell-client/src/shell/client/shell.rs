use anyhow::{Error, Result};
use crossterm;
use futures::channel::mpsc;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender};
use futures::stream::StreamExt;
use log::*;
use std::io::{Read, Write};
use std::thread;
use std::thread::JoinHandle;

pub struct HostShellStdin {
    receiver: UnboundedReceiver<Vec<u8>>,
    buff: Vec<u8>,
}

pub struct HostShellStdout {
    sender: UnboundedSender<Vec<u8>>,
}

pub struct HostShellResizeWatcher {
    receiver: UnboundedReceiver<(u16, u16)>,
}

pub struct HostShell {}

impl HostShellStdin {
    pub fn new() -> Result<Self> {
        let (receiver, _thread) = Self::create_thread();

        Ok(Self {
            receiver,
            buff: vec![],
        })
    }

    fn create_thread() -> (UnboundedReceiver<Vec<u8>>, JoinHandle<()>) {
        let (tx, rx) = mpsc::unbounded::<Vec<u8>>();

        let thread = thread::spawn(move || {
            let stdin = std::io::stdin();
            let mut buff = [0u8; 1024];

            while !tx.is_closed() {
                info!("reading from stdin");
                let read = match stdin.lock().read(&mut buff) {
                    Ok(0) => {
                        info!("stdin stream finished");
                        break;
                    }
                    Ok(read) => {
                        info!("read {} bytes from stdin", read);
                        read
                    }
                    Err(err) => {
                        error!("Failed to read from stdin: {:?}", err);
                        break;
                    }
                };

                match tx.unbounded_send(Vec::from(&buff[..read])) {
                    Ok(_) => info!("sent {} bytes from stdin to channel", read),
                    Err(err) => {
                        error!("Failed to send from stdin: {:?}", err);
                        break;
                    }
                }
            }
        });

        (rx, thread)
    }

    pub async fn read(&mut self, buff: &mut [u8]) -> Result<usize> {
        if self.buff.len() == 0 {
            if let Some(buff) = self.receiver.next().await {
                self.buff.extend(buff);
            }
        }

        let read = std::cmp::min(self.buff.len(), buff.len());
        buff[..read].copy_from_slice(&self.buff[..read]);
        self.buff.drain(..read);

        Ok(read)
    }
}

impl HostShellStdout {
    pub fn new() -> Result<Self> {
        let (sender, _thread) = Self::create_thread();

        Ok(Self { sender })
    }

    fn create_thread() -> (UnboundedSender<Vec<u8>>, JoinHandle<()>) {
        let (tx, mut rx) = mpsc::unbounded::<Vec<u8>>();

        let thread = thread::spawn(move || {
            let stdout = std::io::stdout();

            futures::executor::block_on(async move {
                let mut stdout = stdout.lock();

                while let Some(data) = rx.next().await {
                    match stdout.write_all(data.as_slice()) {
                        Ok(_) => debug!("wrote {} bytes to stdout", data.len()),
                        Err(err) => {
                            error!("Failed to write to stdout: {:?}", err);
                            break;
                        }
                    }

                    match stdout.flush() {
                        Ok(_) => info!("stdout flushed"),
                        Err(err) => {
                            error!("Failed to flush stdout: {:?}", err);
                            break;
                        }
                    };
                }
            });
        });

        (tx, thread)
    }

    pub async fn write(&mut self, buff: &[u8]) -> Result<()> {
        self.sender.unbounded_send(Vec::from(buff))?;

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
        crossterm::terminal::enable_raw_mode()?;
        Ok(Self {})
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

    pub fn size(&self) -> Result<(u16, u16)> {
        match crossterm::terminal::size() {
            Ok(size) => Ok(size),
            Err(err) => Err(Error::new(err)),
        }
    }
}

impl Drop for HostShell {
    fn drop(&mut self) {
        crossterm::terminal::disable_raw_mode()
            .unwrap_or_else(|err| debug!("Failed to disable terminal raw mode: {:?}", err));
    }
}

#[cfg(test)]
mod tests {
    // use super::*;
    // use tokio::runtime::Runtime;
}
