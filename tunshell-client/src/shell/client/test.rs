use anyhow::{Error, Result};
use std::{
    cmp,
    sync::{Arc, Mutex},
    task::{Poll, Waker},
};

pub struct HostShellStdin {
    state: Arc<Mutex<MockStdin>>,
}

struct MockStdin {
    data: Vec<u8>,
    closed: bool,
    wakers: Vec<Waker>,
}

pub struct HostShellStdout {
    state: Arc<Mutex<Vec<u8>>>,
}

pub struct HostShellResizeWatcher {
    state: Arc<Mutex<MockResizeWatcher>>,
}

struct MockResizeWatcher {
    data: Vec<(u16, u16)>,
    closed: bool,
    wakers: Vec<Waker>,
}

pub struct HostShell {
    stdin: Arc<Mutex<MockStdin>>,
    stdout: Arc<Mutex<Vec<u8>>>,
    resize: Arc<Mutex<MockResizeWatcher>>,
}

impl HostShellStdin {
    pub async fn read(&mut self, buff: &mut [u8]) -> Result<usize> {
        futures::future::poll_fn(|cx| {
            let mut stdin = self.state.lock().unwrap();

            if stdin.data.len() == 0 {
                if stdin.closed {
                    return Poll::Ready(Ok(0));
                };

                stdin.wakers.push(cx.waker().clone());
                return Poll::Pending;
            }

            let len = cmp::min(buff.len(), stdin.data.len());
            buff[..len].copy_from_slice(&stdin.data[..len]);
            stdin.data.drain(..len);
            Poll::Ready(Ok(len))
        })
        .await
    }
}

impl HostShellStdout {
    pub async fn write(&mut self, buff: &[u8]) -> Result<()> {
        let mut stdout = self.state.lock().unwrap();
        stdout.extend_from_slice(buff);

        Ok(())
    }
}

impl HostShellResizeWatcher {
    pub async fn next(&mut self) -> Result<(u16, u16)> {
        futures::future::poll_fn(|cx| {
            let mut resize = self.state.lock().unwrap();

            if resize.data.len() == 0 {
                if resize.closed {
                    return Poll::Ready(Err(Error::msg("resize watcher closed")));
                };

                resize.wakers.push(cx.waker().clone());
                return Poll::Pending;
            }

            Poll::Ready(Ok(resize.data.remove(0)))
        })
        .await
    }
}

impl HostShell {
    pub fn new() -> Result<Self> {
        Ok(Self {
            stdin: Arc::new(Mutex::new(MockStdin {
                data: vec![],
                closed: false,
                wakers: vec![],
            })),
            stdout: Arc::new(Mutex::new(vec![])),
            resize: Arc::new(Mutex::new(MockResizeWatcher {
                data: vec![],
                closed: false,
                wakers: vec![],
            })),
        })
    }

    pub async fn println(&self, output: &str) {
        let mut stdout = self.stdout.lock().unwrap();
        stdout.extend_from_slice(output.as_bytes());
    }

    pub fn enable_raw_mode(&mut self) -> Result<()> {
        Ok(())
    }

    pub fn disable_raw_mode(&mut self) -> Result<()> {
        Ok(())
    }

    pub fn stdin(&self) -> Result<HostShellStdin> {
        Ok(HostShellStdin {
            state: Arc::clone(&self.stdin),
        })
    }

    pub fn stdout(&self) -> Result<HostShellStdout> {
        Ok(HostShellStdout {
            state: Arc::clone(&self.stdout),
        })
    }

    pub fn resize_watcher(&self) -> Result<HostShellResizeWatcher> {
        Ok(HostShellResizeWatcher {
            state: Arc::clone(&self.resize),
        })
    }

    pub fn term(&self) -> Result<String> {
        Ok("MOCK".to_owned())
    }

    pub async fn size(&self) -> Result<(u16, u16)> {
        Ok((100, 100))
    }

    pub fn write_to_stdin(&self, buf: &[u8]) {
        let mut stdin = self.stdin.lock().unwrap();
        stdin.data.extend_from_slice(buf);
        stdin.wakers.drain(..).map(|i| i.wake()).for_each(drop);
    }

    pub fn drain_stdout(&self) -> Vec<u8> {
        let mut stdin = self.stdout.lock().unwrap();

        stdin.drain(..).collect()
    }

    pub fn send_resize(&self, size: (u16, u16)) {
        let mut resize = self.resize.lock().unwrap();
        resize.data.push(size);
        resize.wakers.drain(..).map(|i| i.wake()).for_each(drop);
    }
}

impl Clone for HostShell {
    fn clone(&self) -> Self {
        Self {
            stdin: Arc::clone(&self.stdin),
            stdout: Arc::clone(&self.stdout),
            resize: Arc::clone(&self.resize),
        }
    }
}
