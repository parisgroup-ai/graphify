//! Integration tests for the graphify-mcp binary.
//!
//! These tests spawn the `graphify-mcp` binary as a child process, communicate
//! via newline-delimited JSON-RPC 2.0 over stdio, and verify the MCP lifecycle:
//!
//!   initialize → notifications/initialized → tools/list → tools/call
//!
//! If the binary has not been built yet, the tests are skipped gracefully.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

/// Locate the `graphify-mcp` binary.
///
/// First checks the standard cargo target/debug location relative to the
/// workspace root. Falls back to `CARGO_BIN_EXE_graphify-mcp` if set.
fn find_binary() -> Option<PathBuf> {
    // The integration test binary runs from somewhere inside target/; walk up
    // to find the workspace root by looking for the top-level Cargo.toml that
    // contains [workspace].
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // manifest_dir = crates/graphify-mcp, workspace root is two levels up.
    let workspace_root = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .expect("cannot determine workspace root");

    let candidates = [
        workspace_root.join("target/debug/graphify-mcp"),
        workspace_root.join("target/release/graphify-mcp"),
    ];

    for candidate in &candidates {
        if candidate.is_file() {
            return Some(candidate.clone());
        }
    }

    // Last resort: env var set by `cargo test` when the binary is a test
    // dependency (only works with `cargo test -p graphify-mcp`).
    option_env!("CARGO_BIN_EXE_graphify-mcp").map(PathBuf::from)
}

/// Sends a JSON-RPC request (with `id`) and reads exactly one response line.
///
/// Uses a 30-second timeout to avoid hanging forever if the server is stuck.
fn send_and_receive(
    stdin: &mut std::process::ChildStdin,
    stdout: &mut BufReader<std::process::ChildStdout>,
    request: &serde_json::Value,
) -> serde_json::Value {
    let msg = serde_json::to_string(request).expect("failed to serialize request");
    stdin.write_all(msg.as_bytes()).expect("write request");
    stdin.write_all(b"\n").expect("write newline");
    stdin.flush().expect("flush stdin");

    let mut line = String::new();
    // BufReader::read_line will block until a full line is available.
    // We rely on the test timeout (or cargo test --timeout) to avoid infinite hangs.
    stdout.read_line(&mut line).expect("read response line");
    assert!(
        !line.is_empty(),
        "server closed stdout without sending a response"
    );

    serde_json::from_str(&line).unwrap_or_else(|e| {
        panic!("failed to parse server response as JSON: {e}\nraw line: {line:?}")
    })
}

/// Sends a JSON-RPC notification (no `id`, no response expected).
fn send_notification(stdin: &mut std::process::ChildStdin, notification: &serde_json::Value) {
    let msg = serde_json::to_string(notification).expect("failed to serialize notification");
    stdin.write_all(msg.as_bytes()).expect("write notification");
    stdin.write_all(b"\n").expect("write newline");
    stdin.flush().expect("flush stdin");
}

/// Creates a minimal Python project under the given directory.
///
/// Layout:
/// ```text
/// <dir>/
///   myproject/
///     __init__.py    (empty, marks as package)
///     main.py        (from myproject import utils)
///     utils.py       (def helper(): return 42)
///     models.py      (class User: pass)
/// ```
fn create_fixture_project(dir: &Path) {
    let pkg_dir = dir.join("myproject");
    std::fs::create_dir_all(&pkg_dir).expect("create myproject dir");

    std::fs::write(pkg_dir.join("__init__.py"), "").expect("write __init__.py");

    std::fs::write(
        pkg_dir.join("main.py"),
        "from myproject import utils\nfrom myproject import models\n\ndef run():\n    utils.helper()\n",
    )
    .expect("write main.py");

    std::fs::write(pkg_dir.join("utils.py"), "def helper():\n    return 42\n")
        .expect("write utils.py");

    std::fs::write(pkg_dir.join("models.py"), "class User:\n    pass\n").expect("write models.py");
}

