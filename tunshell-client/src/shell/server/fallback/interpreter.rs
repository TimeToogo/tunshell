use super::super::{get_default_shell, DefaultShell};
use super::{SharedState, Token};
use anyhow::{Error, Result};
use log::*;
use std::{cmp, fs, io::Write, iter, process::Stdio};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::{
    process::{self, Command},
    task::JoinHandle,
};

/// Provides (very) basic support for VT100-style line editing
pub(super) struct Interpreter {
    state: SharedState,
    line_buff: Vec<u8>,
    history: Vec<String>,
    cursor_pos: usize,
    history_pos: usize,
    delegate_shell: Option<DefaultShell>,
}

impl Interpreter {
    pub(super) fn start(state: SharedState) -> JoinHandle<Result<()>> {
        let interpreter = Self {
            state,
            line_buff: vec![],
            history: vec![],
            cursor_pos: 0,
            history_pos: 0,
            delegate_shell: get_default_shell(None).ok().filter(|i| i.can_delegate()),
        };

        tokio::spawn(interpreter.start_loop())
    }

    async fn start_loop(mut self) -> Result<()> {
        while self.state.exit_code().is_none() {
            self.write_prompt()?;

            let cmd = self.read_line().await?;

            self.run(cmd).await?;
        }

        Ok(())
    }

    async fn read_line(&mut self) -> Result<String> {
        let line = loop {
            let token = self.state.read_input().await?;

            match token {
                Token::Bytes(bytes) => self.insert_bytes(bytes)?,
                Token::UpArrow => self.previous_in_history()?,
                Token::LeftArrow => self.retract_cursor(1)?,
                Token::RightArrow => self.advance_cursor(1)?,
                Token::DownArrow => self.next_in_history()?,
                Token::Backspace => self.backspace()?,
                Token::ControlC => self.interrupt_line()?,
                Token::Enter => break self.handle_new_line()?,
            }
        };

        let cmd = String::from_utf8(line)?;

        self.history.push(cmd.clone());
        self.cursor_pos = 0;
        self.history_pos = self.history.len();

        Ok(cmd)
    }

    fn insert_bytes(&mut self, bytes: Vec<u8>) -> Result<()> {
        if self.cursor_pos == self.line_buff.len() {
            self.line_buff.extend_from_slice(bytes.as_slice());

            // Echo characters back to client terminal
            let mut state = self.state.inner.lock().unwrap();
            state.output.write_all(bytes.as_slice())?;

            self.cursor_pos += bytes.len();
            debug!("added {} bytes to line buffer", bytes.len());
        } else {
            self.refresh_line(|this| {
                this.line_buff = [
                    &this.line_buff[..this.cursor_pos],
                    bytes.as_slice(),
                    &this.line_buff[this.cursor_pos..],
                ]
                .concat();

                this.cursor_pos += bytes.len();
            })?;
            debug!("inserted {} bytes to into line buffer", bytes.len());
        }

        Ok(())
    }

    fn advance_cursor(&mut self, offset: usize) -> Result<()> {
        let advance_chars = cmp::min(offset, self.line_buff.len() - self.cursor_pos);

        let mut state = self.state.inner.lock().unwrap();

        state.output.write_all(
            Token::RightArrow
                .to_bytes()
                .repeat(advance_chars)
                .as_slice(),
        )?;

        self.cursor_pos += advance_chars;
        debug!("advanced cursor to position {}", self.cursor_pos);

        Ok(())
    }

    fn retract_cursor(&mut self, offset: usize) -> Result<()> {
        let retract_chars = cmp::min(offset, self.cursor_pos);
        let mut state = self.state.inner.lock().unwrap();

        state
            .output
            .write_all(Token::LeftArrow.to_bytes().repeat(retract_chars).as_slice())?;

        self.cursor_pos -= retract_chars;
        debug!("retracted cursor to position {}", self.cursor_pos);

        Ok(())
    }

    fn interrupt_line(&mut self) -> Result<()> {
        {
            let mut state = self.state.inner.lock().unwrap();
            state.output.write_all("\r\n".as_bytes())?;
        }

        self.write_prompt()?;
        self.line_buff = vec![];
        self.cursor_pos = 0;
        debug!("interrupted, starting new line");

        Ok(())
    }

