use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

pub struct ExecutorConfig {
    pub command: String,
    pub env: HashMap<String, String>,
    pub working_dir: PathBuf,
}

pub struct ExecutorOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

/// Execute a command and capture output
pub async fn execute(config: ExecutorConfig) -> Result<ExecutorOutput> {
    // Parse command into program and args using shell-like parsing
    let parts = shell_words::split(&config.command).context("Failed to parse command")?;
    let (program, args) = parts.split_first().context("Empty command")?;

    let mut cmd = Command::new(program);
    cmd.args(args)
        .env_remove("RUST_LOG")
        .current_dir(&config.working_dir)
        .envs(&config.env)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd.spawn().context("Failed to spawn command")?;

    let stdout = child.stdout.take().context("Failed to capture stdout")?;
    let stderr = child.stderr.take().context("Failed to capture stderr")?;

    let mut stdout_reader = BufReader::new(stdout).lines();
    let mut stderr_reader = BufReader::new(stderr).lines();

    let mut stdout_lines = Vec::new();
    let mut stderr_lines = Vec::new();

    // Read output line by line
    loop {
        tokio::select! {
            line = stdout_reader.next_line() => {
                match line? {
                    Some(l) => stdout_lines.push(l),
                    None => break,
                }
            }
            line = stderr_reader.next_line() => {
                if let Some(l) = line? {
                    stderr_lines.push(l);
                }
            }
        }
    }

    // Read any remaining stderr
    while let Some(line) = stderr_reader.next_line().await? {
        stderr_lines.push(line);
    }

    let status = child.wait().await?;
    let exit_code = status.code().unwrap_or(-1);

    Ok(ExecutorOutput {
        stdout: stdout_lines.join("\n"),
        stderr: stderr_lines.join("\n"),
        exit_code,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[tokio::test]
    async fn test_quoted_arguments_parsing() {
        let config = ExecutorConfig {
            command: r#"echo "hello world" --flag="quoted value""#.to_string(),
            env: HashMap::new(),
            working_dir: env::current_dir().unwrap(),
        };

        let result = execute(config).await.unwrap();

        // Should output: hello world --flag=quoted value
        assert_eq!(result.stdout.trim(), "hello world --flag=quoted value");
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_gh_repo_create_command_parsing() {
        // Test the exact command that was failing
        let config = ExecutorConfig {
            command: r#"echo repo create shelly --public --source=. --description="A smart command execution wrapper" --push"#.to_string(),
            env: HashMap::new(),
            working_dir: env::current_dir().unwrap(),
        };

        let result = execute(config).await.unwrap();

        // Should properly parse the quoted description as a single argument
        let expected = "repo create shelly --public --source=. --description=A smart command execution wrapper --push";
        assert_eq!(result.stdout.trim(), expected);
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_simple_command_still_works() {
        let config = ExecutorConfig {
            command: "echo hello".to_string(),
            env: HashMap::new(),
            working_dir: env::current_dir().unwrap(),
        };

        let result = execute(config).await.unwrap();

        assert_eq!(result.stdout.trim(), "hello");
        assert_eq!(result.exit_code, 0);
    }
}
