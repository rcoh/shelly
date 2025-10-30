use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::time::{timeout, Duration};

use crate::process_manager::{ProcessId, ProcessManager};
use crate::runtime::HandlerRuntime;

pub struct StreamingExecutorConfig {
    pub command: String,
    pub env: HashMap<String, String>,
    pub working_dir: PathBuf,
    pub update_interval: Duration,
    pub handler: Option<HandlerRuntime>,
}

pub struct StreamingExecutorResult {
    pub process_id: ProcessId,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub completed: bool,
}

/// Execute with timeout that provides incremental updates
pub async fn spawn(
    config: StreamingExecutorConfig,
    process_manager: Arc<ProcessManager>,
) -> Result<ProcessId> {
    // Start process tracking
    let process_id = process_manager.start_process(config.command.clone()).await;

    // Spawn the actual execution task
    let process_manager_clone = process_manager.clone();
    let process_id_clone = process_id.clone();

    let handle = tokio::spawn(async move {
        execute_streaming_internal(config, process_manager_clone, process_id_clone).await
    });

    // Register the handle
    process_manager.register_handle(&process_id, handle).await;

    // For now, let's wait for the handle directly since we have it
    // TODO: Implement proper incremental updates later
    Ok(process_id)
}

/// Internal execution function that does the actual work
async fn execute_streaming_internal(
    config: StreamingExecutorConfig,
    process_manager: Arc<ProcessManager>,
    process_id: ProcessId,
) -> Result<()> {
    // Parse command into program and args
    let parts = shell_words::split(&config.command).context("Failed to parse command")?;
    let (program, args) = parts.split_first().context("Empty command")?;

    let mut cmd = Command::new(program);
    cmd.args(args)
        .current_dir(&config.working_dir)
        .env_remove("RUST_LOG")
        .envs(&config.env)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd.spawn().context("Failed to spawn command")?;

    let stdout = child.stdout.take().context("Failed to capture stdout")?;
    let stderr = child.stderr.take().context("Failed to capture stderr")?;

    let mut stdout_reader = BufReader::new(stdout).lines();
    let mut stderr_reader = BufReader::new(stderr).lines();

    let handler = config.handler;

    // Read output line by line with periodic updates
    loop {
        tokio::select! {
            line = stdout_reader.next_line() => {
                match line? {
                    Some(l) => {
                            process_manager.update_process_output(
                                &process_id,
                                format!("{}\n", l),
                                String::new(),
                                &handler,
                            ).await;
                    }
                    None => break,
                }
            }
            line = stderr_reader.next_line() => {
                if let Some(l) = line? {
                    process_manager.update_process_output(
                        &process_id,
                        String::new(),
                        format!("{}\n", l),
                        &handler,
                    ).await;
                }
            }
        }
    }

    let status = child.wait().await?;
    let exit_code = status.code().unwrap_or(-1);

    // Final handler call with exit code
    if let Some(ref handler) = handler {
        process_manager.final_process_summary(&process_id, exit_code, &handler).await;
    }

    process_manager
        .complete_process(&process_id, exit_code)
        .await;
    Ok(())
}

#[cfg(test)]
mod tests {
    //use super::*;
    //use std::env;

    /*
    #[tokio::test]
    async fn test_streaming_execution() {
        let process_manager = Arc::new(ProcessManager::new());

        let config = StreamingExecutorConfig {
            command: "echo hello".to_string(),
            env: HashMap::new(),
            working_dir: env::current_dir().unwrap(),
            update_interval: Duration::from_millis(100),
            handler: None,
        };

        let result = execute_with_timeout(config, process_manager, Duration::from_secs(5))
            .await
            .unwrap();

        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("hello"));
        assert!(result.completed);
    }

    #[tokio::test]
    async fn test_long_running_command() {
        let process_manager = Arc::new(ProcessManager::new());

        let config = StreamingExecutorConfig {
            command: "sleep 1 && echo done".to_string(),
            env: HashMap::new(),
            working_dir: env::current_dir().unwrap(),
            update_interval: Duration::from_millis(100),
            handler: None,
        };

        let result = execute_with_timeout(config, process_manager.clone(), Duration::from_secs(5))
            .await
            .unwrap();

        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("done"));

        // Check that process was tracked
        let status = process_manager.get_process_status(&result.process_id).await;
        assert!(status.is_some());
    }
    */
}
