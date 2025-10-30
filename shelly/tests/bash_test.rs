use shelly::{execute_command, ExecuteRequest};
use std::collections::HashMap;

#[tokio::test]
async fn test_bash_command() {
    let result = execute_command(ExecuteRequest {
        command: "bash -c \"echo 'Starting...'; sleep 1; echo 'Done!'\"".to_string(),
        working_dir: "/tmp".into(),
        exact: true, // No handlers
        settings: HashMap::new(),
        env: HashMap::new(),
    })
    .await
    .unwrap();

    println!("Exit code: {}", result.exit_code);
    println!("Summary: '{}'", result.summary);
    println!("Output file: {}", result.output_file);

    // Read the actual output from the file
    let output = tokio::fs::read_to_string(&result.output_file)
        .await
        .unwrap();
    println!("File contents: '{}'", output);

    assert_eq!(result.exit_code, 0);
    assert!(output.contains("Starting..."));
    assert!(output.contains("Done!"));
}

#[tokio::test]
async fn test_bash_command_separate_args() {
    // Test with command and args as separate fields - but ExecuteRequest doesn't have args field
    // So let's test with the command string approach
    let result = execute_command(ExecuteRequest {
        command: "bash -c \"echo 'Hello'; sleep 1; echo 'World'\"".to_string(),
        working_dir: "/tmp".into(),
        exact: true,
        settings: HashMap::new(),
        env: HashMap::new(),
    })
    .await
    .unwrap();

    println!("Exit code: {}", result.exit_code);
    println!("Summary: '{}'", result.summary);

    let output = tokio::fs::read_to_string(&result.output_file)
        .await
        .unwrap();
    println!("File contents: '{}'", output);

    assert_eq!(result.exit_code, 0);
    assert!(output.contains("Hello"));
    assert!(output.contains("World"));
}
