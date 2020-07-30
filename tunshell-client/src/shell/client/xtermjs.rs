use anyhow::{Error, Result};
use futures::channel::mpsc;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender};
use futures::stream::StreamExt;
use log::*;
use std::io::{Read, Write};
use std::thread;
use std::thread::JoinHandle;
use std::sync::{Arc, Mutex};
use crate::TerminalEmulator;

pub struct HostShellStdin {
    term: Arc<Mutex<TerminalEmulator>>
}

pub struct HostShellStdout {
    term: Arc<Mutex<TerminalEmulator>>
}

pub struct HostShellResizeWatcher {
    term: Arc<Mutex<TerminalEmulator>>
}

pub struct HostShell {
    term: Arc<Mutex<TerminalEmulator>>
}

impl HostShellStdin {
    pub fn new(term: Arc<Mutex<TerminalEmulator>>) -> Result<Self> {
        Ok(Self { term })
    }

    pub async fn read(&mut self, buff: &mut [u8]) -> Result<usize> {
        todo!()
    }
}

impl HostShellStdout {
    pub fn new(term: Arc<Mutex<TerminalEmulator>>) -> Result<Self> {
        Ok(Self { term })
    }

    pub fn write(&mut self, buff: &[u8]) -> Result<()> {
        todo!()
    }
}

impl HostShellResizeWatcher {
    pub fn new(term: Arc<Mutex<TerminalEmulator>>) -> Result<Self> {
        Ok(Self { term })
    }

    pub async fn next(&mut self) -> Result<(u16, u16)> {
        todo!()
    }
}

impl HostShell {
    pub fn new(term: TerminalEmulator) -> Result<Self> {
        Ok(Self { 
            term: Arc::new(Mutex::new(term))
         })
    }

    pub fn println(&self, output: &str) {
        let term = self.term.lock().unwrap();
        term.write(output.as_bytes());
        term.write("\r\n".as_bytes());
    }

    pub fn stdin(&self) -> Result<HostShellStdin> {
        HostShellStdin::new(Arc::clone(&self.term))
    }

    pub fn stdout(&self) -> Result<HostShellStdout> {
        HostShellStdout::new(Arc::clone(&self.term))
    }

    pub fn resize_watcher(&self) -> Result<HostShellResizeWatcher> {
        HostShellResizeWatcher::new(Arc::clone(&self.term))
    }

    pub fn term(&self) -> Result<String> {
        Ok("xterm-256color".to_owned())
    }

    pub fn size(&self) -> Result<(u16, u16)> {
        let term = self.term.lock().unwrap();
        
        let size = term.size();

        if size.len() == 2 {
            Ok((size[0], size[1]))
        } else {
            Err(Error::msg(format!("invalid vec received from js: {:?}", size)))
        }
    }
}

