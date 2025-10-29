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
    
    // Check built-in handlers - look in the shelly crate's handlers directory
    let builtin_handler = PathBuf::from("shelly/handlers").join(format!("{}.ts", cmd_name));
    if builtin_handler.exists() {
        return Ok(Some(builtin_handler));
    }
    
    // Also try relative to the current executable location
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let relative_handler = exe_dir.join("../shelly/handlers").join(format!("{}.ts", cmd_name));
            if relative_handler.exists() {
                return Ok(Some(relative_handler));
            }
        }
    }
    
    Ok(None)
}
