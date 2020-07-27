use anyhow::{Error, Result};
use futures::channel::mpsc;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender};
use futures::stream::StreamExt;
use log::*;
use std::io::{Read, Write};
use std::thread;
use std::thread::JoinHandle;

pub struct HostShellStdin {
}

pub struct HostShellStdout {
}

pub struct HostShellResizeWatcher {
}

pub struct HostShell {}

impl HostShellStdin {
    pub fn new() -> Result<Self> {
        Ok(Self {
        })
    }

    pub async fn read(&mut self, buff: &mut [u8]) -> Result<usize> {
        todo!()
    }
}

impl HostShellStdout {
    pub fn new() -> Result<Self> {
        todo!()
    }

    pub fn write(&mut self, buff: &[u8]) -> Result<()> {
        todo!()
    }
}

impl HostShellResizeWatcher {
    pub fn new() -> Result<Self> {
        todo!()
    }

    pub async fn next(&mut self) -> Result<(u16, u16)> {
        todo!()
    }
}

impl HostShell {
    pub fn new() -> Result<Self> {
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
        Ok("xterm".to_owned())
    }

    pub fn size(&self) -> Result<(u16, u16)> {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    // use super::*;
    // use tokio::runtime::Runtime;
}
