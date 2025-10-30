use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use anyhow::Result;

#[derive(Debug, Serialize, Deserialize)]
pub struct TestCase {
    pub command: String,
    pub settings: HashMap<String, serde_json::Value>,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub expected_summary: String,
}

/// Find all test cases for a handler
/// Searches .shelly/tests/ in CWD first, then falls back to built-in tests
pub fn find_tests(handler_name: &str) -> Result<Vec<(String, TestCase)>> {
    let mut tests = Vec::new();
    
    // Check .shelly/tests/ in current directory first
    let local_test_dir = PathBuf::from(".shelly/tests").join(handler_name);
    if local_test_dir.exists() {
        load_tests_from_dir(&local_test_dir, &mut tests)?;
    }
    
    // Check built-in tests if no local tests found
    if tests.is_empty() {
        let builtin_test_dir = PathBuf::from("tests").join(handler_name);
        if builtin_test_dir.exists() {
            load_tests_from_dir(&builtin_test_dir, &mut tests)?;
        }
    }

    tests.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(tests)
}

fn load_tests_from_dir(test_dir: &Path, tests: &mut Vec<(String, TestCase)>) -> Result<()> {
    for entry in std::fs::read_dir(test_dir)? {
        let entry = entry?;
        let path = entry.path();
        
        if path.is_file() && path.extension().is_some_and(|e| e == "toml") {
            let name = path.file_stem().unwrap().to_string_lossy().to_string();
            let test: TestCase = toml::from_str(&std::fs::read_to_string(&path)?)?;
            tests.push((name, test));
        }
    }
    Ok(())
}

/// Run a single test case
pub async fn run_test(handler_path: &Path, name: &str, test: &TestCase) -> Result<TestResult> {
    let mut rt = crate::runtime::HandlerRuntime::new()?;
    rt.load_handler(handler_path.to_str().unwrap()).await?;
    
    rt.create_handler(&test.command, &test.settings).await?;
    rt.prepare().await?;
    
    let result = rt.summarize(
        &test.stdout,
        &test.stderr,
        Some(test.exit_code),
    ).await?;
    
    let actual_summary = result.summary.unwrap_or_default();
    
    // Trim leading/trailing whitespace for comparison
    let expected_trimmed = test.expected_summary.trim();
    let actual_trimmed = actual_summary.trim();
    let passed = actual_trimmed == expected_trimmed;
    
    Ok(TestResult {
        name: name.to_string(),
        passed,
        expected: test.expected_summary.clone(),
        actual: actual_summary,
    })
}

#[derive(Debug)]
pub struct TestResult {
    pub name: String,
    pub passed: bool,
    pub expected: String,
    pub actual: String,
}

/// Update test snapshot
pub async fn update_snapshot(handler_path: &Path, handler_name: &str, name: &str, test: &mut TestCase) -> Result<()> {
    let mut rt = crate::runtime::HandlerRuntime::new()?;
    rt.load_handler(handler_path.to_str().unwrap()).await?;
    
    rt.create_handler(&test.command, &test.settings).await?;
    rt.prepare().await?;
    
    let result = rt.summarize(
        &test.stdout,
        &test.stderr,
        Some(test.exit_code),
    ).await?;
    
    test.expected_summary = result.summary.unwrap_or_default();
    
    let test_path = PathBuf::from(".shelly/tests")
        .join(handler_name)
        .join(format!("{}.toml", name));
    
    // Ensure directory exists
    if let Some(parent) = test_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    
    std::fs::write(
        test_path,
        toml::to_string_pretty(&test)?,
    )?;
    
    Ok(())
}
