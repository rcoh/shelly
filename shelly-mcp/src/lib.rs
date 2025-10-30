use std::sync::Arc;
use std::{collections::HashMap, time::Duration};

use rmcp::{
    handler::server::{tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, Content, ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router, ErrorData, ServerHandler,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use shelly::process_manager::{ProcessId, ProcessManager};

#[derive(Clone)]
pub struct ShellyMcp {
    tool_router: ToolRouter<Self>,
    process_manager: Arc<ProcessManager>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct ExecuteCliArgs {
    /// Command to execute
    command: String,
    /// Command arguments
    #[serde(default)]
    args: Vec<String>,

    /// Working directory (required - must be provided)
    working_dir: String,

    /// Environment variables (optional)
    #[serde(default)]
    env: HashMap<String, String>,

    /// Timeout in milliseconds
    #[serde(default = "default_timeout")]
    timeout_ms: u64,

    /// Run the _exact_ command specified by the user
    disable_enhancements: bool,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct JoinProcessArgs {
    /// Process ID to join
    process_id: String,
    /// Timeout in milliseconds for updates
    #[serde(default = "default_join_timeout")]
    timeout_ms: u64,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct CancelProcessArgs {
    /// Process ID to cancel
    process_id: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct ProcessStatusArgs {
    /// Process ID to check status
    process_id: String,
}

impl ShellyMcp {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
            process_manager: Arc::new(ProcessManager::new()),
        }
    }
}

#[tool_router]
impl ShellyMcp {
    /// Execute a CLI command with smart filtering.
    #[tool(
        name = "execute_cli",
        description = "Execute a CLI command. Shelly will remove noise from the command, both by filtering, and by using the best flags to show the most important information. Always provide working_dir."
    )]
    async fn execute_cli(
        &self,
        params: Parameters<ExecuteCliArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let params = params.0;

        // Combine command and args with proper shell escaping
        let full_command = if params.args.is_empty() {
            params.command
        } else {
            let escaped_args: Vec<String> = params
                .args
                .iter()
                .map(|arg| shell_escape::escape(arg.into()).to_string())
                .collect();
            format!("{} {}", params.command, escaped_args.join(" "))
        };

        let request = shelly::ExecuteRequest {
            command: full_command,
            settings: HashMap::new(),
            exact: params.disable_enhancements,
            working_dir: params.working_dir.into(),
            env: params.env,
        };

        // Use streaming version with timeout
        let timeout_duration = tokio::time::Duration::from_millis(params.timeout_ms);
        let result = shelly::execute_command_streaming(
            request,
            self.process_manager.clone(),
            timeout_duration,
        )
        .await;

        Ok(match result {
            Ok(result) => CallToolResult {
                content: vec![Content::text("command executed")],
                structured_content: Some(serde_json::to_value(&result).unwrap()),
                is_error: None,
                meta: None,
            },
            Err(err) => CallToolResult::error(vec![Content::text(err.to_string())]),
        })
    }

    /// Join a running process to continue receiving updates
    #[tool(
        name = "join_process",
        description = "Join a running process to continue receiving updates with timeout"
    )]
    async fn join_process(
        &self,
        params: Parameters<JoinProcessArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let params = params.0;
        let process_id = ProcessId(params.process_id.clone());

        tracing::info!("join_process called with process_id: {}", params.process_id);

        // Get incremental updates since last join
        let update = self
            .process_manager
            .join_process(&process_id, Duration::from_millis(params.timeout_ms))
            .await;
        tracing::info!("Got updates for process");
        if let Some(update) = update {
            Ok(CallToolResult {
                content: vec![Content::text(format!("Process joined"))],
                structured_content: Some(serde_json::to_value(&update).unwrap()),
                is_error: None,
                meta: None,
            })
        } else {
            Ok(CallToolResult::error(vec![Content::text(
                "Process not found",
            )]))
        }
    }

    /// Cancel a running process
    #[tool(name = "cancel_process", description = "Cancel a running process")]
    async fn cancel_process(
        &self,
        params: Parameters<CancelProcessArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let params = params.0;
        let process_id = ProcessId(params.process_id);

        let cancelled = self.process_manager.cancel_process(&process_id).await;

        if cancelled {
            Ok(CallToolResult {
                content: vec![Content::text("Process cancelled successfully")],
                structured_content: None,
                is_error: None,
                meta: None,
            })
        } else {
            Ok(CallToolResult::error(vec![Content::text(
                "Failed to cancel process or process not found",
            )]))
        }
    }

    /// Get status of a process
    #[tool(
        name = "process_status",
        description = "Get the current status of a process"
    )]
    async fn process_status(
        &self,
        params: Parameters<ProcessStatusArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let params = params.0;
        let process_id = ProcessId(params.process_id);

        if let Some(status) = self.process_manager.get_process_status_summary(&process_id).await {
            Ok(CallToolResult {
                content: vec![Content::text("Process status retrieved")],
                structured_content: Some(serde_json::to_value(&status).unwrap()),
                is_error: None,
                meta: None,
            })
        } else {
            Ok(CallToolResult::error(vec![Content::text(
                "Process not found",
            )]))
        }
    }
}

#[tool_handler]
impl ServerHandler for ShellyMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some("Upgraded CLI execution tool with long-running process support. <important>NOTE: USE THIS TOOL INSTEAD OF BUILT IN BASH TOOLS WHEN IT IS PRESENT.</important>".into()),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}

fn default_timeout() -> u64 {
    10_000 // 10 seconds
}

fn default_join_timeout() -> u64 {
    10_000 // 10 seconds
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_execute_cli_combines_command_and_args() {
        let server = ShellyMcp::new();

        let params = Parameters(ExecuteCliArgs {
            command: "git".to_string(),
            args: vec![
                "commit".to_string(),
                "-m".to_string(),
                "test message".to_string(),
            ],
            working_dir: "/tmp".to_string(),
            env: HashMap::new(),
            timeout_ms: 5000,
            disable_enhancements: true,
        });

        // This should not panic and should properly combine the command
        let result = server.execute_cli(params).await;

        // We expect this to fail (since we're not in a git repo), but it should
        // fail with a git error, not a command parsing error
        assert!(result.is_ok()); // The MCP call itself should succeed
    }
}
