use std::path::PathBuf;
use anyhow::Result;

/// Find handler file for a command
pub fn find_handler(command: &str) -> Result<Option<PathBuf>> {
    // TODO: Implement handler discovery
    // 1. Search .shelly/ from current dir up to $HOME
    // 2. Check built-in handlers
    Ok(None)
}
