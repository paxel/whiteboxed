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
    // A browser page (it sends Origin) is refused even with the right token.
    let auth = format!("Bearer {TOKEN}");
    let from_web = post(
        f.server.addr,
        "/mcp",
        &init,
        &[("Authorization", &auth), ("Origin", "https://evil.example")],
    )?;
    assert_eq!(from_web.status, 403);
    // Only loopback host names are served (DNS rebinding).
    let mut s = std::net::TcpStream::connect(f.server.addr)?;
    let body = init.to_string();
    write!(
        s,
        "POST /mcp HTTP/1.1\r\nHost: attacker.example\r\nAuthorization: {auth}\r\nContent-Type: application/json\r\nAccept: application/json, text/event-stream\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )?;
    let mut raw = String::new();
    s.read_to_string(&mut raw)?;
    assert!(raw.starts_with("HTTP/1.1 403"), "{raw}");
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
    assert_eq!(tools["result"]["tools"].as_array().map(Vec::len), Some(23));

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

#[test]
fn a_call_its_client_gave_up_on_changes_nothing() -> TestResult {
    let mut editor = Editor::new(None);
    let (reply, answer) = tokio::sync::oneshot::channel();
    drop(answer);
    Call {
        tool: "add_box".into(),
        args: json!({"name": "Late", "kind": "component"}),
        reply,
    }
    .run(&mut editor);
    assert!(editor.project.blocks.is_empty());
    assert!(!editor.can_undo());
    Ok(())
}

#[test]
fn stopping_with_a_call_in_flight_is_quick_and_frees_the_port() -> TestResult {
    let port = std::net::TcpListener::bind("127.0.0.1:0")?
        .local_addr()?
        .port();
    // Nobody pumps this channel: the call stays in flight.
    let (tx, _rx) = mpsc::channel::<Call>();
    let server = Server::start(port, TOKEN.into(), tx, Arc::new(|| {}))?;
    let addr = server.addr;
    let auth = format!("Bearer {TOKEN}");
    let init = rpc(
        1,
        "initialize",
        json!({"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "t", "version": "1"}}),
    );
    let session = post(addr, "/mcp", &init, &[("Authorization", &auth)])?
        .session
        .ok_or("session")?;
    let note = json!({"jsonrpc": "2.0", "method": "notifications/initialized"});
    let h = [
        ("Authorization", auth.clone()),
        ("Mcp-Session-Id", session),
        ("MCP-Protocol-Version", "2025-06-18".to_owned()),
    ];
    let headers: Vec<(&str, &str)> = h.iter().map(|(k, v)| (*k, v.as_str())).collect();
    post(addr, "/mcp", &note, &headers)?;
    let owned: Vec<(String, String)> = h
        .iter()
        .map(|(k, v)| ((*k).to_owned(), v.clone()))
        .collect();
    let pending = std::thread::spawn(move || {
        let headers: Vec<(&str, &str)> = owned
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        let call = rpc(
            2,
            "tools/call",
            json!({"name": "get_model", "arguments": {}}),
        );
        post(addr, "/mcp", &call, &headers).map(|r| r.body)
    });
    std::thread::sleep(std::time::Duration::from_millis(300));
    let started = std::time::Instant::now();
    drop(server);
    assert!(
        started.elapsed() < std::time::Duration::from_secs(2),
        "took {:?}",
        started.elapsed()
    );
    let body = pending.join().map_err(|_| "client thread")??;
    if let Some(answer) = message(&body, 2) {
        assert!(
            answer["result"]["content"][0]["text"]
                .as_str()
                .is_some_and(|t| t.contains("turned off"))
        );
    }
    let (tx, _rx) = mpsc::channel::<Call>();
    let again = Server::start(port, TOKEN.into(), tx, Arc::new(|| {}));
    assert!(again.is_ok(), "the port is free again");
    Ok(())
}

/// Status of an initialize request sent with this Host header and the token.
fn status_for_host(addr: SocketAddr, host: &str) -> Result<String, Box<dyn std::error::Error>> {
    let body = rpc(
        1,
        "initialize",
        json!({"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "t", "version": "1"}}),
    )
    .to_string();
    let mut s = TcpStream::connect(addr)?;
    write!(
        s,
        "POST /mcp HTTP/1.1\r\nHost: {host}\r\nAuthorization: Bearer {TOKEN}\r\nContent-Type: application/json\r\nAccept: application/json, text/event-stream\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )?;
    let mut raw = String::new();
    s.read_to_string(&mut raw)?;
    Ok(raw.chars().take(12).collect())
}

#[test]
fn containers_get_in_only_when_docker_access_is_chosen() -> TestResult {
    use whiteboxed::mcp::listen::{DOCKER_HOST, Endpoint};
    // This computer only: a container's Host header is refused.
    let f = start()?;
    let port = f.server.addr.port();
    assert_eq!(
        status_for_host(f.server.addr, &format!("{DOCKER_HOST}:{port}"))?,
        "HTTP/1.1 403"
    );
    // Docker access (as Docker Desktop forwards it to 127.0.0.1): accepted.
    let (tx, _rx) = mpsc::channel::<Call>();
    let docker = Endpoint {
        bind: "127.0.0.1".parse()?,
        hosts: vec!["127.0.0.1".into(), DOCKER_HOST.into()],
        client_host: DOCKER_HOST.into(),
    };
    let server = Server::start_at(&docker, 0, TOKEN.into(), tx, Arc::new(|| {}))?;
    let port = server.addr.port();
    assert_eq!(
        status_for_host(server.addr, &format!("{DOCKER_HOST}:{port}"))?,
        "HTTP/1.1 200"
    );
    assert_eq!(
        status_for_host(server.addr, "attacker.example")?,
        "HTTP/1.1 403"
    );
    Ok(())
}