    fn backspace(&mut self) -> Result<()> {
        if self.cursor_pos == 0 {
            return Ok(());
        }

        if self.cursor_pos == self.line_buff.len() {
            let mut state = self.state.inner.lock().unwrap();
            self.line_buff.pop().unwrap();
            state.output.write_all(
                [
                    Token::LeftArrow.to_bytes(),
                    &[b' '],
                    Token::LeftArrow.to_bytes(),
                ]
                .concat()
                .as_slice(),
            )?;
            self.cursor_pos -= 1;
        } else {
            self.refresh_line(|this| {
                this.line_buff.remove(this.cursor_pos - 1);
                this.cursor_pos -= 1;
            })?;
        }

        Ok(())
    }

    fn previous_in_history(&mut self) -> Result<()> {
        if self.history_pos == 0 || self.history.len() == 0 {
            return Ok(());
        }

        self.refresh_line(|this| {
            this.history_pos -= 1;
            this.line_buff = this.history[this.history_pos].as_bytes().to_vec();
            this.cursor_pos = this.line_buff.len();
        })?;

        debug!("retreated to position {} in history", self.history_pos);

        Ok(())
    }

    fn next_in_history(&mut self) -> Result<()> {
        if self.history_pos >= self.history.len() {
            return Ok(());
        }

        self.refresh_line(|this| {
            this.history_pos += 1;

            this.line_buff = if this.history_pos == this.history.len() {
                vec![]
            } else {
                this.history[this.history_pos].as_bytes().to_vec()
            };

            this.cursor_pos = this.line_buff.len();
        })?;

        debug!("advanced to {} in history", self.history_pos);

        Ok(())
    }

    fn refresh_line(&mut self, mut update_cb: impl FnMut(&mut Self) -> ()) -> Result<()> {
        let original_len = self.line_buff.len();
        let original_pos = self.cursor_pos;

        // modify line
        update_cb(self);

        let mut state = self.state.inner.lock().unwrap();
        let refresh_len = cmp::max(original_len, self.line_buff.len());

        // move cursor to start
        state
            .output
            .write_all(Token::LeftArrow.to_bytes().repeat(original_pos).as_slice())?;
        // write new line
        state.output.write_all(
            self.line_buff
                .iter()
                .copied()
                .chain(iter::repeat(b' '))
                .take(refresh_len)
                .collect::<Vec<u8>>()
                .as_slice(),
        )?;
        // revert cursor to correct pos
        state.output.write_all(
            Token::LeftArrow
                .to_bytes()
                .repeat(refresh_len - self.cursor_pos)
                .as_slice(),
        )?;

        Ok(())
    }

    fn handle_new_line(&mut self) -> Result<Vec<u8>> {
        let mut state = self.state.inner.lock().unwrap();

        state.output.write_all("\r\n".as_bytes())?;
        Ok(self.line_buff.drain(..).collect())
    }

    fn write_prompt(&mut self) -> Result<()> {
        let mut state = self.state.inner.lock().unwrap();
        let pwd = state.pwd.clone();
        state.output.write(
            pwd.file_name()
                .map(|i| i.to_string_lossy().to_string())
                .unwrap_or("/".to_owned())
                .as_bytes(),
        )?;
        state.output.write("> ".as_bytes())?;
        Ok(())
    }

    async fn run(&mut self, cmd: String) -> Result<()> {
        if self.state.exit_code().is_some() {
            return Err(Error::msg("shell has exited"));
        }

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
        state.input.shutdown();
        state.output.shutdown();
        debug!("exited with status {}", code);
    }

    async fn run_process(&mut self, program: String, args: Vec<&str>) -> Result<()> {
        let mut cmd = self.create_command(program.as_str(), args);
        let process = cmd.spawn();
        debug!("spawned new process {}", program);

        if let Err(err) = process {
            warn!("spawn failed with: {:?}", err);
            let mut state = self.state.inner.lock().unwrap();
            state.output.write_all(err.to_string().as_bytes())?;
            state.output.write_all("\r\n".as_bytes())?;
            return Ok(());
        }

        let mut process = process.unwrap();
        self.stream_process_io(&mut process).await?;

        let exit_status = process.await?;
        debug!("cmd exited with: {}", exit_status);

        Ok(())
    }

