#[tokio::main]
async fn main() -> anyhow::Result<()> {
    setup_shelly().await
}

async fn setup_shelly() -> anyhow::Result<()> {
    println!("🔧 Setting up Shelly...");

    // 1. Build release binary
    build_release_binary()?;

    // 2. Install binary
    install_binary()?;

    // 3. Create .shelly directory structure
    create_shelly_directory()?;

    // 4. Create test handler
    create_test_handler()?;

    // 5. Configure Q CLI MCP integration
    configure_q_cli_mcp()?;

    println!("✅ Shelly setup complete!");
    println!("You can now use Shelly with Q CLI by running commands through the execute_cli tool.");
    println!();
    println!("🧪 Test your setup with:");
    println!("   q chat --non-interactive \"Run \\`shelly-test\\` with shelly\"");

    Ok(())
}

fn build_release_binary() -> anyhow::Result<()> {
    println!("🔨 Building Shelly MCP server...");

    let output = std::process::Command::new("cargo")
        .args(["build", "--release", "-p", "shelly-mcp"])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Failed to build shelly-mcp:\n{}", stderr);
    }

    println!("   ✅ Build complete");
    Ok(())
}

fn install_binary() -> anyhow::Result<()> {
    println!("📦 Installing Shelly binary...");

    // Find the shelly-mcp binary in the workspace target directory
    let workspace_root = std::env::current_dir()?;
    let target_exe = workspace_root
        .join("target")
        .join("release")
        .join("shelly-mcp");

    if !target_exe.exists() {
        anyhow::bail!(
            "shelly-mcp binary not found at {}. Please run 'cargo build --release' first.",
            target_exe.display()
        );
    }

    let bin_dir = dirs::home_dir()
        .ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?
        .join(".local/bin");

    std::fs::create_dir_all(&bin_dir)?;
    let target_path = bin_dir.join("shelly-mcp");

    std::fs::copy(&target_exe, &target_path)?;

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

    let home_dir =
        dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?;
    let shelly_dir = home_dir.join(".shelly");

    std::fs::create_dir_all(&shelly_dir)?;
    std::fs::create_dir_all(shelly_dir.join("tests"))?;

    println!("   Created directory: {}", shelly_dir.display());
    Ok(())
}

fn create_test_handler() -> anyhow::Result<()> {
    println!("🧪 Creating test handler...");

    let home_dir =
        dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?;
    let shelly_dir = home_dir.join(".shelly");

    // Create api.ts file
    let api_content = r#"/**
 * Shelly Handler API
 * 
 * Handlers process commands before execution and summarize output after.
 * Handlers are stateful - create a new instance for each command execution.
 */

export interface HandlerFactory {
  /**
   * Check if this handler should process the given command.
   * 
   * @param cmd - The command name (e.g., "cargo")
   * @param args - The command arguments (e.g., ["build", "--release"])
   * @returns true if this handler should process the command
   */
  matches(cmd: string, args: string[]): boolean;

  /**
   * Create a new handler instance for a command execution.
   * 
   * @param cmd - The command name
   * @param args - The command arguments
   * @param settings - User-provided settings for this handler
   * @returns A new handler instance
   */
  create(cmd: string, args: string[], settings: Record<string, any>): Handler;

  /**
   * Describe the settings this handler accepts.
   * Used for documentation and validation.
   */
  settings(): SettingsSchema;
}

export interface Handler {
  /**
   * Prepare the command for execution.
   * Can modify the command and set environment variables.
   * 
   * Note: This is skipped when exact: true is set.
   * 
   * @returns Modified command and environment variables
   */
  prepare(): PrepareResult;

  /**
   * Process incremental output chunks.
   * Called repeatedly as output arrives, and once more when complete.
   * 
   * @param stdoutChunk - New stdout data (may be empty)
   * @param stderrChunk - New stderr data (may be empty)
   * @param exitCode - Exit code if complete, null if still running
   * @returns Summary to emit, or null to keep buffering
   */
  summarize(stdoutChunk: string, stderrChunk: string, exitCode: number | null): SummaryResult;
}

export interface PrepareResult {
  /** The command to execute */
  cmd: string;
  /** The command arguments */
  args: string[];
  /** Environment variables to set */
  env: Record<string, string>;
}

export interface SummaryResult {
  /** 
   * Summary text to emit to the agent.
   * Return null to keep buffering (waiting for more output).
   */
  summary: string | null;
  
  /**
   * Optional truncation metadata to help agents understand what was filtered.
   */
  truncation?: TruncationInfo;
}

export interface TruncationInfo {
  /** Whether content was truncated/filtered */
  truncated: boolean;
  /** Reason for truncation */
  reason?: "filtered_noise" | "content_too_large" | "filtered_duplicates";
  /** Human-readable description of what was removed */
  description?: string;
}

export interface SettingsSchema {
  [key: string]: SettingDefinition;
}

export interface SettingDefinition {
  type: "boolean" | "string" | "number";
  default: any;
  description: string;
}
"#;

    std::fs::write(shelly_dir.join("api.ts"), api_content)?;

    // Create shelly-test.ts handler
    let handler_content = r#"import type { HandlerFactory, Handler, PrepareResult, SummaryResult } from "./api.ts";

class ShellyTestHandler implements Handler {
  constructor(
    private cmd: string,
    private args: string[],
    private settings: Record<string, any>
  ) {}

  prepare(): PrepareResult {
    // Replace shelly-test command with echo
    return {
      cmd: "echo",
      args: ["Shelly", "is", "~NOT~", "working!"],
      env: {}
    };
  }

  summarize(stdout: string, stderr: string, exitCode: number | null): SummaryResult {
    if (exitCode === null) return { summary: null };
    
    // Strip out ~NOT~ to show filtering works
    const filtered = stdout.replace(/~NOT~/g, '');
    return { summary: filtered.trim() };
  }
}

export const shellyTestHandler: HandlerFactory = {
  matches(cmd: string, args: string[]): boolean {
    return cmd === "shelly-test";
  },

  create(cmd: string, args: string[], settings: Record<string, any>): Handler {
    return new ShellyTestHandler(cmd, args, settings);
  },

  settings() {
    return {};
  }
};
"#;

    std::fs::write(shelly_dir.join("shelly-test.ts"), handler_content)?;
    println!("   Created shelly-test handler");
    Ok(())
}

fn configure_q_cli_mcp() -> anyhow::Result<()> {
    println!("⚙️  Configuring Q CLI MCP integration...");

    let home_dir =
        dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?;
    let shelly_mcp_path = home_dir.join(".local/bin/shelly-mcp");

    let output = std::process::Command::new("q")
        .args([
            "mcp",
            "add",
            "--name",
            "shelly",
            "--command",
            &shelly_mcp_path.to_string_lossy(),
            "--force",
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
            println!(
                "   📋 Manual setup: Run `q mcp add --name shelly --command {}`",
                shelly_mcp_path.display()
            );
            Ok(())
        }
        Err(e) => {
            println!("   ❌ Could not run 'q mcp add' command: {}", e);
            println!(
                "   📋 Manual setup: Run `q mcp add --name shelly --command {}`",
                shelly_mcp_path.display()
            );
            Ok(())
        }
    }
}
