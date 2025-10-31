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
        .output()
        .map_err(|e| anyhow::anyhow!(
            "❌ Failed to run cargo command: {}\n💡 Ensure cargo is installed and in PATH", e
        ))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        
        let mut error_msg = String::from("❌ Build failed\n");
        
        if !stderr.is_empty() {
            error_msg.push_str(&format!("📋 Error output:\n{}\n", stderr));
        }
        if !stdout.is_empty() {
            error_msg.push_str(&format!("📋 Build output:\n{}\n", stdout));
        }
        
        error_msg.push_str("💡 Try: cargo clean && cargo build --release -p shelly-mcp");
        
        anyhow::bail!(error_msg);
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
            "❌ Binary not found: {}\n💡 Solution: Run 'cargo build --release -p shelly-mcp' first",
            target_exe.display()
        );
    }

    let bin_dir = dirs::home_dir()
        .ok_or_else(|| anyhow::anyhow!("❌ Could not find home directory"))?
        .join(".local/bin");

    if let Err(e) = std::fs::create_dir_all(&bin_dir) {
        anyhow::bail!(
            "❌ Failed to create bin directory: {}\n💡 Check permissions for: {}",
            e,
            bin_dir.display()
        );
    }

    let target_path = bin_dir.join("shelly-mcp");

    // Handle "Text file busy" error with retry and better messaging
    match std::fs::copy(&target_exe, &target_path) {
        Ok(_) => {},
        Err(e) if e.raw_os_error() == Some(26) => {
            anyhow::bail!(
                "❌ Cannot install binary - file is currently in use (Text file busy)\n\
                 💡 The shelly-mcp process is likely running. Try:\n\
                    • Stop any running Q CLI sessions\n\
                    • Kill shelly-mcp processes: pkill shelly-mcp\n\
                    • Wait a moment and try again\n\
                 📍 Target: {}",
                target_path.display()
            );
        },
        Err(e) => {
            anyhow::bail!(
                "❌ Failed to copy binary: {}\n\
                 📍 From: {}\n\
                 📍 To: {}\n\
                 💡 Check file permissions and disk space",
                e,
                target_exe.display(),
                target_path.display()
            );
        }
    }

    // Make executable on Unix systems
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(e) = std::fs::metadata(&target_path)
            .and_then(|metadata| {
                let mut perms = metadata.permissions();
                perms.set_mode(0o755);
                std::fs::set_permissions(&target_path, perms)
            })
        {
            anyhow::bail!(
                "❌ Failed to set executable permissions: {}\n📍 File: {}",
                e,
                target_path.display()
            );
        }
    }

    println!("   ✅ Installed to: {}", target_path.display());
    Ok(())
}

fn create_shelly_directory() -> anyhow::Result<()> {
    println!("📁 Creating .shelly directory structure...");

    let home_dir =
        dirs::home_dir().ok_or_else(|| anyhow::anyhow!("❌ Could not find home directory"))?;
    let shelly_dir = home_dir.join(".shelly");

    std::fs::create_dir_all(&shelly_dir)
        .map_err(|e| anyhow::anyhow!(
            "❌ Failed to create .shelly directory: {}\n📍 Path: {}\n💡 Check permissions",
            e, shelly_dir.display()
        ))?;
        
    std::fs::create_dir_all(shelly_dir.join("tests"))
        .map_err(|e| anyhow::anyhow!(
            "❌ Failed to create tests directory: {}\n📍 Path: {}/tests",
            e, shelly_dir.display()
        ))?;

    println!("   ✅ Created directory: {}", shelly_dir.display());
    Ok(())
}

fn create_test_handler() -> anyhow::Result<()> {
    println!("🧪 Creating test handler...");

    let home_dir =
        dirs::home_dir().ok_or_else(|| anyhow::anyhow!("❌ Could not find home directory"))?;
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

    let api_path = shelly_dir.join("api.ts");
    std::fs::write(&api_path, api_content)
        .map_err(|e| anyhow::anyhow!(
            "❌ Failed to create api.ts: {}\n📍 Path: {}",
            e, api_path.display()
        ))?;

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

    let handler_path = shelly_dir.join("shelly-test.ts");
    std::fs::write(&handler_path, handler_content)
        .map_err(|e| anyhow::anyhow!(
            "❌ Failed to create shelly-test.ts: {}\n📍 Path: {}",
            e, handler_path.display()
        ))?;
        
    println!("   ✅ Created test handler files");
    Ok(())
}

fn configure_q_cli_mcp() -> anyhow::Result<()> {
    println!("⚙️  Configuring Q CLI MCP integration...");

    let home_dir =
        dirs::home_dir().ok_or_else(|| anyhow::anyhow!("❌ Could not find home directory"))?;
    let shelly_mcp_path = home_dir.join(".local/bin/shelly-mcp");

    if !shelly_mcp_path.exists() {
        anyhow::bail!(
            "❌ Binary not found: {}\n💡 Installation may have failed",
            shelly_mcp_path.display()
        );
    }

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
            let stdout = String::from_utf8_lossy(&result.stdout);
            
            println!("   ❌ Failed to register MCP server with Q CLI");
            if !stderr.is_empty() {
                println!("   📋 Error: {}", stderr.trim());
            }
            if !stdout.is_empty() {
                println!("   📋 Output: {}", stdout.trim());
            }
            println!("   💡 Manual setup: q mcp add --name shelly --command {}", shelly_mcp_path.display());
            println!("   💡 Check: q mcp list (to see registered servers)");
            Ok(())
        }
        Err(e) => {
            println!("   ❌ Could not run 'q mcp add': {}", e);
            println!("   💡 Ensure Q CLI is installed and in PATH");
            println!("   💡 Manual setup: q mcp add --name shelly --command {}", shelly_mcp_path.display());
            Ok(())
        }
    }
}
