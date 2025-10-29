use std::collections::HashMap;
use serde::{Deserialize, Serialize};

pub mod executor;
pub mod output;
pub mod runtime;
pub mod handler;
pub mod testing;

/// Execute a command with optional handler processing
pub async fn execute_command(
    command: &str,
    settings: HashMap<String, serde_json::Value>,
    exact: bool,
) -> anyhow::Result<ExecutionResult> {
    // Clean up old output files
    let _ = output::cleanup_old_files();
    
    // Create output file
    let output_file = output::create_output_file(command)?;
    
    // Find and load handler (if not exact mode)
    let (final_command, env) = if exact {
        (command.to_string(), HashMap::new())
    } else if let Some(handler_path) = handler::find_handler(command)? {
        let mut rt = runtime::HandlerRuntime::new()?;
        rt.load_handler(handler_path.to_str().unwrap()).await?;
        
        if rt.matches(command).await? {
            rt.create_handler(command, &settings).await?;
            let prep = rt.prepare().await?;
            (prep.command, prep.env)
        } else {
            (command.to_string(), HashMap::new())
        }
    } else {
        (command.to_string(), HashMap::new())
    };
    
    // Execute command
    let exec_result = executor::execute(executor::ExecutorConfig {
        command: final_command,
        env,
    }).await?;
    
    // Write full output to file
    output::write_output(
        &output_file,
        &exec_result.stdout,
        &exec_result.stderr,
        exec_result.exit_code,
    )?;
    
    // Get summary (either from handler or raw output)
    let summary = if exact {
        format!("{}\n{}", exec_result.stdout, exec_result.stderr)
    } else if let Some(handler_path) = handler::find_handler(command)? {
        let mut rt = runtime::HandlerRuntime::new()?;
        rt.load_handler(handler_path.to_str().unwrap()).await?;
        
        if rt.matches(command).await? {
            rt.create_handler(command, &settings).await?;
            rt.prepare().await?;
            let result = rt.summarize(
                &exec_result.stdout,
                &exec_result.stderr,
                Some(exec_result.exit_code),
            ).await?;
            result.summary.unwrap_or_else(|| "No output".to_string())
        } else {
            format!("{}\n{}", exec_result.stdout, exec_result.stderr)
        }
    } else {
        format!("{}\n{}", exec_result.stdout, exec_result.stderr)
    };
    
    // Truncate summary to ~500 tokens (rough estimate: 4 chars per token)
    const MAX_CHARS: usize = 2000;
    let (truncated_summary, truncated) = if summary.len() > MAX_CHARS {
        (format!("{}...\n\n[Output truncated. See full output in {}]", 
                 &summary[..MAX_CHARS], 
                 output_file.display()), 
         true)
    } else {
        (summary, false)
    };
    
    Ok(ExecutionResult {
        summary: truncated_summary,
        output_file: output_file.to_string_lossy().to_string(),
        exit_code: exec_result.exit_code,
        truncated,
    })
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

    #[tokio::test]
    async fn test_execute_command_with_handler() {
        let settings = HashMap::new();
        
        // Test with cargo (should add --quiet and filter output)
        let result = execute_command("cargo --version", settings, false).await.unwrap();
        assert_eq!(result.exit_code, 0);
        assert!(!result.output_file.is_empty());
        // Handler filters output, so we get "Build succeeded" for successful commands
        assert!(!result.summary.is_empty());
    }

    #[tokio::test]
    async fn test_execute_command_exact_mode() {
        let settings = HashMap::new();
        
        // Test exact mode (no handler processing)
        let result = execute_command("echo hello", settings, true).await.unwrap();
        assert_eq!(result.exit_code, 0);
        assert!(result.summary.contains("hello"));
    }

    #[tokio::test]
    async fn test_handler_test_framework() {
        // Find tests for cargo handler
        let tests = testing::find_tests("cargo").unwrap();
        assert!(!tests.is_empty(), "Should find cargo tests");
        
        // Run all tests
        let handler_path = std::path::PathBuf::from("handlers/cargo.ts");
        for test in &tests {
            let result = testing::run_test(&handler_path, test).await.unwrap();
            assert!(result.passed, "Test {} failed:\nExpected: {}\nActual: {}", 
                    result.name, result.expected, result.actual);
        }
    }
}
