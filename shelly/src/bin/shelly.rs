use clap::{Parser, Subcommand};
use shelly::{testing, handler};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "shelly")]
#[command(about = "A command execution tool with smart output filtering")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run tests for handlers
    Test {
        /// Handler name to test (e.g., "cargo", "brazil-build")
        handler: Option<String>,
        /// Update test snapshots instead of running tests
        #[arg(long)]
        update: bool,
    },
    /// Execute a command with handler processing
    Execute {
        /// Command to execute
        command: String,
        /// Execute in exact mode (no handler processing)
        #[arg(long)]
        exact: bool,
        /// Working directory
        #[arg(long)]
        working_dir: Option<PathBuf>,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Test { handler, update } => {
            if let Some(handler_name) = handler {
                run_handler_tests(&handler_name, update).await?;
            } else {
                // Run tests for all handlers found in .shelly/tests/
                let test_dir = PathBuf::from(".shelly/tests");
                if test_dir.exists() {
                    for entry in std::fs::read_dir(&test_dir)? {
                        let entry = entry?;
                        if entry.path().is_dir() {
                            let handler_name = entry.file_name().to_string_lossy().to_string();
                            println!("Testing handler: {}", handler_name);
                            run_handler_tests(&handler_name, update).await?;
                        }
                    }
                } else {
                    println!("No .shelly/tests directory found");
                }
            }
        }
        Commands::Execute { command, exact, working_dir } => {
            // Parse command into cmd and args
            let parts: Vec<&str> = command.split_whitespace().collect();
            let cmd = parts.first().unwrap_or(&"").to_string();
            let args = parts.iter().skip(1).map(|s| s.to_string()).collect();
            
            let request = shelly::ExecuteRequest {
                cmd,
                args,
                settings: HashMap::new(),
                exact,
                working_dir: working_dir.unwrap_or_else(|| std::env::current_dir().unwrap()),
                env: std::env::vars().collect(),
            };

            let result = shelly::execute_command(request).await?;
            println!("{}", result.summary);
            
            if result.exit_code != 0 {
                std::process::exit(result.exit_code);
            }
        }
    }

    Ok(())
}

async fn run_handler_tests(handler_name: &str, update: bool) -> anyhow::Result<()> {
    // Find handler file
    let handler_path = if let Some(path) = handler::find_handler(handler_name)? {
        path
    } else {
        anyhow::bail!("Handler not found: {}", handler_name);
    };

    // Find tests
    let mut tests = testing::find_tests(handler_name)?;
    if tests.is_empty() {
        println!("No tests found for handler: {}", handler_name);
        return Ok(());
    }

    println!("Running {} tests for handler: {}", tests.len(), handler_name);

    let mut passed = 0;
    let mut failed = 0;

    for (name, mut test) in tests {
        if update {
            // Update snapshot
            testing::update_snapshot(&handler_path, handler_name, &name, &mut test).await?;
            println!("  ✓ Updated snapshot: {}", name);
        } else {
            // Run test
            let result = testing::run_test(&handler_path, &name, &test).await?;
            if result.passed {
                println!("  ✓ {}", name);
                passed += 1;
            } else {
                println!("  ✗ {}", name);
                println!("    Expected:");
                for line in result.expected.lines() {
                    println!("      {}", line);
                }
                println!("    Actual:");
                for line in result.actual.lines() {
                    println!("      {}", line);
                }
                failed += 1;
            }
        }
    }

    if !update {
        println!("\nResults: {} passed, {} failed", passed, failed);
        if failed > 0 {
            std::process::exit(1);
        }
    }

    Ok(())
}