/// Creates a `graphify.toml` pointing at the fixture project.
fn create_config(dir: &Path) -> PathBuf {
    let config_path = dir.join("graphify.toml");
    let repo_path = dir.to_string_lossy();
    let content = format!(
        r#"[settings]
output = "./report"

[[project]]
name = "test-project"
repo = "{repo_path}"
lang = ["python"]
local_prefix = "myproject"
"#
    );
    std::fs::write(&config_path, content).expect("write graphify.toml");
    config_path
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn mcp_lifecycle_initialize_list_call() {
    // --- Find binary (skip gracefully if not built) --------------------------
    let binary = match find_binary() {
        Some(b) => b,
        None => {
            eprintln!(
                "SKIP: graphify-mcp binary not found. Run `cargo build -p graphify-mcp` first."
            );
            return;
        }
    };

    eprintln!("Using binary: {}", binary.display());

    // --- Create temp fixture -------------------------------------------------
    let tmp = tempfile::tempdir().expect("create temp dir");
    create_fixture_project(tmp.path());
    let config_path = create_config(tmp.path());

    eprintln!("Fixture dir: {}", tmp.path().display());
    eprintln!("Config: {}", config_path.display());

    // --- Spawn the MCP server ------------------------------------------------
    let mut child = Command::new(&binary)
        .arg("--config")
        .arg(&config_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn graphify-mcp");

    let mut child_stdin = child.stdin.take().expect("take stdin");
    let child_stdout = child.stdout.take().expect("take stdout");
    let child_stderr = child.stderr.take().expect("take stderr");

    let mut stdout_reader = BufReader::new(child_stdout);

    // Spawn a thread to drain stderr so it doesn't block the child.
    let stderr_thread = std::thread::spawn(move || {
        let reader = BufReader::new(child_stderr);
        let mut lines = Vec::new();
        for line in reader.lines() {
            match line {
                Ok(l) => {
                    eprintln!("[graphify-mcp stderr] {l}");
                    lines.push(l);
                }
                Err(_) => break,
            }
        }
        lines
    });

    // --- 1. Initialize -------------------------------------------------------
    eprintln!("Sending initialize...");
    let init_request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {
                "name": "integration-test",
                "version": "0.1.0"
            }
        }
    });

    let init_response = send_and_receive(&mut child_stdin, &mut stdout_reader, &init_request);
    eprintln!(
        "Initialize response: {}",
        serde_json::to_string_pretty(&init_response).unwrap()
    );

    // Validate initialize response
    assert_eq!(
        init_response["jsonrpc"], "2.0",
        "response must be JSON-RPC 2.0"
    );
    assert_eq!(init_response["id"], 1, "response id must match request id");
    assert!(
        init_response.get("result").is_some(),
        "initialize must return a result, got: {init_response}"
    );

    // Verify serverInfo is present. Note: rmcp's Implementation::from_build_env()
    // uses its own crate name ("rmcp"), not the binary's. We just check it exists.
    let server_info = &init_response["result"]["serverInfo"];
    assert!(
        server_info["name"].is_string(),
        "serverInfo.name must be a string, got: {server_info}"
    );
    assert!(
        server_info["version"].is_string(),
        "serverInfo.version must be a string, got: {server_info}"
    );

    // Verify protocolVersion is returned.
    assert!(
        init_response["result"]["protocolVersion"].is_string(),
        "protocolVersion must be present in initialize result"
    );

    // --- 2. Send initialized notification ------------------------------------
    eprintln!("Sending initialized notification...");
    let initialized_notification = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    });
    send_notification(&mut child_stdin, &initialized_notification);

    // Brief pause to let the server process the notification.
    std::thread::sleep(Duration::from_millis(200));

    // --- 3. tools/list -------------------------------------------------------
    eprintln!("Sending tools/list...");
    let list_request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/list",
        "params": {}
    });

    let list_response = send_and_receive(&mut child_stdin, &mut stdout_reader, &list_request);
    eprintln!(
        "tools/list response: {}",
        serde_json::to_string_pretty(&list_response).unwrap()
    );

    assert_eq!(list_response["jsonrpc"], "2.0");
    assert_eq!(list_response["id"], 2);
    assert!(
        list_response.get("result").is_some(),
        "tools/list must return a result"
    );

    let tools = list_response["result"]["tools"]
        .as_array()
        .expect("result.tools must be an array");

    eprintln!("Found {} tools", tools.len());
    assert!(
        tools.len() >= 8,
        "expected at least 8 tools, got {}",
        tools.len()
    );

    // Verify some expected tool names are present.
    let tool_names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
    eprintln!("Tool names: {:?}", tool_names);

    assert!(
        tool_names.contains(&"graphify_stats"),
        "tools should include graphify_stats"
    );
    assert!(
        tool_names.contains(&"graphify_search"),
        "tools should include graphify_search"
    );
    assert!(
        tool_names.contains(&"graphify_explain"),
        "tools should include graphify_explain"
    );
    assert!(
        tool_names.contains(&"graphify_path"),
        "tools should include graphify_path"
    );

    // --- 4. tools/call: graphify_stats ---------------------------------------
    eprintln!("Sending tools/call graphify_stats...");
    let stats_request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "graphify_stats",
            "arguments": {}
        }
    });

    let stats_response = send_and_receive(&mut child_stdin, &mut stdout_reader, &stats_request);
    eprintln!(
        "graphify_stats response: {}",
        serde_json::to_string_pretty(&stats_response).unwrap()
    );

    assert_eq!(stats_response["jsonrpc"], "2.0");
    assert_eq!(stats_response["id"], 3);
    assert!(
        stats_response.get("result").is_some(),
        "tools/call must return a result"
    );

    // The result should have content[0].text with JSON containing node_count.
    let content = stats_response["result"]["content"]
        .as_array()
        .expect("result.content must be an array");
    assert!(!content.is_empty(), "result.content must not be empty");

    let text = content[0]["text"]
        .as_str()
        .expect("content[0].text must be a string");

    let stats: serde_json::Value =
        serde_json::from_str(text).expect("stats text must be valid JSON");
    eprintln!(
        "Parsed stats: {}",
        serde_json::to_string_pretty(&stats).unwrap()
    );

    let node_count = stats["node_count"]
        .as_u64()
        .expect("stats must have node_count");
    assert!(
        node_count >= 3,
        "expected at least 3 nodes (myproject, main, utils, models), got {node_count}"
    );

    // --- 5. tools/call: graphify_search --------------------------------------
    eprintln!("Sending tools/call graphify_search...");
    let search_request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": {
            "name": "graphify_search",
            "arguments": {
                "pattern": "myproject.*"
            }
        }
    });

    let search_response = send_and_receive(&mut child_stdin, &mut stdout_reader, &search_request);
    eprintln!(
        "graphify_search response: {}",
        serde_json::to_string_pretty(&search_response).unwrap()
    );

    assert_eq!(search_response["jsonrpc"], "2.0");
    assert_eq!(search_response["id"], 4);
    assert!(
        search_response.get("result").is_some(),
        "graphify_search must return a result"
    );

    let search_content = search_response["result"]["content"]
        .as_array()
        .expect("result.content must be an array");
    let search_text = search_content[0]["text"]
        .as_str()
        .expect("content[0].text must be a string");
    let search_results: serde_json::Value =
        serde_json::from_str(search_text).expect("search results must be valid JSON");

    let matches = search_results
        .as_array()
        .expect("search results must be an array");
    assert!(
        !matches.is_empty(),
        "search for 'myproject.*' should return at least one match"
    );

    // --- Cleanup: close stdin to signal the server to shut down --------------
    drop(child_stdin);

    // Wait for the child with a timeout.
    let status = child.wait().expect("failed to wait for child process");
    eprintln!("graphify-mcp exited with: {status}");

    // Collect stderr for diagnostics.
    let stderr_lines = stderr_thread.join().expect("stderr thread panicked");
    eprintln!("Captured {} stderr lines", stderr_lines.len());
}

