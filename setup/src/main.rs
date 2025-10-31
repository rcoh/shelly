#[tokio::main]
async fn main() -> anyhow::Result<()> {
    setup_shelly().await
}

use std::io::BufRead;

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

    // Get available agents
    let agents = get_available_agents()?;
    
    if agents.is_empty() {
        println!("   📋 No agents found");
        return Ok(());
    }

    println!("   📋 Available agents:");
    for (i, agent) in agents.iter().enumerate() {
        println!("   {}. {}", i + 1, agent);
    }
    
    println!();
    println!("Which agents should include Shelly?");
    println!("Enter numbers (e.g., '1,3' or '1-3' or 'all' or 'none'): ");
    
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    let input = input.trim();
    
    if input == "none" {
        println!("   ⏭️  Skipping agent configuration");
        return Ok(());
    }
    
    let selected = if input == "all" {
        (0..agents.len()).collect()
    } else {
        parse_selection(input, agents.len())?
    };
    
    for &idx in &selected {
        let agent_name = &agents[idx];
        add_shelly_to_agent(agent_name, &shelly_mcp_path)?;
    }
    
    Ok(())
}

fn get_available_agents() -> anyhow::Result<Vec<String>> {
    let output = std::process::Command::new("q")
        .args(["agent", "list"])
        .output()
        .map_err(|e| anyhow::anyhow!("❌ Failed to run 'q agent list': {}", e))?;
        
    if !output.status.success() {
        return Ok(Vec::new());
    }
    
    let stderr = String::from_utf8_lossy(&output.stderr);
    let agents: Vec<String> = stderr
        .lines()
        .filter_map(|line| {
            // Strip ANSI color codes
            let clean_line = strip_ansi_codes(line).trim().to_string();
            if clean_line.starts_with("Error:") || clean_line.is_empty() {
                return None;
            }
            let parts: Vec<&str> = clean_line.split_whitespace().collect();
            if parts.len() >= 2 {
                Some(parts[0].to_string())
            } else {
                None
            }
        })
        .collect();
        
    Ok(agents)
}

fn strip_ansi_codes(input: &str) -> String {
    let mut result = String::new();
    let mut chars = input.chars();
    
    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            // Skip escape sequence
            if chars.next() == Some('[') {
                while let Some(c) = chars.next() {
                    if c.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
        } else {
            result.push(ch);
        }
    }
    
    result
}

fn parse_selection(input: &str, max: usize) -> anyhow::Result<Vec<usize>> {
    let mut selected = Vec::new();
    
    for part in input.split(',') {
        let part = part.trim();
        if part.contains('-') {
            let range: Vec<&str> = part.split('-').collect();
            if range.len() != 2 {
                anyhow::bail!("❌ Invalid range format: {}", part);
            }
            let start: usize = range[0].parse().map_err(|_| anyhow::anyhow!("❌ Invalid number: {}", range[0]))?;
            let end: usize = range[1].parse().map_err(|_| anyhow::anyhow!("❌ Invalid number: {}", range[1]))?;
            
            if start == 0 || end == 0 || start > max || end > max {
                anyhow::bail!("❌ Numbers must be between 1 and {}", max);
            }
            
            for i in start..=end {
                selected.push(i - 1);
            }
        } else {
            let num: usize = part.parse().map_err(|_| anyhow::anyhow!("❌ Invalid number: {}", part))?;
            if num == 0 || num > max {
                anyhow::bail!("❌ Number must be between 1 and {}", max);
            }
            selected.push(num - 1);
        }
    }
    
    selected.sort_unstable();
    selected.dedup();
    Ok(selected)
}

fn add_shelly_to_agent(agent_name: &str, shelly_mcp_path: &std::path::Path) -> anyhow::Result<()> {
    println!("   🔧 Adding Shelly to agent '{}'...", agent_name);
    
    let output = std::process::Command::new("q")
        .args([
            "mcp",
            "add",
            "--name",
            "shelly",
            "--command",
            &shelly_mcp_path.to_string_lossy(),
            "--agent",
            agent_name,
            "--force",
        ])
        .output();

    match output {
        Ok(result) if result.status.success() => {
            println!("   ✅ Added to agent '{}'", agent_name);
            Ok(())
        }
        Ok(result) => {
            let stderr = String::from_utf8_lossy(&result.stderr);
            println!("   ❌ Failed to add to agent '{}': {}", agent_name, stderr.trim());
            Ok(())
        }
        Err(e) => {
            println!("   ❌ Error adding to agent '{}': {}", agent_name, e);
            Ok(())
        }
    }
}
