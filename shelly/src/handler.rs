use std::path::PathBuf;
use anyhow::Result;

/// Find handler file for a command
/// Searches .shelly/ in CWD, then falls back to built-in handlers
pub fn find_handler(command: &str) -> Result<Option<PathBuf>> {
    let cmd_name = command.split_whitespace().next()
        .ok_or_else(|| anyhow::anyhow!("Empty command"))?;
    
    // Check .shelly/ in current directory
    let local_handler = PathBuf::from(".shelly").join(format!("{}.ts", cmd_name));
    if local_handler.exists() {
        return Ok(Some(local_handler));
    }
    
    // Check built-in handlers
    let builtin_handler = PathBuf::from("handlers").join(format!("{}.ts", cmd_name));
    if builtin_handler.exists() {
        return Ok(Some(builtin_handler));
    }
    
    Ok(None)
}
