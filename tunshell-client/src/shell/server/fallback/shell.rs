use super::IoStream;
use crate::shell::{proto::WindowSize, server::shell::Shell};
use anyhow::{Error, Result};
use async_trait::async_trait;
use log::*;
use std::{collections::HashMap, fs, io::Write, path::PathBuf, process::Stdio};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::{self, Command};

/// In unix environments which do not support pty's we use this
/// bare-bones shell implementation
pub(crate) struct FallbackShell {
    _term: String,
    pwd: PathBuf,
    input: IoStream,
    output: IoStream,
    size: WindowSize,
    env: HashMap<String, String>,
    exit_code: Option<u8>,
}

impl FallbackShell {
    pub(in super::super) fn new(term: &str, size: WindowSize) -> Self {
        let mut shell = Self {
            _term: term.to_owned(),
            pwd: std::env::current_dir().unwrap(),
            input: IoStream::new(),
            output: IoStream::new(),
            size,
            env: HashMap::new(),
            exit_code: None,
        };

        shell.write_notice().unwrap();
        shell.write_prompt().unwrap();

        shell
    }

    fn write_notice(&mut self) -> Result<()> {
        self.output.write("\r\n".as_bytes())?;
        self.output.write("NOTICE: Tunshell is running in a limited environment and is unable to allocate a pty for a real shell. ".as_bytes())?;
        self.output.write(
            "Falling back to a built-in pseudo-shell with very limited function".as_bytes(),
        )?;
        self.output.write("\r\n\r\n".as_bytes())?;
        Ok(())
    }

    fn write_prompt(&mut self) -> Result<()> {
        self.output.write(self.pwd.to_string_lossy().as_bytes())?;
        self.output.write("> ".as_bytes())?;
        Ok(())
    }

    async fn process_cmds(&mut self) -> Result<()> {
        loop {
            // find index of carriage return
            let cmd = self.input.process_buff(|buff| {
                let i = buff.iter().position(|i| *i == 0x0D);

                if i.is_none() {
                    return None;
                }

                let cmd = buff.drain(..i.unwrap()).collect::<Vec<u8>>();
                // remove carriage return
                buff.drain(..1);

                Some(cmd)
            });

            if cmd.is_none() {
                return Ok(());
            }

            self.run(String::from_utf8(cmd.unwrap())?).await?;
            self.output.write("\r\n".as_bytes())?;
            self.write_prompt()?;
        }
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
        let new_pwd = self.pwd.join(dir.clone());

        if !new_pwd.exists() {
            self.output
                .write(format!("no such directory: {}", dir).as_bytes())?;
            return Ok(());
        }

        self.pwd = fs::canonicalize(new_pwd)?;
        debug!("change pwd to: {}", self.pwd.to_string_lossy());
        return Ok(());
    }

    fn exit(&mut self, code: Option<String>) {
        let code = code.map(|i| i.parse::<u8>().unwrap_or(1)).unwrap_or(0);

        self.exit_code = Some(code);
        debug!("exited with status {}", code);
    }

    async fn run_process(&mut self, program: String, args: Vec<&str>) -> Result<()> {
        let mut cmd = Command::new(program);

        cmd.args(args)
            .current_dir(self.pwd.clone())
            .envs(self.env.iter())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let process = cmd.spawn();

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
                    stdin.write(&stdin_buff[..read?]).await?;
                }
                read = stdout.read(&mut stdout_buff) => {
                    let read = read?;
                    if read == 0 {break;}
                    self.output.write_all(convert_to_crlf(stdout_buff[..read].to_vec()).as_slice())?;
                }
                read = stderr.read(&mut stderr_buff) => {
                    let read = read?;
                    if read == 0 {break;}
                    self.output.write_all(convert_to_crlf(stdout_buff[..read].to_vec()).as_slice())?;
                }
            }
        }

        Ok(())
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

#[async_trait]
impl Shell for FallbackShell {
    async fn read(&mut self, buff: &mut [u8]) -> Result<usize> {
        if self.exit_code.is_some() {
            return Ok(0);
        }

        self.output.read(buff).await.map_err(Error::from)
    }

    async fn write(&mut self, buff: &[u8]) -> Result<()> {
        if self.exit_code.is_some() {
            return Err(Error::msg("shell has exited"));
        }

        // echo chars
        self.output.write(buff)?;

        self.input.write_all(buff).map_err(Error::from)?;
        self.process_cmds().await?;
        Ok(())
    }

    fn resize(&mut self, size: WindowSize) -> Result<()> {
        self.size = size;
        Ok(())
    }

    fn exit_code(&self) -> Result<u8> {
        self.exit_code
            .ok_or_else(|| Error::msg("shell has not closed"))
    }
}

impl Drop for FallbackShell {
    fn drop(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
}
