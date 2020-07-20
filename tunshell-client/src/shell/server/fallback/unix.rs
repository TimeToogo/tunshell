use super::IoStream;
use crate::shell::{proto::WindowSize, server::shell::Shell};
use anyhow::{Error, Result};
use async_trait::async_trait;
use log::*;
use std::{
    cmp,
    collections::HashMap,
    io::Write,
    path::PathBuf,
    pin::Pin,
    process::{Command, Stdio},
    task::{Context, Poll, Waker},
};
use tokio::io::AsyncReadExt;

/// In unix environments which do not support pty's we use this
/// bare-bones shell implementation
pub(crate) struct FallbackShell {
    term: String,
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
            term: term.to_owned(),
            pwd: std::env::current_dir().unwrap(),
            input: IoStream::new(),
            output: IoStream::new(),
            size,
            env: HashMap::new(),
            exit_code: None,
        };

        shell.write_prompt().unwrap();

        shell
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
                // remove carraige return
                buff.drain(..1);

                Some(cmd)
            });

            if cmd.is_none() {
                return Ok(());
            }

            self.run(String::from_utf8(cmd.unwrap())?).await?;
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

        let mut cmd = Command::new(program);

        cmd.args(args)
            .current_dir(self.pwd.clone())
            .envs(self.env.iter())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let process = cmd.spawn();

        if let Err(err) = process {
            warn!("spawn failed with: {:?}", err);
            self.output.write(err.to_string().as_bytes())?;
            return Ok(());
        }

        let output = process.unwrap().wait_with_output()?;
        debug!("cmd exited with: {}", output.status);

        self.output.write_all(output.stdout.as_slice())?;
        self.output.write_all(output.stderr.as_slice())?;

        Ok(())
    }
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
