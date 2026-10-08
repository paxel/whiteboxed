//! The MCP endpoint over real HTTP, with a plain socket as the client.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};
use whiteboxed::editor::Editor;
use whiteboxed::mcp::server::{Call, Server};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const TOKEN: &str = "test-token-0123456789";

struct Reply {
    status: u16,
    session: Option<String>,
    body: String,
}

fn post(
    addr: SocketAddr,
    path: &str,
    body: &Value,
    headers: &[(&str, &str)],
) -> std::io::Result<Reply> {
    let body = body.to_string();
    let mut s = TcpStream::connect(addr)?;
    let mut req = format!(
        "POST {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nContent-Type: application/json\r\n\
         Accept: application/json, text/event-stream\r\nConnection: close\r\nContent-Length: {}\r\n",
        addr.port(),
        body.len()
    );
    for (k, v) in headers {
        req.push_str(&format!("{k}: {v}\r\n"));
    }
    req.push_str("\r\n");
    req.push_str(&body);
    s.write_all(req.as_bytes())?;
    let mut raw = String::new();
    s.read_to_string(&mut raw)?;
    let (head, rest) = raw.split_once("\r\n\r\n").unwrap_or((&raw, ""));
    let status = head
        .split_whitespace()
        .nth(1)
        .and_then(|c| c.parse().ok())
        .unwrap_or(0);
    let session = head.lines().find_map(|l| {
        l.to_lowercase()
            .strip_prefix("mcp-session-id:")
            .map(|_| l[15..].trim().to_owned())
    });
    Ok(Reply {
        status,
        session,
        body: rest.to_owned(),
    })
}

/// The JSON-RPC message with `id` in a JSON or SSE (possibly chunked) body.
fn message(body: &str, id: u64) -> Option<Value> {
    if let Ok(v) = serde_json::from_str::<Value>(body)
        && v["id"] == id
    {
        return Some(v);
    }
    body.lines()
        .filter_map(|l| l.strip_prefix("data:"))
        .filter_map(|d| serde_json::from_str::<Value>(d.trim()).ok())
        .find(|v| v["id"] == id)
}

struct Fixture {
    server: Server,
    editor: Arc<Mutex<Editor>>,
}

fn start() -> std::io::Result<Fixture> {
    let (tx, rx) = mpsc::channel::<Call>();
    let editor = Arc::new(Mutex::new(Editor::new(None)));
    let pump = editor.clone();
    // Stands in for the UI thread.
    std::thread::spawn(move || {
        while let Ok(call) = rx.recv() {
            if let Ok(mut e) = pump.lock() {
                call.run(&mut e);
            }
        }
    });
    let server = Server::start(0, TOKEN.into(), tx, Arc::new(|| {}))?;
    Ok(Fixture { server, editor })
}

fn rpc(id: u64, method: &str, params: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
}

#[test]
fn the_endpoint_needs_the_token() -> TestResult {
    let f = start()?;
    let init = rpc(
        1,
        "initialize",
        json!({"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "t", "version": "1"}}),
    );
    assert_eq!(post(f.server.addr, "/mcp", &init, &[])?.status, 401);
    assert_eq!(
        post(
            f.server.addr,
            "/mcp",
            &init,
            &[("Authorization", "Bearer wrong")]
        )?
        .status,
        401
    );
    let ok = post(f.server.addr, &format!("/mcp?token={TOKEN}"), &init, &[])?;
    assert_eq!(ok.status, 200);
    Ok(())
}

#[test]
fn a_client_lists_and_calls_tools() -> TestResult {
    let f = start()?;
    let auth = format!("Bearer {TOKEN}");
    let init = rpc(
        1,
        "initialize",
        json!({"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "t", "version": "1"}}),
    );
    let r = post(f.server.addr, "/mcp", &init, &[("Authorization", &auth)])?;
    assert_eq!(r.status, 200, "{}", r.body);
    let hello = message(&r.body, 1).ok_or("no initialize result")?;
    assert!(
        hello["result"]["instructions"]
            .as_str()
            .is_some_and(|s| s.contains("arc42"))
    );
    assert_eq!(hello["result"]["serverInfo"]["name"], "whiteboxed");
    let session = r.session.ok_or("no session id")?;
    let h = [
        ("Authorization", auth.as_str()),
        ("Mcp-Session-Id", session.as_str()),
        ("MCP-Protocol-Version", "2025-06-18"),
    ];

    let note = json!({"jsonrpc": "2.0", "method": "notifications/initialized"});
    assert_eq!(post(f.server.addr, "/mcp", &note, &h)?.status, 202);

    let list = post(f.server.addr, "/mcp", &rpc(2, "tools/list", json!({})), &h)?;
    let tools = message(&list.body, 2).ok_or("no tools/list result")?;
    assert_eq!(tools["result"]["tools"].as_array().map(Vec::len), Some(17));

    let call = rpc(
        3,
        "tools/call",
        json!({"name": "add_box", "arguments": {"name": "Web Shop", "kind": "component"}}),
    );
    let added =
        message(&post(f.server.addr, "/mcp", &call, &h)?.body, 3).ok_or("no add_box result")?;
    let text = added["result"]["content"][0]["text"]
        .as_str()
        .ok_or("no text")?;
    assert_eq!(serde_json::from_str::<Value>(text)?["name"], "Web Shop");
    assert_eq!(
        f.editor
            .lock()
            .map_err(|e| e.to_string())?
            .project
            .blocks
            .len(),
        1
    );

    let dup = rpc(
        4,
        "tools/call",
        json!({"name": "add_box", "arguments": {"name": "web shop", "kind": "component"}}),
    );
    let refused = message(&post(f.server.addr, "/mcp", &dup, &h)?.body, 4).ok_or("no result")?;
    assert_eq!(refused["result"]["isError"], true);
    assert!(
        refused["result"]["content"][0]["text"]
            .as_str()
            .is_some_and(|t| t.contains("already exists"))
    );

    let render = rpc(
        5,
        "tools/call",
        json!({"name": "render_diagram", "arguments": {}}),
    );
    let image =
        message(&post(f.server.addr, "/mcp", &render, &h)?.body, 5).ok_or("no render result")?;
    assert_eq!(image["result"]["content"][0]["type"], "image");
    assert_eq!(image["result"]["content"][0]["mimeType"], "image/png");
    Ok(())
}

#[test]
fn a_taken_port_fails_at_once() -> TestResult {
    let f = start()?;
    let (tx, _rx) = mpsc::channel::<Call>();
    let again = Server::start(f.server.addr.port(), TOKEN.into(), tx, Arc::new(|| {}));
    assert!(again.is_err());
    Ok(())
}

/// Serves on 127.0.0.1:7399 for a minute, for trying real MCP clients by hand.
#[test]
#[ignore = "runs a server for manual client tests"]
fn serve_for_manual_clients() -> TestResult {
    let (tx, rx) = mpsc::channel::<Call>();
    let _server = Server::start(7399, TOKEN.into(), tx, Arc::new(|| {}))?;
    let mut editor = Editor::new(None);
    let until = std::time::Instant::now() + std::time::Duration::from_secs(60);
    while std::time::Instant::now() < until {
        if let Ok(call) = rx.recv_timeout(std::time::Duration::from_millis(100)) {
            call.run(&mut editor);
        }
    }
    Ok(())
}
