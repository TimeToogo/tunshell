use super::{ByteChannel, OutputStream};
use crate::shell::{proto::WindowSize, server::shell::Shell};
use anyhow::{Error, Result};
use async_trait::async_trait;
use futures::{Future, TryFuture};
use log::*;
use std::{
    collections::HashMap,
    fs,
    io::Write,
    path::PathBuf,
    pin::Pin,
    process::Stdio,
    sync::{Arc, Mutex},
};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::{
    process::{self, Command},
    task::JoinHandle,
};

/// In unix environments which do not support pty's we use this
/// bare-bones shell implementation
pub(crate) struct FallbackShell {
    interpreter_task: JoinHandle<Result<()>>,
    state: SharedState,
}

struct Interpreter {
    state: SharedState,
}

#[derive(Clone)]
struct SharedState {
    inner: Arc<Mutex<Inner>>,
}

struct Inner {
    input: ByteChannel,
    output: OutputStream,
    pwd: PathBuf,
    size: WindowSize,
    env: HashMap<String, String>,
    exit_code: Option<u8>,
}

impl FallbackShell {
    pub(in super::super) fn new(_term: &str, size: WindowSize) -> Self {
        let state = SharedState::new(size);

        let mut shell = Self {
            interpreter_task: Interpreter::start(state.clone()),
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
        state.output.write(buff)?;

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

impl Interpreter {
    fn start(state: SharedState) -> JoinHandle<Result<()>> {
        let interpreter = Self { state };

        tokio::spawn(interpreter.start_loop())
    }

    async fn start_loop(mut self) -> Result<()> {
        let mut buff = Vec::<u8>::new();
        loop {
            self.write_prompt()?;

            // find index of carriage return
            let i = loop {
                let i = buff.iter().position(|i| *i == 0x0D);

                if i.is_some() {
                    break i.unwrap();
                }

                let mut chunk = [0u8; 1024];
                let read = self.state.read_input(&mut chunk).await?;

                if read == 0 {
                    return Ok(());
                }

                buff.extend_from_slice(&chunk[..read]);
            };

            let exit_code = { self.state.inner.lock().unwrap().exit_code };

            if exit_code.is_some() {
                break;
            }

            let cmd = buff.drain(..i).collect::<Vec<u8>>();
            // remove carriage return
            buff.drain(..1);

            self.run(String::from_utf8(cmd)?).await?;

            {
                let mut state = self.state.inner.lock().unwrap();
                state.output.write("\r\n".as_bytes())?;
            }
        }

        Ok(())
    }

    fn write_prompt(&mut self) -> Result<()> {
        let mut state = self.state.inner.lock().unwrap();
        let pwd = state.pwd.clone();
        state.output.write(pwd.to_string_lossy().as_bytes())?;
        state.output.write("> ".as_bytes())?;
        Ok(())
    }

    async fn run(&mut self, cmd: String) -> Result<()> {
        if cmd.len() == 0 {
            return Ok(());
        }

        debug!("running cmd: {}", cmd);

        let mut iter = cmd.split_whitespace();

        let program = iter.next().unwrap().to_owned();
        let args = iter.collect::<Vec<&str>>();

        match (program.as_str(), args.len()) {
            ("exit", _) => self.exit(args.first().map(|i| i.to_string())),
            ("cd", 1) => self.change_pwd(args.first().unwrap().to_string())?,
            _ => self.run_process(program, args).await?,
        }

        Ok(())
    }

    fn change_pwd(&mut self, dir: String) -> Result<()> {
        let mut state = self.state.inner.lock().unwrap();
        let new_pwd = state.pwd.join(dir.clone());

        if !new_pwd.exists() {
            state
                .output
                .write(format!("no such directory: {}", dir).as_bytes())?;
            return Ok(());
        }

        state.pwd = fs::canonicalize(new_pwd)?;
        debug!("change pwd to: {}", state.pwd.to_string_lossy());
        return Ok(());
    }

    fn exit(&mut self, code: Option<String>) {
        let code = code.map(|i| i.parse::<u8>().unwrap_or(1)).unwrap_or(0);

        let mut state = self.state.inner.lock().unwrap();
        state.exit_code = Some(code);
        debug!("exited with status {}", code);
    }

    async fn run_process(&mut self, program: String, args: Vec<&str>) -> Result<()> {
        let mut cmd = Command::new(program.clone());

        let (pwd, env) = {
            let state = self.state.inner.lock().unwrap();
            (state.pwd.clone(), state.env.clone())
        };

        cmd.args(args)
            .current_dir(pwd.clone())
            .envs(env.iter())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let process = cmd.spawn();
        debug!("spawned new process {}", program);

        if let Err(err) = process {
            warn!("spawn failed with: {:?}", err);
            let mut state = self.state.inner.lock().unwrap();
            state.output.write("\r\n".as_bytes())?;
            state.output.write(err.to_string().as_bytes())?;
            return Ok(());
        }

        let mut process = process.unwrap();
        self.stream_process_io(&mut process).await?;

        let exit_status = process.await?;
        debug!("cmd exited with: {}", exit_status);

        Ok(())
    }

    async fn stream_process_io(&mut self, process: &mut process::Child) -> Result<()> {
        let mut stdin = process.stdin.take().unwrap();
        let mut stdout = process.stdout.take().unwrap();
        let mut stderr = process.stderr.take().unwrap();

        let mut stdin_buff = [0u8; 1024];
        let mut stdout_buff = [0u8; 1024];
        let mut stderr_buff = [0u8; 1024];

        loop {
            tokio::select! {
                read = self.state.read_input(&mut stdin_buff) => {
                    let read = read?;
                    debug!("read {} bytes from shell stdin", read);
                    stdin.write_all(&stdin_buff[..read]).await?;
                    debug!("wrote {} bytes to process stdin", read);
                }
                read = stdout.read(&mut stdout_buff) => {
                    let read = read?;
                    debug!("read {} bytes from process stdout", read);
                    if read == 0 {break;}
                    let mut state = self.state.inner.lock().unwrap();
                    state.output.write_all(&stdout_buff[..read])?;
                }
                read = stderr.read(&mut stderr_buff) => {
                    let read = read?;
                    debug!("read {} bytes from process stderr", read);
                    if read == 0 {break;}
                    let mut state = self.state.inner.lock().unwrap();
                    state.output.write_all(&stderr_buff[..read])?;
                }
            }
        }

        Ok(())
    }
}

impl SharedState {
    fn new(size: WindowSize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                size,
                input: ByteChannel::new(),
                output: OutputStream::new(),
                pwd: std::env::current_dir().unwrap(),
                env: HashMap::new(),
                exit_code: None,
            })),
        }
    }

    fn read_input<'a>(
        &'a mut self,
        buff: &'a mut [u8],
    ) -> impl Future<Output = Result<usize>> + 'a {
        futures::future::poll_fn(move |cx| {
            let mut state = self.inner.lock().unwrap();
            let result = Pin::new(&mut state.input).poll_read(cx, buff);

            result.map_err(Error::from)
        })
    }

    fn read_output<'a>(
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
