use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::time::{sleep, Duration};

#[derive(Serialize, Deserialize)]
struct JsonRpcRequest<T> {
    jsonrpc: String,
    id: Option<u32>,
    method: String,
    params: Option<T>,
}

#[derive(Serialize, Deserialize)]
struct InitializeParams {
    #[serde(rename = "protocolVersion")]
    protocol_version: String,
    capabilities: Value,
    #[serde(rename = "clientInfo")]
    client_info: ClientInfo,
}

#[derive(Serialize, Deserialize)]
struct ClientInfo {
    name: String,
    version: String,
}

#[derive(Serialize, Deserialize)]
struct ToolCallParams {
    name: String,
    arguments: Value,
}

#[derive(Serialize, Deserialize)]
struct ExecuteArgs {
    command: String,
    args: Vec<String>,
    working_dir: String,
    timeout_ms: u64,
    disable_enhancements: bool,
}

#[derive(Serialize, Deserialize)]
struct ProcessStatusArgs {
    process_id: String,
}

struct McpClient {
    stdin: tokio::process::ChildStdin,
    reader: tokio::io::Lines<BufReader<tokio::process::ChildStdout>>,
    request_id: u32,
}

impl McpClient {
    async fn send_request<T: Serialize>(&mut self, method: &str, params: Option<T>) -> String {
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(self.request_id),
            method: method.to_string(),
            params,
        };
        self.request_id += 1;

        let request_str = serde_json::to_string(&request).unwrap();
        self.stdin
            .write_all(format!("{}\n", request_str).as_bytes())
            .await
            .unwrap();

        self.reader.next_line().await.unwrap().unwrap()
    }

    async fn send_notification<T: Serialize>(&mut self, method: &str, params: Option<T>) {
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: None,
            method: method.to_string(),
            params,
        };

        let request_str = serde_json::to_string(&request).unwrap();
        self.stdin
            .write_all(format!("{}\n", request_str).as_bytes())
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn test_process_status_excludes_raw_output() {
    let mut child = Command::new("cargo")
        .args(&["run", "--bin", "shelly-mcp"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start MCP server");

    let mut client = McpClient {
        stdin: child.stdin.take().unwrap(),
        reader: BufReader::new(child.stdout.take().unwrap()).lines(),
        request_id: 1,
    };

    // Initialize
    let init_params = InitializeParams {
        protocol_version: "2024-11-05".to_string(),
        capabilities: serde_json::json!({}),
        client_info: ClientInfo {
            name: "test-client".to_string(),
            version: "1.0.0".to_string(),
        },
    };

    client.send_request("initialize", Some(init_params)).await;
    client
        .send_notification::<Value>("notifications/initialized", None)
        .await;

    // Execute command that generates output
    let execute_args = ExecuteArgs {
        command: "echo".to_string(),
        args: vec!["hello world from test".to_string()],
        working_dir: "/tmp".to_string(),
        timeout_ms: 5000,
        disable_enhancements: true,
    };

    let tool_params = ToolCallParams {
        name: "execute_cli".to_string(),
        arguments: serde_json::to_value(execute_args).unwrap(),
    };

    let response = client.send_request("tools/call", Some(tool_params)).await;
    let response_json: Value = serde_json::from_str(&response).unwrap();
    let structured_content = &response_json["result"]["structuredContent"];
    
    // Extract process_id
    let process_id = structured_content["process_id"]
        .as_str()
        .expect("No process_id in execute result");

    // Wait a moment for the process to complete
    sleep(Duration::from_millis(100)).await;

    // Get process status
    let status_args = ProcessStatusArgs {
        process_id: process_id.to_string(),
    };

    let status_params = ToolCallParams {
        name: "process_status".to_string(),
        arguments: serde_json::to_value(status_args).unwrap(),
    };

    let status_response = client.send_request("tools/call", Some(status_params)).await;
    let status_json: Value = serde_json::from_str(&status_response).unwrap();
    let status_data = &status_json["result"]["structuredContent"];

    // Verify the structured content contains ProcessStatus fields
    assert!(status_data["stdout_length"].is_number(), "Should have stdout_length");
    assert!(status_data["stderr_length"].is_number(), "Should have stderr_length");
    
    // Should NOT have raw output fields
    assert!(status_data["raw_stdout"].is_null(), "Should not have raw_stdout");
    assert!(status_data["raw_stderr"].is_null(), "Should not have raw_stderr");

    // Should have other expected fields
    assert!(status_data["id"].is_string(), "Should have id");
    assert!(status_data["command"].is_string(), "Should have command");
    assert!(status_data["state"].is_object(), "Should have state");
    assert!(status_data["started_at"].is_object(), "Should have started_at");

    // Verify stdout_length is reasonable (should be > 0 for echo command)
    let stdout_length = status_data["stdout_length"].as_u64().unwrap();
    assert!(stdout_length > 0, "stdout_length should be > 0 for echo command");

    // Clean up
    child.kill().await.ok();
}
