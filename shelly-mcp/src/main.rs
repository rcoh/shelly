use rmcp::{transport::stdio, ServiceExt};
use shelly_mcp::ShellyMcp;
use tracing_subscriber::EnvFilter;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "shelly-mcp")]
#[command(about = "Shelly MCP server and setup tool")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Set up Shelly for Q CLI integration
    Setup,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Setup) => {
            setup_shelly().await?;
        }
        None => {
            run_mcp_server().await?;
        }
    }

    Ok(())
}

async fn setup_shelly() -> anyhow::Result<()> {
    println!("🔧 Setting up Shelly...");

    // 1. Install binary
    install_binary()?;
    
    // 2. Create .shelly directory structure
    create_shelly_directory()?;
    
    // 3. Configure Q CLI MCP integration
    configure_q_cli_mcp()?;
    
    println!("✅ Shelly setup complete!");
    println!("You can now use Shelly with Q CLI by running commands through the execute_cli tool.");
    
    Ok(())
}

fn install_binary() -> anyhow::Result<()> {
    println!("📦 Installing Shelly binary...");
    
    let current_exe = std::env::current_exe()?;
    let bin_dir = dirs::home_dir()
        .ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?
        .join(".local/bin");
    
    std::fs::create_dir_all(&bin_dir)?;
    let target_path = bin_dir.join("shelly-mcp");
    
    std::fs::copy(&current_exe, &target_path)?;
    
    // Make executable on Unix systems
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&target_path)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&target_path, perms)?;
    }
    
    println!("   Installed to: {}", target_path.display());
    Ok(())
}

fn create_shelly_directory() -> anyhow::Result<()> {
    println!("📁 Creating .shelly directory structure...");
    
    let home_dir = dirs::home_dir()
        .ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?;
    let shelly_dir = home_dir.join(".shelly");
    
    std::fs::create_dir_all(&shelly_dir)?;
    std::fs::create_dir_all(shelly_dir.join("tests"))?;
    
    println!("   Created directory: {}", shelly_dir.display());
    Ok(())
}

fn configure_q_cli_mcp() -> anyhow::Result<()> {
    println!("⚙️  Configuring Q CLI MCP integration...");
    
    let home_dir = dirs::home_dir()
        .ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?;
    let shelly_mcp_path = home_dir.join(".local/bin/shelly-mcp");
    
    let output = std::process::Command::new("q")
        .args([
            "mcp",
            "add", 
            "--name", "shelly",
            "--command", &shelly_mcp_path.to_string_lossy(),
            "--force"
        ])
        .output();
    
    match output {
        Ok(result) if result.status.success() => {
            println!("   ✅ MCP server 'shelly' registered successfully with Q CLI!");
            Ok(())
        }
        Ok(result) => {
            let stderr = String::from_utf8_lossy(&result.stderr);
            println!("   ❌ Failed to register MCP server with Q CLI:");
            println!("      Error: {}", stderr.trim());
            println!("   📋 Manual setup: Run `q mcp add --name shelly --command {}`", shelly_mcp_path.display());
            Ok(())
        }
        Err(e) => {
            println!("   ❌ Could not run 'q mcp add' command: {}", e);
            println!("   📋 Manual setup: Run `q mcp add --name shelly --command {}`", shelly_mcp_path.display());
            Ok(())
        }
    }
}

async fn run_mcp_server() -> anyhow::Result<()> {
    // Initialize logging
    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("shelly-mcp.log")?;
    
    tracing_subscriber::fmt()
        .with_writer(log_file)
        .with_ansi(false)
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    tracing::info!("Starting Shellephant MCP server");

    // Create and run the MCP server
    let server = ShellyMcp::new()
        .serve(stdio())
        .await
        .inspect_err(|e| tracing::error!("serving error: {e:?}"))?;

    server.waiting().await.unwrap();
    Ok(())
}
