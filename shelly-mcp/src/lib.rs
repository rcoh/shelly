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
        
        let request = shelly::ExecuteRequest {
            command: params.command,
            settings: HashMap::new(),
            exact: params.exact,
            working_dir: std::env::current_dir().unwrap(),
            env: HashMap::new(),
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
