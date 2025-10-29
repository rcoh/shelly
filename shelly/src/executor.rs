use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use anyhow::{Context, Result};

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
    // Parse command into program and args
    let parts: Vec<&str> = config.command.split_whitespace().collect();
    let (program, args) = parts.split_first()
        .context("Empty command")?;

    let mut cmd = Command::new(program);
    cmd.args(args)
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
