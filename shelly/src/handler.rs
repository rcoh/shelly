use anyhow::Result;
use std::{path::PathBuf, fs, io::Write};

// Embed built-in handlers at compile time
const CARGO_HANDLER: &[u8] = include_bytes!("../handlers/cargo.ts");

/// Find handler file for a command
/// Searches in order: ~/.shelly, CWD/.shelly, built-in handlers
pub fn find_handler(command: &str) -> Result<Option<PathBuf>> {
    let cmd_name = command
        .split_whitespace()
        .next()
        .ok_or_else(|| anyhow::anyhow!("Empty command"))?;

    let handler_filename = format!("{}.ts", cmd_name);

    // 1. Check ~/.shelly
    if let Some(home_dir) = dirs::home_dir() {
        let home_handler = home_dir.join(".shelly").join(&handler_filename);
        if home_handler.exists() {
            return Ok(Some(home_handler));
        }
    }

    // 2. Check CWD/.shelly
    let cwd_handler = PathBuf::from(".shelly").join(&handler_filename);
    if cwd_handler.exists() {
        return Ok(Some(cwd_handler));
    }

    // 3. Check built-in handlers
    let builtin_content = match cmd_name {
        "cargo" => Some(CARGO_HANDLER),
        _ => None,
    };

    if let Some(content) = builtin_content {
        // Create temp file with built-in handler content
        // Use the original handler name to ensure proper module loading
        let temp_dir = std::env::temp_dir();
        let temp_handler = temp_dir.join(handler_filename);
        let mut file = fs::File::create(&temp_handler)?;
        file.write_all(content)?;
        return Ok(Some(temp_handler));
    }

    Ok(None)
}
