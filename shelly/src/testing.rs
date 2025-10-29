use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use anyhow::Result;

#[derive(Debug, Serialize, Deserialize)]
pub struct TestInput {
    pub command: String,
    pub settings: HashMap<String, serde_json::Value>,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TestOutput {
    pub summary: String,
}

pub struct TestCase {
    pub name: String,
    pub input: TestInput,
    pub expected: TestOutput,
}

/// Find all test cases for a handler
pub fn find_tests(handler_name: &str) -> Result<Vec<TestCase>> {
    let test_dir = PathBuf::from(".shelly/tests").join(handler_name);
    if !test_dir.exists() {
        return Ok(Vec::new());
    }

    let mut tests = Vec::new();
    for entry in std::fs::read_dir(&test_dir)? {
        let entry = entry?;
        let path = entry.path();
        
        if path.is_file() && path.extension().map_or(false, |e| e == "toml") {
            let name = path.file_stem().unwrap().to_string_lossy();
            if name.starts_with("input-") {
                let test_name = name.strip_prefix("input-").unwrap();
                let output_path = test_dir.join(format!("output-{}.toml", test_name));
                
                if output_path.exists() {
                    let input: TestInput = toml::from_str(&std::fs::read_to_string(&path)?)?;
                    let expected: TestOutput = toml::from_str(&std::fs::read_to_string(&output_path)?)?;
                    
                    tests.push(TestCase {
                        name: test_name.to_string(),
                        input,
                        expected,
                    });
                }
            }
        }
    }

    tests.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(tests)
}

/// Run a single test case
pub async fn run_test(handler_path: &Path, test: &TestCase) -> Result<TestResult> {
    let mut rt = crate::runtime::HandlerRuntime::new()?;
    rt.load_handler(handler_path.to_str().unwrap()).await?;
    
    rt.create_handler(&test.input.command, &test.input.settings).await?;
    rt.prepare().await?;
    
    let result = rt.summarize(
        &test.input.stdout,
        &test.input.stderr,
        Some(test.input.exit_code),
    ).await?;
    
    let actual_summary = result.summary.unwrap_or_default();
    let passed = actual_summary == test.expected.summary;
    
    Ok(TestResult {
        name: test.name.clone(),
        passed,
        expected: test.expected.summary.clone(),
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

/// Update test snapshots
pub async fn update_snapshot(handler_path: &Path, handler_name: &str, test: &TestCase) -> Result<()> {
    let mut rt = crate::runtime::HandlerRuntime::new()?;
    rt.load_handler(handler_path.to_str().unwrap()).await?;
    
    rt.create_handler(&test.input.command, &test.input.settings).await?;
    rt.prepare().await?;
    
    let result = rt.summarize(
        &test.input.stdout,
        &test.input.stderr,
        Some(test.input.exit_code),
    ).await?;
    
    let output = TestOutput {
        summary: result.summary.unwrap_or_default(),
    };
    
    let output_path = PathBuf::from(".shelly/tests")
        .join(handler_name)
        .join(format!("output-{}.toml", test.name));
    
    std::fs::write(
        output_path,
        toml::to_string_pretty(&output)?,
    )?;
    
    Ok(())
}
