use rmcp::{transport::stdio, ServiceExt};
use shelly_mcp::ShellyMcp;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    run_mcp_server().await
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
