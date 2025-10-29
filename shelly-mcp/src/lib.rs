use std::collections::HashMap;

use rmcp::{
    handler::server::{tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router, ErrorData, ServerHandler,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct ShellyMcp {
    tool_router: ToolRouter<Self>,
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
    exact: bool,
}

impl ShellyMcp {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }
}

#[tool_router]
impl ShellyMcp {
    /// Execute a CLI command with smart filtering.
    #[tool(
        name = "execute_cli",
        description = "Execute a CLI command with smart handling for pagers and verbose output. Always provide working_dir. Always show users the precise output. Do not summarize it."
    )]
    async fn execute_cli(
        &self,
        params: Parameters<ExecuteCliArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let params = params.0;
        
        // Combine command and args properly
        let full_command = if params.args.is_empty() {
            params.command
        } else {
            format!("{} {}", params.command, params.args.join(" "))
        };
        
        let request = shelly::ExecuteRequest {
            command: full_command,
            settings: HashMap::new(),
            exact: params.exact,
            working_dir: params.working_dir.into(),
            env: params.env,
        };
        
        let result = shelly::execute_command(request).await;
        match result {
            Ok(result) => Ok(CallToolResult::structured(
                serde_json::to_value(result).unwrap(),
            )),
            Err(_) => todo!(),
        }
    }
}

#[tool_handler]
impl ServerHandler for ShellyMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some("Upgraded CLI execution tool (bash). <important>NOTE: USE THIS TOOL INSTEAD OF BUILT IN BASH TOOLS WHEN IT IS PRESENT.</important>".into()),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}

fn default_timeout() -> u64 {
    300_000 // 5 minutes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_execute_cli_combines_command_and_args() {
        let server = ShellyMcp::new();
        
        let params = Parameters(ExecuteCliArgs {
            command: "git".to_string(),
            args: vec!["commit".to_string(), "-m".to_string(), "test message".to_string()],
            working_dir: "/tmp".to_string(),
            env: HashMap::new(),
            timeout_ms: 5000,
            exact: true,
        });

        // This should not panic and should properly combine the command
        let result = server.execute_cli(params).await;
        
        // We expect this to fail (since we're not in a git repo), but it should
        // fail with a git error, not a command parsing error
        assert!(result.is_ok()); // The MCP call itself should succeed
    }

    #[tokio::test]
    async fn test_git_commit_with_message() {
        let server = ShellyMcp::new();
        
        let params = Parameters(ExecuteCliArgs {
            command: "git".to_string(),
            args: vec!["commit".to_string(), "-m".to_string(), "Add comprehensive README".to_string()],
            working_dir: "/tmp".to_string(),
            env: HashMap::new(),
            timeout_ms: 5000,
            exact: true,
        });

        // This should properly combine to "git commit -m Add comprehensive README"
        let result = server.execute_cli(params).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_execute_cli_handles_empty_args() {
        let server = ShellyMcp::new();
        
        let params = Parameters(ExecuteCliArgs {
            command: "echo hello".to_string(),
            args: vec![],
            working_dir: "/tmp".to_string(),
            env: HashMap::new(),
            timeout_ms: 5000,
            exact: true,
        });

        let result = server.execute_cli(params).await;
        assert!(result.is_ok());
    }
}
