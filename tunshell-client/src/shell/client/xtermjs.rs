use crate::TerminalEmulator;
use anyhow::{Error, Result};
use js_sys::{Uint16Array, Uint8Array};
use log::*;
use std::cmp;
use tokio::sync::mpsc::{self, Receiver, Sender};
use wasm_bindgen::JsValue;

pub struct HostShellStdin {
    channel: Receiver<JsValue>,
    buff: Vec<u8>,
}

pub struct HostShellStdout {
    channel: Sender<Uint8Array>,
}

pub struct HostShellResizeWatcher {
    channel: Receiver<JsValue>,
}

pub struct HostShell {
    term: TerminalEmulator,
    stdin: Option<Receiver<JsValue>>,
    stdout: Option<Sender<Uint8Array>>,
    resize: Option<Receiver<JsValue>>,
    initial_size: JsValue,
}

impl HostShellStdin {
    pub fn new(channel: Receiver<JsValue>) -> Self {
        Self {
            channel,
            buff: vec![],
        }
    }

    pub async fn read(&mut self, buff: &mut [u8]) -> Result<usize> {
        while self.buff.len() == 0 {
            let data = self
                .channel
                .recv()
                .await
                .ok_or_else(|| Error::msg("stdin channel closed"))?;

            let data = if data.is_string() {
                data.as_string().unwrap().as_bytes().to_vec()
            } else {
                Uint8Array::new(&data).to_vec()
            };

            self.buff.extend_from_slice(data.as_slice());
        }

        let len = cmp::min(self.buff.len(), buff.len());
        buff[..len].copy_from_slice(&self.buff[..len]);
        self.buff.drain(..len);
        debug!("read {} bytes from terminal", len);

        Ok(len)
    }
}

impl HostShellStdout {
    pub fn new(channel: Sender<Uint8Array>) -> Self {
        Self { channel }
    }

    pub async fn write(&mut self, buff: &[u8]) -> Result<()> {
        debug!("writing {} bytes to stdout", buff.len());

        Ok(self
            .channel
            .send(buff.into())
            .await
            .map_err(|_| Error::msg("failed to send to stdout channel"))?)
    }
}

impl HostShellResizeWatcher {
    pub fn new(channel: Receiver<JsValue>) -> Self {
        Self { channel }
    }

    pub async fn next(&mut self) -> Result<(u16, u16)> {
        let size = self
            .channel
            .recv()
            .await
            .ok_or_else(|| Error::msg("resize channel closed"))?;
        let size = convert_to_size(size)?;
        debug!("terminal resized to {:?}", size);

        Ok(size)
    }
}

impl HostShell {
    pub fn new(term: TerminalEmulator) -> Result<Self> {
        Ok(Self::start_io_loop(term))
    }

    fn start_io_loop(term: TerminalEmulator) -> Self {
        let (mut stdin_tx, stdin_rx) = mpsc::channel::<JsValue>(10);
        let (stdout_tx, mut stdout_rx) = mpsc::channel::<Uint8Array>(10);
        let (mut resize_tx, resize_rx) = mpsc::channel::<JsValue>(10);
        let initial_size = term.size();

        {
            let term = term.clone();

            wasm_bindgen_futures::spawn_local(async move {
                loop {
                    tokio::select! {
                        stdin = term.data() => if let Err(err) = stdin_tx.send(stdin).await {
                            error!("error while receiving terminal data: {:?}", err);
                            break;
                        },
                        stdout = stdout_rx.recv() => {
                            if stdout.is_none() {
                                error!("stdout channel closed");
                                break;
                            }

                            term.write(stdout.unwrap()).await;
                        },
                        size = term.resize() => if let Err(err) = resize_tx.send(size).await {
                            error!("error while receiving resize event: {:?}", err);
                            break;
                        }
                    }
                }
            });
        }

        Self {
            term,
            stdin: Some(stdin_rx),
            stdout: Some(stdout_tx),
            resize: Some(resize_rx),
            initial_size,
        }
    }

    pub async fn println(&mut self, output: &str) {
        let output: Uint8Array = output.as_bytes().into();
        let newline: Uint8Array = "\r\n".as_bytes().into();

        self.term.write(output).await;
        self.term.write(newline).await;
    }

    pub fn stdin(&mut self) -> Result<HostShellStdin> {
        Ok(HostShellStdin::new(self.stdin.take().unwrap()))
    }

    pub fn stdout(&mut self) -> Result<HostShellStdout> {
        Ok(HostShellStdout::new(self.stdout.take().unwrap()))
    }

    pub fn resize_watcher(&mut self) -> Result<HostShellResizeWatcher> {
        Ok(HostShellResizeWatcher::new(self.resize.take().unwrap()))
    }

    pub fn term(&self) -> Result<String> {
        Ok("xterm-256color".to_owned())
    }

    pub fn initial_size(&self) -> Result<(u16, u16)> {
        convert_to_size(self.initial_size.clone())
    }
}

fn convert_to_size(arr: JsValue) -> Result<(u16, u16)> {
    let arr = Uint16Array::new(&arr);
    if arr.length() == 2 {
        Ok((arr.get_index(0), arr.get_index(1)))
    } else {
        Err(Error::msg(format!(
            "invalid vec received from js: {:?}",
            arr
        )))
    }
}
