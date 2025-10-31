use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::time::Duration;

use crate::process_manager::ProcessState;

pub mod executor;
pub mod handler;
pub mod output;
pub mod process_manager;
pub mod runtime;
pub mod streaming_executor;
pub mod testing;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteRequest {
    pub cmd: String,
    pub args: Vec<String>,
    pub settings: HashMap<String, serde_json::Value>,
    pub exact: bool,
    pub working_dir: PathBuf,
    pub env: HashMap<String, String>,
}

impl ExecuteRequest {
    /// Get the full command as a single string (for compatibility)
    pub fn command(&self) -> String {
        if self.args.is_empty() {
            self.cmd.clone()
        } else {
            format!("{} {}", self.cmd, self.args.join(" "))
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutedCommand {
    pub cmd: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
    pub working_dir: PathBuf,
}

impl ExecutedCommand {
    /// Get the full command as a single string (for compatibility)
    pub fn command(&self) -> String {
        if self.args.is_empty() {
            self.cmd.clone()
        } else {
            format!("{} {}", self.cmd, self.args.join(" "))
        }
    }
}

/// Execute a command with streaming support and timeout handling
pub async fn execute_command_streaming(
    request: ExecuteRequest,
    process_manager: Arc<process_manager::ProcessManager>,
    timeout_duration: Duration,
) -> anyhow::Result<ExecutionResult> {
    let command = request.command();
    let settings = &request.settings;
    let exact = request.exact;

    // Clean up old output files
    let _ = output::cleanup_old_files();

    // Create output file
    let output_file = output::create_output_file(&command)?;

    // Find and load handler (if not exact mode)
    let (final_cmd, final_args, handler_env, rt) = if exact {
        (request.cmd.clone(), request.args.clone(), HashMap::new(), None)
    } else if let Some(handler_path) = handler::find_handler(&command)? {
        tracing::info!("found custom hanlder for {command} @ {handler_path:?}");
        let mut rt = runtime::HandlerRuntime::new()?;
        rt.load_handler(handler_path.to_str().unwrap()).await?;

        if rt.matches(&request.cmd, &request.args).await? {
            tracing::info!("Found custom handler for {command}");
            rt.create_handler(&request.cmd, &request.args, &settings).await?;
            let prep = rt.prepare().await?;
            tracing::info!("Command has changed command to be: {prep:?}");
            (prep.cmd, prep.args, prep.env, Some(rt))
        } else {
            tracing::info!("no custom handler for {command}");
            (request.cmd.clone(), request.args.clone(), HashMap::new(), None)
        }
    } else {
        (request.cmd.clone(), request.args.clone(), HashMap::new(), None)
    };

    // Merge env vars: start with agent's, then handler's (handler wins)
    let mut final_env = request.env.clone();
    final_env.extend(handler_env);

    // Execute with streaming
    let streaming_config = streaming_executor::StreamingExecutorConfig {
        cmd: final_cmd.clone(),
        args: final_args.clone(),
        env: final_env.clone(),
        working_dir: request.working_dir.clone(),
        update_interval: Duration::from_millis(500), // Update every 500ms
        handler: rt,
        output_file: output_file.clone(),
    };

    let process_id = streaming_executor::spawn(streaming_config, process_manager.clone()).await?;
    let status = process_manager
        .join_process(&process_id, timeout_duration)
        .await
        .expect("we just started it, it should be running");
    let executed_command = ExecutedCommand {
        cmd: final_cmd,
        args: final_args,
        env: final_env,
        working_dir: request.working_dir,
    };

    // If command timed out, return partial results with process info
    Ok(match status.status {
        ProcessState::Running => ExecutionResult {
            summary: format!(
                "Command is still running - use join_process to continue monitoring\n{}",
                status.incremental_summary
            ),
            output_file: output_file.to_string_lossy().to_string(),
            exit_code: -1,
            truncated: false,
            truncation_reason: Some("timeout".to_string()),
            executed_command,
            process_id: Some(process_id),
            is_running: true,
            available_actions: vec![
                ProcessAction::Join,
                ProcessAction::Cancel,
                ProcessAction::Status,
            ],
        },
        ProcessState::Completed { exit_code } => ExecutionResult {
            summary: status.incremental_summary,
            output_file: output_file.to_string_lossy().to_string(),
            exit_code,
            truncated: false,
            truncation_reason: Some("ignore".to_string()),
            executed_command,
            process_id: Some(process_id),
            is_running: false,
            available_actions: vec![],
        },
        _ => panic!("unepxected state"),
    })
}

/// Simple wrapper around execute_command_streaming for non-streaming use cases
pub async fn execute_command(request: ExecuteRequest) -> anyhow::Result<ExecutionResult> {
    let process_manager = Arc::new(process_manager::ProcessManager::new());
    let timeout_duration = Duration::from_secs(30);
    execute_command_streaming(request, process_manager, timeout_duration).await
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    /// Summary of the command output (max ~500 tokens)
    pub summary: String,
    /// Path to file containing full output
    pub output_file: String,
    /// Exit code of the command
    pub exit_code: i32,
    /// Whether output was truncated
    pub truncated: bool,
    /// Reason for truncation (if any)
    pub truncation_reason: Option<String>,
    /// The actual command that was executed
    pub executed_command: ExecutedCommand,
    /// Process ID for long-running commands (if applicable)
    pub process_id: Option<process_manager::ProcessId>,
    /// Whether the command is still running
    pub is_running: bool,
    /// Available actions for the client
    pub available_actions: Vec<ProcessAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ProcessAction {
    Join,   // Continue waiting with updates
    Cancel, // Cancel the running process
    Status, // Get current status
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_basic_runtime() {
        let mut rt = runtime::HandlerRuntime::new().unwrap();

        // Load the cargo handler
        rt.load_handler("handlers/cargo.ts").await.unwrap();

        // Test matches
        assert!(rt.matches("cargo", &["build".to_string()]).await.unwrap());
        assert!(!rt.matches("npm", &["install".to_string()]).await.unwrap());

        // Create handler instance
        let settings = HashMap::new();
        rt.create_handler("cargo", &["build".to_string()], &settings).await.unwrap();

        // Test prepare
        let prep = rt.prepare().await.unwrap();
        assert!(prep.args.contains(&"--quiet".to_string()));
    }

    #[tokio::test]
    async fn test_handler_summarize() {
        let mut rt = runtime::HandlerRuntime::new().unwrap();
        rt.load_handler("handlers/cargo.ts").await.unwrap();

        let settings = HashMap::new();
        rt.create_handler("cargo", &["build".to_string()], &settings).await.unwrap();
        rt.prepare().await.unwrap();

        // Test incremental summarization
        let result = rt.summarize("", "Compiling...\n", None).await.unwrap();
        assert!(result.summary.is_none()); // Still buffering

        // Complete with error
        let result = rt
            .summarize("", "error: something broke\n", Some(1))
            .await
            .unwrap();
        assert!(result.summary.is_some());
        assert!(result.summary.unwrap().contains("error"));
    }

    #[tokio::test]
    async fn test_execute_command_with_handler() {
        let request = ExecuteRequest {
            cmd: "cargo".to_string(),
            args: vec!["--version".to_string()],
            settings: HashMap::new(),
            exact: false,
            working_dir: std::env::current_dir().unwrap(),
            env: HashMap::new(),
        };

        let result = execute_command(request).await.unwrap();
        assert_eq!(result.exit_code, 0);
        assert!(!result.output_file.is_empty());
        assert!(!result.summary.is_empty());
    }

    #[tokio::test]
    async fn test_execute_command_exact_mode() {
        let request = ExecuteRequest {
            cmd: "echo".to_string(),
            args: vec!["hello".to_string()],
            settings: HashMap::new(),
            exact: true,
            working_dir: std::env::current_dir().unwrap(),
            env: HashMap::new(),
        };

        let result = execute_command(request).await.unwrap();
        assert_eq!(result.exit_code, 0);
        assert!(result.summary.contains("hello"));
    }

    #[tokio::test]
    async fn test_output_file_creation() {
        let request = ExecuteRequest {
            cmd: "echo".to_string(),
            args: vec!["test".to_string(), "output".to_string()],
            settings: HashMap::new(),
            exact: true,
            working_dir: std::env::current_dir().unwrap(),
            env: HashMap::new(),
        };

        let result = execute_command(request).await.unwrap();

        // Check that output file was created and contains content
        assert!(!result.output_file.is_empty());
        let output_content = std::fs::read_to_string(&result.output_file).unwrap();
        assert!(output_content.contains("test output"));
    }

    #[tokio::test]
    async fn test_streaming_output_file_creation() {
        let process_manager = Arc::new(process_manager::ProcessManager::new());

        let request = ExecuteRequest {
            cmd: "echo".to_string(),
            args: vec!["streaming".to_string(), "test".to_string()],
            settings: HashMap::new(),
            exact: true,
            working_dir: std::env::current_dir().unwrap(),
            env: HashMap::new(),
        };

        let result = execute_command_streaming(
            request,
            process_manager,
            Duration::from_secs(30), // Long timeout so it completes
        )
        .await
        .unwrap();

        dbg!(&result);
        // Check that output file was created and contains content
        assert!(!result.output_file.is_empty());
        let output_content = std::fs::read_to_string(&result.output_file).unwrap();
        assert!(output_content.contains("streaming test"));
    }
}