    fn create_command(&self, program: &str, args: Vec<&str>) -> Command {
        let mut cmd = if let Some(delegate_shell) = self.delegate_shell.as_ref() {
            // In the case there is a shell on the system we will delegate all commands
            // to that shell for execution to enable near full shell command capability
            let path = delegate_shell.path.clone();
            let args = delegate_shell
                .get_execute_command_args(program, args.iter().cloned().collect())
                .unwrap();
            let mut cmd = Command::new(path.clone());
            cmd.args(args.clone());

            debug!("created command: {} {:?}", path, args);

            cmd
        } else {
            // In the case of no shells available on the system we fallback to executing
            // the programs directly
            let mut cmd = Command::new(program.to_owned());
            cmd.args(args.clone());

            debug!("created command: {} {:?}", program.to_owned(), args);

            cmd
        };

        let (pwd, env) = {
            let state = self.state.inner.lock().unwrap();
            (state.pwd.clone(), state.env.clone())
        };

        cmd.current_dir(pwd.clone())
            .envs(env.iter())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        cmd
    }

    async fn stream_process_io(&mut self, process: &mut process::Child) -> Result<()> {
        let mut stdin = process.stdin.take().unwrap();
        let mut stdout = process.stdout.take().unwrap();
        let mut stderr = process.stderr.take().unwrap();

        let mut stdout_buff = [0u8; 1024];
        let mut stderr_buff = [0u8; 1024];

        loop {
            tokio::select! {
                token = self.state.read_input() => {
                    match token? {
                        Token::ControlC => {
                            process.kill()?; // TODO: replace with SIGTERM on unix
                            debug!("killed child process");
                        },
                        data @ _ => {
                            let data = data.to_bytes();
                            stdin.write_all(data).await?;
                            debug!("wrote {} bytes to process stdin", data.len());
                        }
                    }
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
                    let mut state = self.state.inner.lock().unwrap();
                    state.output.write_all(&stderr_buff[..read])?;
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell::proto::WindowSize;
    use std::time::Duration;
    use tokio::{runtime::Runtime, time::delay_for};

    fn init_interpreter() -> SharedState {
        let state = SharedState::new(WindowSize(100, 100));
        {
            let mut state = state.inner.lock().unwrap();
            state.pwd = "/".parse().unwrap();
        }
        Interpreter::start(state.clone());

        state
    }

    fn write_input(state: &mut SharedState, data: &[u8]) {
        let mut state = state.inner.lock().unwrap();
        state.input.write_all(data).unwrap();
    }

    async fn read_to_end(state: &mut SharedState) -> Vec<u8> {
        let mut output = vec![];
        let mut buff = [0u8; 1024];

        loop {
            let read = state.read_output(&mut buff).await.unwrap();

            if read == 0 {
                break;
            }

            output.extend_from_slice(&buff[..read]);
        }

        output
    }

    #[test]
    fn test_run_echo() {
        Runtime::new().unwrap().block_on(async {
            let mut state = init_interpreter();

            write_input(&mut state, "echo test\r".as_bytes());

            // Let process execute
            delay_for(Duration::from_millis(100)).await;

            write_input(&mut state, "exit\r".as_bytes());

            let output = read_to_end(&mut state).await;
            let output = String::from_utf8(output).unwrap();

            assert_eq!(
                output.as_str(),
                concat!(
                    "/> echo test\r\n", //
                    "test\r\n",
                    "/> exit\r\n"
                )
            );
        });
    }

    #[test]
    fn test_modifying_line_buffer_from_end_of_text() {
        Runtime::new().unwrap().block_on(async {
            let mut state = init_interpreter();

            write_input(&mut state, "echo hello".as_bytes());

            write_input(&mut state, Token::Backspace.to_bytes().repeat(5).as_slice());

            write_input(&mut state, "goodbye\r".as_bytes());

            // Let process execute
            delay_for(Duration::from_millis(100)).await;

            write_input(&mut state, "exit\r".as_bytes());

            let output = read_to_end(&mut state).await;
            let output = String::from_utf8(output).unwrap();

            let echo_output = output.split("\r\n").collect::<Vec<&str>>()[1];

            assert_eq!(echo_output, "goodbye");
        });
    }

    #[test]
    fn test_modifying_line_buffer_in_middle() {
        Runtime::new().unwrap().block_on(async {
            let mut state = init_interpreter();

            write_input(&mut state, "echo hello".as_bytes());

            write_input(&mut state, Token::LeftArrow.to_bytes().repeat(4).as_slice());

            write_input(
                &mut state,
                Token::RightArrow.to_bytes().repeat(2).as_slice(),
            );

            write_input(&mut state, Token::Backspace.to_bytes().repeat(3).as_slice());

            write_input(&mut state, "buffa\r".as_bytes());

            // Let process execute
            delay_for(Duration::from_millis(100)).await;

            write_input(&mut state, "exit\r".as_bytes());

            let output = read_to_end(&mut state).await;
            let output = String::from_utf8(output).unwrap();

            let echo_output = output.split("\r\n").collect::<Vec<&str>>()[1];

            assert_eq!(echo_output, "buffalo");
        });
    }

    #[test]
    fn test_interrupt_line() {
        Runtime::new().unwrap().block_on(async {
            let mut state = init_interpreter();

            write_input(&mut state, "echo test".as_bytes());

            write_input(&mut state, Token::ControlC.to_bytes());

            write_input(&mut state, "exit\r".as_bytes());

            let output = read_to_end(&mut state).await;
            let output = String::from_utf8(output).unwrap();

            assert_eq!(
                output.as_str(),
                concat!(
                    "/> echo test\r\n", //
                    "/> exit\r\n"
                )
            );
        });
    }

    #[test]
    fn test_interrupt_running_process() {
        Runtime::new().unwrap().block_on(async {
            let mut state = init_interpreter();

            write_input(&mut state, "/bin/sh\r".as_bytes());

            delay_for(Duration::from_millis(100)).await;

            // Send interrupt to process
            write_input(&mut state, Token::ControlC.to_bytes());

            delay_for(Duration::from_millis(100)).await;

            write_input(&mut state, "exit\r".as_bytes());

            let output = read_to_end(&mut state).await;
            let output = String::from_utf8(output).unwrap();

            assert_eq!(
                output.as_str(),
                concat!(
                    "/> /bin/sh\r\n", //
                    "/> exit\r\n"
                )
            );
        });
    }

    #[test]
    fn test_previous_in_history() {
        Runtime::new().unwrap().block_on(async {
            let mut state = init_interpreter();

            write_input(&mut state, "echo first\r".as_bytes());

            // Wait for process to finish
            delay_for(Duration::from_millis(100)).await;

            write_input(&mut state, "echo second\r".as_bytes());

            // Wait for process to finish
            delay_for(Duration::from_millis(100)).await;

            // Navigate to and re-execute first command
            write_input(&mut state, Token::UpArrow.to_bytes().repeat(2).as_slice());
            write_input(&mut state, Token::Enter.to_bytes());

            delay_for(Duration::from_millis(100)).await;

            write_input(&mut state, "exit\r".as_bytes());

            let output = read_to_end(&mut state).await;
            let output = String::from_utf8(output).unwrap();

            let echo_output = output
                .split("\r\n")
                .filter(|i| !i.starts_with("/>") && i.len() > 0)
                .collect::<Vec<&str>>();

            assert_eq!(echo_output, vec!["first", "second", "first"]);
        });
    }

    #[test]
    fn test_next_in_history() {
        Runtime::new().unwrap().block_on(async {
            let mut state = init_interpreter();

            write_input(&mut state, "echo first\r".as_bytes());

            // Wait for process to finish
            delay_for(Duration::from_millis(100)).await;

            write_input(&mut state, "echo second\r".as_bytes());

            // Wait for process to finish
            delay_for(Duration::from_millis(100)).await;

            // Navigate to and re-execute second command
            write_input(&mut state, Token::UpArrow.to_bytes().repeat(2).as_slice());
            write_input(&mut state, Token::DownArrow.to_bytes());
            write_input(&mut state, Token::Enter.to_bytes());

            delay_for(Duration::from_millis(100)).await;

            write_input(&mut state, "exit\r".as_bytes());

            let output = read_to_end(&mut state).await;
            let output = String::from_utf8(output).unwrap();

            let echo_output = output
                .split("\r\n")
                .filter(|i| !i.starts_with("/>") && i.len() > 0)
                .collect::<Vec<&str>>();

            assert_eq!(echo_output, vec!["first", "second", "second"]);
        });
    }
}
