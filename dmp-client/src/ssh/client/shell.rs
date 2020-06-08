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

pub struct HostShell {
    event_stream: crossterm::event::EventStream,
}

impl HostShellStdin {
    pub fn new() -> Result<Self> {
        let (receiver, _thread) = Self::create_stdin_thread();

        Ok(Self {
            receiver,
            buff: vec![],
        })
    }

    fn create_stdin_thread() -> (UnboundedReceiver<Vec<u8>>, JoinHandle<()>) {
        let (tx, rx) = mpsc::unbounded::<Vec<u8>>();

        let thread = thread::spawn(move || {
            let mut buff = [0u8; 1024];

            while !tx.is_closed() {
                info!("reading from stdin");
                let read = match std::io::stdin().lock().read(&mut buff) {
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
        let (sender, _thread) = Self::create_stdout_thread();

        Ok(Self { sender })
    }

    fn create_stdout_thread() -> (UnboundedSender<Vec<u8>>, JoinHandle<()>) {
        let (tx, mut rx) = mpsc::unbounded::<Vec<u8>>();

        let thread = thread::spawn(move || {
            let stdout = std::io::stdout();

            futures::executor::block_on(async move {
                while let Some(data) = rx.next().await {
                    let mut stdout = stdout.lock();

                    match stdout.write_all(data.as_slice()) {
                        Ok(_) => {}
                        Err(err) => {
                            error!("Failed to write to stdout: {:?}", err);
                            break;
                        }
                    }

                    info!("wrote {} bytes to stdout", data.len());
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

impl HostShell {
    pub fn new() -> Result<Self> {
        crossterm::terminal::enable_raw_mode()?;
        Ok(Self {
            event_stream: crossterm::event::EventStream::new(),
        })
    }

    pub fn create_reader_writer(&self) -> Result<(HostShellStdin, HostShellStdout)> {
        Ok((HostShellStdin::new()?, HostShellStdout::new()?))
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

    pub async fn on_resized(&mut self) -> Result<(u16, u16)> {
        loop {
            match self.event_stream.next().await {
                Some(Ok(crossterm::event::Event::Resize(w, h))) => return Ok((w, h)),
                Some(Ok(_)) => {}
                Some(Err(err)) => return Err(Error::new(err)),
                None => return Err(Error::msg("No events in stream")),
            };
        }
    }
}

impl Drop for HostShell {
    fn drop(&mut self) {
        crossterm::terminal::disable_raw_mode()
            .unwrap_or_else(|err| debug!("Failed to disable terminal raw mode: {:?}", err));
    }
}
