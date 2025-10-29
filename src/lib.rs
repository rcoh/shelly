use std::collections::HashMap;
use serde::{Deserialize, Serialize};

pub mod executor;
pub mod output;
pub mod runtime;
pub mod handler;

/// Execute a command with optional handler processing
pub async fn execute_command(
    _command: &str,
    _settings: HashMap<String, serde_json::Value>,
    _exact: bool,
) -> anyhow::Result<ExecutionResult> {
    todo!()
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
        assert!(rt.matches("cargo build").await.unwrap());
        assert!(!rt.matches("npm install").await.unwrap());
        
        // Create handler instance
        let settings = HashMap::new();
        rt.create_handler("cargo build", &settings).await.unwrap();
        
        // Test prepare
        let prep = rt.prepare().await.unwrap();
        assert!(prep.command.contains("--quiet"));
    }

    #[tokio::test]
    async fn test_handler_summarize() {
        let mut rt = runtime::HandlerRuntime::new().unwrap();
        rt.load_handler("handlers/cargo.ts").await.unwrap();
        
        let settings = HashMap::new();
        rt.create_handler("cargo build", &settings).await.unwrap();
        rt.prepare().await.unwrap();
        
        // Test incremental summarization
        let result = rt.summarize("", "Compiling...\n", None).await.unwrap();
        assert!(result.summary.is_none()); // Still buffering
        
        // Complete with error
        let result = rt.summarize("", "error: something broke\n", Some(1)).await.unwrap();
        assert!(result.summary.is_some());
        assert!(result.summary.unwrap().contains("error"));
    }
}
