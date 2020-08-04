use anyhow::{Context, Error, Result};
use log::*;
use std::{fs, path::PathBuf};

#[derive(Clone, PartialEq, Debug)]
pub(super) struct DefaultShell {
    pub(super) path: String,
    pub(super) args: Vec<String>,
}

impl DefaultShell {
    pub(super) fn new(path: String) -> Self {
        Self { path, args: vec![] }
    }

    fn file_name(&self) -> Result<String> {
        let path = self.path.to_lowercase().parse::<PathBuf>()?;
        let shell = path.file_name().ok_or_else(|| Error::msg("no file name"))?;
        let shell = shell
            .to_str()
            .ok_or_else(|| Error::msg("invalid file name"))?;

        Ok(shell.to_owned())
    }

    pub(super) fn validate(&self) -> Result<()> {
        if !PathBuf::from(self.path.clone()).exists() {
            debug!("cannot find default shell program: {}", self.path);
            return Err(Error::msg(format!(
                "cannot find default shell program: {}",
                self.path
            )));
        }

        return Ok(());
    }

    pub(super) fn can_delegate(&self) -> bool {
        self.get_execute_command_args("", vec![])
            .map_err(|err| debug!("cannot delegate shell: {}", err))
            .is_ok()
    }

    pub(super) fn get_execute_command_args(
        &self,
        program: &str,
        args: Vec<&str>,
    ) -> Result<Vec<String>> {
        let cmd_arg = match self.file_name()?.as_str() {
            "bash" => "-c",
            "sh" => "-c",
            "zsh" => "-c",
            "cmd.exe" => "/C",
            _ => return Err(Error::msg(format!("unknown shell: {}", self.path))),
        };

        let mut exec_args = self.args.clone();
        exec_args.push(cmd_arg.to_string());
        exec_args.push(format!("{} {}", program, args.join(" ")));

        Ok(exec_args)
    }
}

#[cfg(not(target_os = "windows"))]
pub(super) fn get_default_shell(shell: Option<&str>) -> Result<DefaultShell> {
    // Copied from portable_pty
    let mut shell =
        shell
            .and_then(|i| Some(i.to_owned()))
            .unwrap_or(std::env::var("SHELL").or_else(|_| {
                let ent = unsafe { libc::getpwuid(libc::getuid()) };

                if ent.is_null() {
                    Ok("/bin/sh".into())
                } else {
                    use std::ffi::CStr;
                    use std::str;
                    let shell = unsafe { CStr::from_ptr((*ent).pw_shell) };
                    shell
                        .to_str()
                        .map(str::to_owned)
                        .context("failed to resolve shell")
                }
            })?);

    if !shell.parse::<PathBuf>()?.exists() || shell == "nologin" || shell.ends_with("/nologin") {
        shell = "/bin/sh".to_owned();
    }

//     shell = fs::canonicalize(shell.parse::<PathBuf>()?)?
//         .to_string_lossy()
//         .to_string();

    let mut cmd = DefaultShell::new(shell.clone());

    cmd.validate()?;

    // We disable rc files which could potentially alter the stdout/stderr configuration
    // of our shell
    if shell == "bash" || shell.ends_with("/bash") {
        cmd.args.push("--norc".to_owned());
    }

    if shell == "zsh" || shell.ends_with("/zsh") {
        cmd.args.push("--no-rcs".to_owned());
    }

    debug!("identified default shell: {}", shell);

    Ok(cmd)
}

#[cfg(target_os = "windows")]
pub(super) fn get_default_shell(shell: Option<&str>) -> Result<DefaultShell> {
    // Copied from portable_pty
    let shell = shell
        .and_then(|i| Some(i.to_owned()))
        .unwrap_or(std::env::var("ComSpec").unwrap_or("cmd.exe".into()));

    let cmd = DefaultShell::new(shell.clone());

    cmd.validate()?;

    debug!("identified default shell: {}", shell);

    Ok(cmd)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_shell_bash() {
        let cmd = get_default_shell(Some("/bin/bash")).unwrap();

        assert_eq!(
            cmd,
            DefaultShell {
                path: "/bin/bash".to_owned(),
                args: vec!["--norc".to_owned()]
            }
        );

        cmd.validate().unwrap();
    }

    #[test]
    fn test_unknown_validate() {
        let cmd = DefaultShell::new("unknown_shell".to_owned());

        cmd.validate().unwrap_err();
    }

    #[test]
    fn test_new_shell_zsh() {
        let cmd = get_default_shell(Some("/bin/zsh")).unwrap();

        assert_eq!(
            cmd,
            DefaultShell {
                path: "/bin/zsh".to_owned(),
                args: vec!["--no-rcs".to_owned()]
            }
        );
    }

    #[test]
    fn test_new_shell_sh() {
        let cmd = get_default_shell(Some("/bin/sh")).unwrap();

        assert_eq!(
            cmd,
            DefaultShell {
                path: "/bin/sh".to_owned(),
                args: vec![]
            }
        );
    }

    #[test]
    fn test_new_shell_no_env() {
        get_default_shell(None).unwrap();
    }

    #[test]
    fn test_bash_can_delegate() {
        let cmd = get_default_shell(Some("/bin/bash")).unwrap();

        assert_eq!(cmd.can_delegate(), true);
    }

    #[test]
    fn test_unknown_can_delegate() {
        let cmd = DefaultShell::new("unknown_shell".to_owned());

        assert_eq!(cmd.can_delegate(), false);
    }

    #[test]
    fn test_bash_get_execute_command_args() {
        let cmd = get_default_shell(Some("/bin/bash")).unwrap();

        assert_eq!(
            cmd.get_execute_command_args("test", vec!["a", "b", "c"])
                .unwrap(),
            vec!["--norc", "-c", "test a b c"]
        );
    }

    #[test]
    fn test_bash_get_execute_command_args_in_usr_dir() {
        let cmd = DefaultShell::new("/usr/bin/bash".to_owned());

        assert_eq!(
            cmd.get_execute_command_args("test", vec!["a", "b", "c"])
                .unwrap(),
            vec!["-c", "test a b c"]
        );
    }

    #[test]
    fn test_unknown_get_execute_command_args() {
        let cmd = DefaultShell::new("unknown_shell".to_owned());

        cmd.get_execute_command_args("test", vec!["a", "b", "c"])
            .unwrap_err();
    }
}