#[test]
fn mcp_tools_call_explain() {
    // --- Find binary (skip gracefully if not built) --------------------------
    let binary = match find_binary() {
        Some(b) => b,
        None => {
            eprintln!(
                "SKIP: graphify-mcp binary not found. Run `cargo build -p graphify-mcp` first."
            );
            return;
        }
    };

    // --- Create temp fixture -------------------------------------------------
    let tmp = tempfile::tempdir().expect("create temp dir");
    create_fixture_project(tmp.path());
    let config_path = create_config(tmp.path());

    // --- Spawn the MCP server ------------------------------------------------
    let mut child = Command::new(&binary)
        .arg("--config")
        .arg(&config_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn graphify-mcp");

    let mut child_stdin = child.stdin.take().expect("take stdin");
    let child_stdout = child.stdout.take().expect("take stdout");
    let child_stderr = child.stderr.take().expect("take stderr");

    let mut stdout_reader = BufReader::new(child_stdout);

    // Drain stderr in background.
    let stderr_thread = std::thread::spawn(move || {
        let reader = BufReader::new(child_stderr);
        for line in reader.lines() {
            if let Ok(l) = line {
                eprintln!("[graphify-mcp stderr] {l}");
            }
        }
    });

    // --- Initialize handshake ------------------------------------------------
    let init_request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": "test", "version": "0.1.0" }
        }
    });
    let init_response = send_and_receive(&mut child_stdin, &mut stdout_reader, &init_request);
    assert!(init_response.get("result").is_some(), "initialize failed");

    send_notification(
        &mut child_stdin,
        &serde_json::json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
    );
    std::thread::sleep(Duration::from_millis(200));

    // --- tools/call: graphify_explain ----------------------------------------
    let explain_request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "graphify_explain",
            "arguments": {
                "node_id": "myproject.main"
            }
        }
    });

    let explain_response = send_and_receive(&mut child_stdin, &mut stdout_reader, &explain_request);
    eprintln!(
        "graphify_explain response: {}",
        serde_json::to_string_pretty(&explain_response).unwrap()
    );

    assert_eq!(explain_response["jsonrpc"], "2.0");
    assert_eq!(explain_response["id"], 2);
    assert!(explain_response.get("result").is_some());

    let content = explain_response["result"]["content"]
        .as_array()
        .expect("result.content must be an array");
    let text = content[0]["text"]
        .as_str()
        .expect("content[0].text must be a string");

    let explain: serde_json::Value =
        serde_json::from_str(text).expect("explain text must be valid JSON");

    // The explain result should reference the node we asked about.
    assert_eq!(
        explain["node_id"].as_str().unwrap_or(""),
        "myproject.main",
        "explain should return the requested node_id"
    );

    // --- Cleanup -------------------------------------------------------------
    drop(child_stdin);
    child.wait().ok();
    stderr_thread.join().ok();
}
