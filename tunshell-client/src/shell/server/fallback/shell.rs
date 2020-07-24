use super::IoStream;
use crate::shell::{proto::WindowSize, server::shell::Shell};
use anyhow::{Error, Result};
use async_trait::async_trait;
use log::*;
use std::{
    collections::HashMap,
    fs,
    io::Write,
    path::PathBuf,
    process::Stdio,
    sync::{Arc, Mutex},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::{
    process::{self, Command},
    task::JoinHandle,
};

/// In unix environments which do not support pty's we use this
/// bare-bones shell implementation
pub(crate) struct FallbackShell {
    input: IoStream,
    output: IoStream,
    interpreter_task: JoinHandle<Result<()>>,
    state: Arc<Mutex<SharedState>>,
}

struct Interpreter {
    input: IoStream,
    output: IoStream,
    state: Arc<Mutex<SharedState>>,
}

struct SharedState {
    pwd: PathBuf,
    size: WindowSize,
    env: HashMap<String, String>,
    exit_code: Option<u8>,
}

impl FallbackShell {
    pub(in super::super) fn new(_term: &str, size: WindowSize) -> Self {
        let input = IoStream::new();
        let output = IoStream::new();
        let state = Arc::new(Mutex::new(SharedState::new(size)));

        let mut shell = Self {
            input: input.clone(),
            output: output.clone(),
            interpreter_task: Interpreter::start(input, output, Arc::clone(&state)),
            state,
        };

        shell.write_notice().unwrap();

        shell
    }

    fn write_notice(&mut self) -> Result<()> {
        self.output.write("\r\n".as_bytes())?;
        self.output.write("NOTICE: Tunshell is running in a limited environment and is unable to allocate a pty for a real shell. ".as_bytes())?;
        self.output.write(
            "Falling back to a built-in pseudo-shell with very limited functionality".as_bytes(),
        )?;
        self.output.write("\r\n\r\n".as_bytes())?;
        Ok(())
    }
}

#[async_trait]
impl Shell for FallbackShell {
    async fn read(&mut self, buff: &mut [u8]) -> Result<usize> {
        if self.exit_code().is_ok() {
            return Ok(0);
        }

        self.output.read(buff).await.map_err(Error::from)
    }

    async fn write(&mut self, buff: &[u8]) -> Result<()> {
        if self.exit_code().is_ok() {
            return Err(Error::msg("shell has exited"));
        }

        // echo chars
        self.output.write(buff)?;

        self.input.write_all(buff).map_err(Error::from)?;
        Ok(())
    }

    fn resize(&mut self, size: WindowSize) -> Result<()> {
        let mut state = self.state.lock().unwrap();
        state.size = size;
        Ok(())
    }

    fn exit_code(&self) -> Result<u8> {
        let state = self.state.lock().unwrap();
        state
            .exit_code
            .ok_or_else(|| Error::msg("shell has not closed"))
    }
}

impl Drop for FallbackShell {
    fn drop(&mut self) {}
}

impl Interpreter {
    fn start(
        mut input: IoStream,
        mut output: IoStream,
        state: Arc<Mutex<SharedState>>,
    ) -> JoinHandle<Result<()>> {
        let interpreter = Self {
            input: input.clone(),
            output: output.clone(),
            state,
        };

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
                let read = self.input.read(&mut chunk).await?;

                if read == 0 {
                    return Ok(());
                }

                buff.extend_from_slice(&chunk[..read]);
            };

            let exit_code = { self.state.lock().unwrap().exit_code };

            if exit_code.is_some() {
                break;
            }

            let cmd = buff.drain(..i).collect::<Vec<u8>>();
            // remove carriage return
            buff.drain(..1);

            self.run(String::from_utf8(cmd)?).await?;
            self.output.write("\r\n".as_bytes())?;
        }

        Ok(())
    }

    fn write_prompt(&mut self) -> Result<()> {
        let pwd = { self.state.lock().unwrap().pwd.clone() };
        self.output.write(pwd.to_string_lossy().as_bytes())?;
        self.output.write("> ".as_bytes())?;
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
        let mut state = self.state.lock().unwrap();
        let new_pwd = state.pwd.join(dir.clone());

        if !new_pwd.exists() {
            self.output
                .write(format!("no such directory: {}", dir).as_bytes())?;
            return Ok(());
        }

        state.pwd = fs::canonicalize(new_pwd)?;
        debug!("change pwd to: {}", state.pwd.to_string_lossy());
        return Ok(());
    }

    fn exit(&mut self, code: Option<String>) {
        let code = code.map(|i| i.parse::<u8>().unwrap_or(1)).unwrap_or(0);

        let mut state = self.state.lock().unwrap();
        state.exit_code = Some(code);
        debug!("exited with status {}", code);
    }

    async fn run_process(&mut self, program: String, args: Vec<&str>) -> Result<()> {
        let mut cmd = Command::new(program.clone());

        let (pwd, env) = {
            let state = self.state.lock().unwrap();
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
            self.output.write("\r\n".as_bytes())?;
            self.output.write(err.to_string().as_bytes())?;
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
                read = self.input.read(&mut stdin_buff) => {
                    let read = read?;
                    debug!("read {} bytes from shell stdin", read);
                    stdin.write_all(&stdin_buff[..read]).await?;
                    debug!("wrote {} bytes to process stdin", read);
                }
                read = stdout.read(&mut stdout_buff) => {
                    let read = read?;
                    debug!("read {} bytes from process stdout", read);
                    if read == 0 {break;}
                    self.output.write_all(convert_to_crlf(stdout_buff[..read].to_vec()).as_slice())?;
                }
                read = stderr.read(&mut stderr_buff) => {
                    let read = read?;
                    debug!("read {} bytes from process stderr", read);
                    if read == 0 {break;}
                    self.output.write_all(convert_to_crlf(stdout_buff[..read].to_vec()).as_slice())?;
                }
            }
        }

        Ok(())
    }
}

impl SharedState {
    fn new(size: WindowSize) -> Self {
        Self {
            size,
            pwd: std::env::current_dir().unwrap(),
            env: HashMap::new(),
            exit_code: None,
        }
    }
}

// TODO: convert to stream wrapper
fn convert_to_crlf(mut data: Vec<u8>) -> Vec<u8> {
    let mut i = 0;

    while i < data.len() {
        if i > 0 && data[i] == 0x0A && data[i - 1] != 0x0D {
            data.insert(i, 0x0D);
        }

        i += 1;
    }

    data
}

#[cfg(test)]
mod tests {
    // use super::*;
    // use std::time::Duration;
}
