use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

pub mod executor;
pub mod handler;
pub mod output;
pub mod runtime;
pub mod testing;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteRequest {
    pub command: String,
    pub settings: HashMap<String, serde_json::Value>,
    pub exact: bool,
    pub working_dir: PathBuf,
    pub env: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutedCommand {
    pub command: String,
    pub env: HashMap<String, String>,
    pub working_dir: PathBuf,
}

/// Execute a command with optional handler processing
pub async fn execute_command(request: ExecuteRequest) -> anyhow::Result<ExecutionResult> {
    let command = &request.command;
    let settings = &request.settings;
    let exact = request.exact;

    // Clean up old output files
    let _ = output::cleanup_old_files();

    // Create output file
    let output_file = output::create_output_file(command)?;

    // Find and load handler (if not exact mode)
    let (final_command, handler_env) = if exact {
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

    // Merge env vars: start with agent's, then handler's (handler wins)
    let mut final_env = request.env.clone();
    final_env.extend(handler_env);

    // Execute command
    let exec_result = executor::execute(executor::ExecutorConfig {
        command: final_command.clone(),
        env: final_env.clone(),
        working_dir: request.working_dir.clone(),
    })
    .await?;

    // Write full output to file
    output::write_output(
        &output_file,
        &exec_result.stdout,
        &exec_result.stderr,
        exec_result.exit_code,
    )?;

    // Get summary (either from handler or raw output)
    let (summary, handler_truncation) = if exact {
        (
            format!("{}\n{}", exec_result.stdout, exec_result.stderr),
            None,
        )
    } else if let Some(handler_path) = handler::find_handler(command)? {
        let mut rt = runtime::HandlerRuntime::new()?;
        rt.load_handler(handler_path.to_str().unwrap()).await?;

        if rt.matches(command).await? {
            rt.create_handler(command, &settings).await?;
            rt.prepare().await?;
            let result = rt
                .summarize(
                    &exec_result.stdout,
                    &exec_result.stderr,
                    Some(exec_result.exit_code),
                )
                .await?;
            (
                result.summary.unwrap_or_else(|| "No output".to_string()),
                result.truncation,
            )
        } else {
            (
                format!("{}\n{}", exec_result.stdout, exec_result.stderr),
                None,
            )
        }
    } else {
        (
            format!("{}\n{}", exec_result.stdout, exec_result.stderr),
            None,
        )
    };

    // Apply length-based truncation if needed and no handler truncation occurred
    const MAX_CHARS: usize = 10000;
    let (final_summary, truncated, truncation_reason) =
        if let Some(handler_trunc) = handler_truncation {
            // Handler provided truncation info
            let truncation_note = if let Some(desc) = &handler_trunc.description {
                format!("\n\n[{}]", desc)
            } else {
                String::new()
            };
            (
                format!("{}{}", summary, truncation_note),
                handler_trunc.truncated,
                handler_trunc.reason,
            )
        } else if summary.len() > MAX_CHARS {
            // Fallback to length-based truncation
            (
                format!(
                    "{}...\n\n[Output truncated due to length. See full output in {}]",
                    &summary[..MAX_CHARS],
                    output_file.display()
                ),
                true,
                Some("content_too_large".to_string()),
            )
        } else {
            (summary, false, None)
        };

    Ok(ExecutionResult {
        summary: final_summary,
        output_file: output_file.to_string_lossy().to_string(),
        exit_code: exec_result.exit_code,
        truncated,
        truncation_reason,
        executed_command: ExecutedCommand {
            command: final_command,
            env: final_env,
            working_dir: request.working_dir,
        },
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
    /// Reason for truncation (if any)
    pub truncation_reason: Option<String>,
    /// The actual command that was executed
    pub executed_command: ExecutedCommand,
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
            command: "cargo --version".to_string(),
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
            command: "echo hello".to_string(),
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
    async fn test_handler_test_framework() {
        // Find tests for cargo handler
        let tests = testing::find_tests("cargo").unwrap();
        assert!(!tests.is_empty(), "Should find cargo tests");

        // Run all tests
        let handler_path = std::path::PathBuf::from("handlers/cargo.ts");
        for (name, test) in &tests {
            let result = testing::run_test(&handler_path, name, test).await.unwrap();
            assert!(
                result.passed,
                "Test {} failed:\nExpected: {}\nActual: {}",
                result.name, result.expected, result.actual
            );
        }
    }
}
