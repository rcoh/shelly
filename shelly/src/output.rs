use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use anyhow::{Context, Result};

const MAX_AGE_SECS: u64 = 86400; // 1 day

/// Get the shelly output directory, creating it if needed
pub fn output_dir() -> Result<PathBuf> {
    let dir = std::env::temp_dir().join("shelly");
    fs::create_dir_all(&dir).context("Failed to create output directory")?;
    Ok(dir)
}

/// Create a new output file for a command
pub fn create_output_file(command: &str) -> Result<PathBuf> {
    let dir = output_dir()?;
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_millis();
    let safe_cmd = command.chars()
        .take(20)
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
        .collect::<String>();
    let filename = format!("{}-{}.txt", safe_cmd, timestamp);
    Ok(dir.join(filename))
}

/// Write output to a file
pub fn write_output(path: &Path, stdout: &str, stderr: &str, exit_code: i32) -> Result<()> {
    let content = format!(
        "Exit Code: {}\n\n=== STDOUT ===\n{}\n\n=== STDERR ===\n{}",
        exit_code, stdout, stderr
    );
    fs::write(path, content).context("Failed to write output file")?;
    Ok(())
}

/// Clean up old output files
pub fn cleanup_old_files() -> Result<()> {
    let dir = output_dir()?;
    let now = SystemTime::now();
    
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let metadata = entry.metadata()?;
        
        if let Ok(modified) = metadata.modified() {
            if let Ok(age) = now.duration_since(modified) {
                if age.as_secs() > MAX_AGE_SECS {
                    fs::remove_file(entry.path())?;
                }
            }
        }
    }
    
    Ok(())
}
