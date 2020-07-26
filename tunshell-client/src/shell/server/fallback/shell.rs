use super::{InputStream, Interpreter, OutputStream, Token};
use crate::shell::{proto::WindowSize, server::shell::Shell};
use anyhow::{Error, Result};
use async_trait::async_trait;
use futures::{Future, Stream};
use std::{
    collections::HashMap,
    io::Write,
    path::PathBuf,
    pin::Pin,
    sync::{Arc, Mutex},
};
use tokio::io::AsyncRead;
use tokio::task::JoinHandle;

/// In unix environments which do not support pty's we use this
/// bare-bones shell implementation
pub(crate) struct FallbackShell {
    _interpreter_task: JoinHandle<Result<()>>,
    state: SharedState,
}

#[derive(Clone)]
pub(super) struct SharedState {
    pub(super) inner: Arc<Mutex<Inner>>,
}

pub(super) struct Inner {
    pub(super) input: InputStream,
    pub(super) output: OutputStream,
    pub(super) pwd: PathBuf,
    pub(super) env: HashMap<String, String>,
    pub(super) size: WindowSize,
    pub(super) exit_code: Option<u8>,
}

impl FallbackShell {
    pub(in super::super) fn new(_term: &str, size: WindowSize) -> Self {
        let state = SharedState::new(size);

        let mut shell = Self {
            _interpreter_task: Interpreter::start(state.clone()),
            state,
        };

        shell.write_notice().unwrap();

        shell
    }

    fn write_notice(&mut self) -> Result<()> {
        let mut state = self.state.inner.lock().unwrap();

        state.output.write("\r\n".as_bytes())?;
        state.output.write("NOTICE: Tunshell is running in a limited environment and is unable to allocate a pty for a real shell. ".as_bytes())?;
        state.output.write(
            "Falling back to a built-in pseudo-shell with very limited functionality".as_bytes(),
        )?;
        state.output.write("\r\n\r\n".as_bytes())?;

        Ok(())
    }
}

#[async_trait]
impl Shell for FallbackShell {
    async fn read(&mut self, buff: &mut [u8]) -> Result<usize> {
        if self.exit_code().is_ok() {
            return Ok(0);
        }

        self.state.read_output(buff).await
    }

    async fn write(&mut self, buff: &[u8]) -> Result<()> {
        if self.exit_code().is_ok() {
            return Err(Error::msg("shell has exited"));
        }

        // echo chars
        let mut state = self.state.inner.lock().unwrap();
        state.input.write_all(buff).map_err(Error::from)?;
        Ok(())
    }

    fn resize(&mut self, size: WindowSize) -> Result<()> {
        let mut state = self.state.inner.lock().unwrap();
        state.size = size;
        Ok(())
    }

    fn exit_code(&self) -> Result<u8> {
        let state = self.state.inner.lock().unwrap();
        state
            .exit_code
            .ok_or_else(|| Error::msg("shell has not closed"))
    }
}

impl Drop for FallbackShell {
    fn drop(&mut self) {}
}

impl SharedState {
    pub(super) fn new(size: WindowSize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                size,
                input: InputStream::new(),
                output: OutputStream::new(),
                pwd: std::env::current_dir().unwrap(),
                env: HashMap::new(),
                exit_code: None,
            })),
        }
    }

    pub(super) fn exit_code(&self) -> Option<u8> {
        self.inner.lock().unwrap().exit_code
    }

    pub(super) fn read_input<'a>(&'a mut self) -> impl Future<Output = Result<Token>> + 'a {
        futures::future::poll_fn(move |cx| {
            let mut state = self.inner.lock().unwrap();
            let result = Pin::new(&mut state.input).poll_next(cx);

            result.map(|i| i.ok_or_else(|| Error::msg("input stream ended unexpectedly")))
        })
    }

    pub(super) fn read_output<'a>(
        &'a mut self,
        buff: &'a mut [u8],
    ) -> impl Future<Output = Result<usize>> + 'a {
        futures::future::poll_fn(move |cx| {
            let mut state = self.inner.lock().unwrap();
            let result = Pin::new(&mut state.output).poll_read(cx, buff);

            result.map_err(Error::from)
        })
    }
}

#[cfg(test)]
mod tests {
    // use super::*;
    // use std::time::Duration;
}
