use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

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
struct JoinArgs {
    process_id: String,
    timeout_ms: u64,
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

        self.stdin
            .write_all(format!("{}\n", serde_json::to_string(&request).unwrap()).as_bytes())
            .await
            .unwrap();
        self.stdin.flush().await.unwrap();

        let response = self.reader.next_line().await.unwrap().unwrap();
        self.request_id += 1;
        response
    }

    async fn send_notification<T: Serialize>(&mut self, method: &str, params: Option<T>) {
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: None,
            method: method.to_string(),
            params,
        };

        self.stdin
            .write_all(format!("{}\n", serde_json::to_string(&request).unwrap()).as_bytes())
            .await
            .unwrap();
        self.stdin.flush().await.unwrap();
    }
}

#[tokio::test]
async fn test_mcp_execute_and_join() {
    tracing_subscriber::fmt::init();
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

    let response = client.send_request("initialize", Some(init_params)).await;
    println!("Initialize response: {}", response);

    client
        .send_notification::<Value>("notifications/initialized", None)
        .await;

    // Execute command
    let execute_args = ExecuteArgs {
        command: "bash".to_string(),
        args: vec![
            "-c".to_string(),
            "echo Starting; sleep 2; echo Middle; sleep 1; echo Done".to_string(),
        ],
        working_dir: "/tmp".to_string(),
        timeout_ms: 1000,
        disable_enhancements: true,
    };

    let tool_params = ToolCallParams {
        name: "execute_cli".to_string(),
        arguments: serde_json::to_value(execute_args).unwrap(),
    };

    let response = client.send_request("tools/call", Some(tool_params)).await;
    println!("Execute response: {}", response);

    let response_json: Value = serde_json::from_str(&response).unwrap();
    let structured_content = &response_json["result"]["structuredContent"];

    // Check if command completed immediately or needs polling
    if let Some(is_running) = structured_content.get("is_running") {
        if !is_running.as_bool().unwrap_or(true) {
            // Command completed immediately, validate output
            let mut stdout = structured_content["stdout"]
                .as_str()
                .unwrap_or("")
                .to_string();

            // If stdout is empty, try reading from output file
            if stdout.is_empty() {
                if let Some(output_file) = structured_content
                    .get("output_file")
                    .and_then(|f| f.as_str())
                {
                    stdout = tokio::fs::read_to_string(output_file)
                        .await
                        .unwrap_or_default();
                }
            }

            println!("Immediate stdout: {}", stdout);

            assert!(stdout.contains("Starting"));
            assert!(stdout.contains("Middle"));
            assert!(stdout.contains("Done"));

            child.kill().await.ok();
            return;
        }
    }

    let process_id = structured_content["process_id"].as_str().unwrap();
    let output_file = structured_content["output_file"].as_str().unwrap();

    // Poll for completion
    let final_response = loop {
        let join_args = JoinArgs {
            process_id: process_id.to_string(),
            timeout_ms: 1000,
        };

        let tool_params = ToolCallParams {
            name: "join_process".to_string(),
            arguments: serde_json::to_value(join_args).unwrap(),
        };

        let response = client.send_request("tools/call", Some(tool_params)).await;
        let join_json: Value = serde_json::from_str(&response).unwrap();
        let structured_content = &join_json["result"]["structuredContent"];

        if is_completed(structured_content) {
            break join_json;
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    };

    let file_output = tokio::fs::read_to_string(output_file).await.unwrap();

    println!("Final file output: {file_output}");

    assert!(file_output.contains("Starting"));
    assert!(file_output.contains("Middle"));
    assert!(file_output.contains("Done"));

    child.kill().await.ok();
}

fn is_completed(content: &Value) -> bool {
    dbg!(content);
    content["status"]
        .as_object()
        .map(|status| status.contains_key("Completed"))
        .unwrap_or(false)
}
