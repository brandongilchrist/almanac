//! Black-box integration test for the almanac-mcp stdio server.
//!
//! Spawns the `almanac-mcp` binary, speaks MCP JSON-RPC over its stdin/stdout,
//! and asserts the server: initializes, lists all 9 tools, and correctly
//! answers `check_lineage` against the seeded demo community.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};

struct McpSession {
    child: Child,
    out: BufReader<std::process::ChildStdout>,
}

impl McpSession {
    fn spawn() -> Self {
        let bin = env!("CARGO_BIN_EXE_almanac-mcp");
        let mut child = Command::new(bin)
            .env("ALMANAC_MCP_SEED", "1")
            .env("ALMANAC_COMMUNITY", "demo")
            .env("RUST_LOG", "") // keep stdout clean
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn almanac-mcp");
        let out = BufReader::new(child.stdout.take().expect("stdout"));
        Self { child, out }
    }

    fn send(&mut self, line: &str) {
        let stdin = self.child.stdin.as_mut().expect("stdin");
        stdin.write_all(line.as_bytes()).unwrap();
        stdin.write_all(b"\n").unwrap();
        stdin.flush().unwrap();
    }

    /// Read one JSON-RPC response (one line of stdout), skipping any non-JSON.
    fn recv(&mut self) -> serde_json::Value {
        let mut line = String::new();
        loop {
            line.clear();
            if self.out.read_line(&mut line).unwrap() == 0 {
                panic!("almanac-mcp stdout closed before a response");
            }
            let trimmed = line.trim();
            if trimmed.starts_with('{') {
                return serde_json::from_str(trimmed).expect("valid JSON-RPC");
            }
            // skip stderr noise or empty lines
        }
    }

    fn call_tool(&mut self, name: &str, args: serde_json::Value) -> serde_json::Value {
        self.send(
            &serde_json::json!({
                "jsonrpc": "2.0",
                "id": 99,
                "method": "tools/call",
                "params": { "name": name, "arguments": args }
            })
            .to_string(),
        );
        self.recv()
    }
}

impl Drop for McpSession {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn initializes_and_lists_all_tools() {
    let mut s = McpSession::spawn();
    // initialize
    s.send(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"t","version":"0"}}}"#);
    let init = s.recv();
    assert_eq!(init["jsonrpc"], "2.0");
    assert_eq!(init["id"], 1);
    assert!(init["result"]["capabilities"]["tools"].is_object());
    // initialized notification (no response expected)
    s.send(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#);
    // tools/list
    s.send(r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#);
    let list = s.recv();
    let tools = list["result"]["tools"].as_array().unwrap();
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    for expected in [
        "register_agent",
        "create_schedule",
        "list_schedules",
        "declare_contract",
        "record_manifest",
        "check_lineage",
        "trigger_run",
        "update_run_status",
        "get_calendar",
    ] {
        assert!(
            names.contains(&expected),
            "missing tool {expected}; got {names:?}"
        );
    }
    // Each tool has an inputSchema.
    for t in tools {
        assert!(t["inputSchema"]["type"] == "object", "tool missing schema");
    }
}

#[test]
fn check_lineage_returns_ready_for_seeded_demo() {
    let mut s = McpSession::spawn();
    s.send(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"t","version":"0"}}}"#);
    let _ = s.recv();
    s.send(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#);

    let res = s.call_tool(
        "check_lineage",
        serde_json::json!({"schedule_id": "weekly-strategy", "community": "demo"}),
    );
    assert_eq!(res["result"]["isError"], false);
    let text = res["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("weekly-strategy"), "resp: {text}");
    // The seeded manifest is fresh + v3 >= min v2 → ready.
    assert!(text.contains("✅"), "expected ready marker; got: {text}");
}

#[test]
fn create_schedule_then_list_shows_it() {
    let mut s = McpSession::spawn();
    s.send(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"t","version":"0"}}}"#);
    let _ = s.recv();
    s.send(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#);

    let _ = s.call_tool(
        "create_schedule",
        serde_json::json!({"schedule_id": "my-test-sched", "summary": "My Test Schedule"}),
    );
    let res = s.call_tool("list_schedules", serde_json::json!({"community": "demo"}));
    let text = res["result"]["content"][0]["text"].as_str().unwrap();
    assert!(
        text.contains("my-test-sched"),
        "list missing new sched: {text}"
    );
}

#[test]
fn get_calendar_returns_json_with_agents() {
    let mut s = McpSession::spawn();
    s.send(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"t","version":"0"}}}"#);
    let _ = s.recv();
    s.send(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#);

    let res = s.call_tool("get_calendar", serde_json::json!({"community": "demo"}));
    let text = res["result"]["content"][0]["text"].as_str().unwrap();
    let snap: serde_json::Value = serde_json::from_str(text).expect("get_calendar returns JSON");
    assert!(snap["agents"].is_array(), "no agents: {snap}");
    assert!(
        snap["agents"].as_array().unwrap().len() >= 4,
        "demo seeds 4 agents"
    );
    assert!(snap["schedules"].is_array());
}
