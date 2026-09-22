//! Drives the real compiled `opencut-mcp` binary over its real stdio
//! transport with hand-written JSON-RPC, exactly as an MCP client (Claude
//! Code, or any other agent) would. This is the one test that proves an
//! agent can actually create a project, import a clip, trim it, and export
//! a file through tool calls, not just that the Rust functions behind them
//! work in isolation.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

struct McpProcess {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl McpProcess {
    fn spawn() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_opencut-mcp"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("failed to spawn the opencut-mcp binary");
        let stdin = child.stdin.take().expect("child stdin was not piped");
        let stdout = BufReader::new(child.stdout.take().expect("child stdout was not piped"));
        Self {
            child,
            stdin,
            stdout,
            next_id: 1,
        }
    }

    fn send(&mut self, message: Value) {
        let line = serde_json::to_string(&message).expect("request must serialize to JSON");
        writeln!(self.stdin, "{line}").expect("failed to write to the server's stdin");
        self.stdin
            .flush()
            .expect("failed to flush the server's stdin");
    }

    /// Reads one newline-delimited JSON-RPC message, per the wire framing
    /// rmcp's stdio transport actually uses (confirmed against its own
    /// JsonRpcMessageCodec, not assumed).
    fn recv(&mut self) -> Value {
        let mut line = String::new();
        let bytes = self
            .stdout
            .read_line(&mut line)
            .expect("failed to read from the server's stdout");
        assert!(
            bytes > 0,
            "the server closed its stdout before sending a response"
        );
        serde_json::from_str(&line)
            .unwrap_or_else(|err| panic!("response was not valid JSON: {err}\nline: {line}"))
    }

    fn initialize(&mut self) {
        self.send(json!({
            "jsonrpc": "2.0",
            "id": 0,
            "method": "initialize",
            "params": {
                "protocolVersion": "2026-07-28",
                "capabilities": {},
                "clientInfo": { "name": "opencut-mcp-smoke-test", "version": "0.0.1" }
            }
        }));
        let response = self.recv();
        assert!(
            response.get("result").is_some(),
            "initialize failed: {response}"
        );
        self.send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }));
    }

    /// Calls a tool and returns its structured result. Panics with the
    /// server's own error message on either a protocol-level rejection or a
    /// tool-level failure, so a failing assertion here points straight at
    /// what OpenCut itself said was wrong.
    fn call(&mut self, name: &str, arguments: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        self.send(json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": { "name": name, "arguments": arguments }
        }));
        let response = self.recv();
        if let Some(error) = response.get("error") {
            panic!("{name} was rejected before it ran: {error}");
        }
        let result = &response["result"];
        if result.get("isError") == Some(&Value::Bool(true)) {
            panic!("{name} returned an error: {result}");
        }
        result
            .get("structuredContent")
            .cloned()
            .unwrap_or_else(|| result["content"].clone())
    }
}

impl Drop for McpProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn ffmpeg_available() -> bool {
    Command::new("ffmpeg").arg("-version").output().is_ok()
}

/// A short, unique-enough suffix for temp file names. `SystemTime` rather
/// than a `uuid` dev-dependency, since this test needs nothing stronger.
fn unique_suffix() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is before the Unix epoch")
        .as_nanos()
}

#[test]
fn edits_and_exports_a_project_end_to_end_over_mcp() {
    if !ffmpeg_available() {
        eprintln!("skipping: ffmpeg not found on PATH");
        return;
    }

    let temp_dir = std::env::temp_dir();
    let clip_path = temp_dir.join(format!("opencut-mcp-smoke-{}.mp4", unique_suffix()));
    let output_path = temp_dir.join(format!("opencut-mcp-smoke-out-{}.mp4", unique_suffix()));

    let generated = Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc=duration=2:size=64x36:rate=10",
            "-pix_fmt",
            "yuv420p",
        ])
        .arg(&clip_path)
        .status()
        .expect("ffmpeg must run to generate the fixture");
    assert!(
        generated.success(),
        "ffmpeg failed to generate the smoke test's fixture clip"
    );

    let mut mcp = McpProcess::spawn();
    mcp.initialize();

    mcp.call("new_project", json!({ "name": "Smoke Test" }));

    let clip = mcp.call(
        "import_clip",
        json!({ "path": clip_path.to_string_lossy() }),
    );
    let clip_id = clip["id"]
        .as_str()
        .expect("import_clip did not return a clip id")
        .to_string();
    let imported_duration = clip["duration"]
        .as_f64()
        .expect("import_clip did not return a duration");
    assert!(
        (imported_duration - 2.0).abs() < 0.2,
        "expected the fixture's ~2s duration, got {imported_duration}"
    );

    let after_trim = mcp.call(
        "trim_clip",
        json!({ "clip_id": clip_id, "in_point": 0.0, "out_point": 1.0 }),
    );
    assert_eq!(
        after_trim["total_duration"].as_f64(),
        Some(1.0),
        "trim did not change the project's total duration: {after_trim}"
    );

    let export_result = mcp.call(
        "export_project",
        json!({ "output_path": output_path.to_string_lossy() }),
    );
    let export_text = export_result
        .as_array()
        .and_then(|blocks| blocks.first())
        .and_then(|block| block["text"].as_str())
        .unwrap_or_default();
    assert!(
        export_text.contains("Exported"),
        "export_project's response did not confirm the export: {export_result}"
    );

    assert!(
        output_path.exists(),
        "export_project reported success but wrote no file at {}",
        output_path.display()
    );
    let exported_info =
        opencut_core::ffmpeg::probe(&output_path).expect("could not probe the exported file");
    assert!(
        (exported_info.duration - 1.0).abs() < 0.2,
        "expected the export to be ~1s (the trimmed length), got {}",
        exported_info.duration
    );

    std::fs::remove_file(&clip_path).ok();
    std::fs::remove_file(&output_path).ok();
}

#[test]
fn rejects_a_tool_call_before_any_project_exists() {
    let mut mcp = McpProcess::spawn();
    mcp.initialize();

    mcp.send(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "get_project", "arguments": {} }
    }));
    let response = mcp.recv();

    // A propagated ErrorData surfaces as a JSON-RPC protocol-level error
    // (code -32602, invalid params), not as a successful CallToolResult with
    // isError: true. Confirmed by running this test, not assumed.
    let error = response
        .get("error")
        .unwrap_or_else(|| panic!("expected a JSON-RPC error, got: {response}"));
    let message = error["message"].as_str().unwrap_or_default();
    assert!(
        message.contains("new_project"),
        "the error should tell the caller to call new_project, got: {message}"
    );
}
