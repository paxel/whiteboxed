//! The MCP endpoint: Streamable HTTP on 127.0.0.1, guarded by the access token. It
//! runs on its own thread with a tokio runtime; every tool call is handed to the UI
//! thread, which owns the editor, and the answer comes back over a oneshot channel.

use std::net::{SocketAddr, TcpListener};
use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::thread::JoinHandle;
use std::time::Duration;

use axum::extract::Request;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation,
    ListToolsResult, PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool,
};
use rmcp::service::RequestContext;
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
};
use rmcp::{ErrorData, RoleServer, ServerHandler};
use serde_json::Value;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

use crate::api::{self, ApiError, Output};

/// How long a call waits for the UI thread (which may be paused while minimised).
pub const CALL_TIMEOUT: Duration = Duration::from_secs(30);

/// One tool call for the UI thread to run.
pub struct Call {
    pub tool: String,
    pub args: Value,
    pub reply: oneshot::Sender<Result<Output, ApiError>>,
}

impl Call {
    /// Runs the call against an editor and sends the answer back.
    pub fn run(self, editor: &mut crate::editor::Editor) {
        let result = api::call(editor, &self.tool, self.args);
        // The client may have given up already; nothing to do then.
        let _ = self.reply.send(result);
    }
}

/// Wakes the UI thread when a call is waiting.
pub type Wake = Arc<dyn Fn() + Send + Sync>;

#[derive(Clone)]
struct Handler {
    calls: Sender<Call>,
    wake: Wake,
}

impl ServerHandler for Handler {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("whiteboxed", env!("CARGO_PKG_VERSION")))
            .with_instructions(api::INSTRUCTIONS)
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        let tools = api::tools()
            .into_iter()
            .map(|t| Tool::new(t.name, t.description, Arc::new(t.schema)))
            .collect();
        Ok(ListToolsResult::with_all_items(tools))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let args = request.arguments.map_or(Value::Null, Value::Object);
        let (reply, answer) = oneshot::channel();
        let call = Call {
            tool: request.name.to_string(),
            args,
            reply,
        };
        if self.calls.send(call).is_err() {
            return Err(ErrorData::internal_error("whiteboxed is closing", None));
        }
        (self.wake)();
        let result = match tokio::time::timeout(CALL_TIMEOUT, answer).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => return Err(ErrorData::internal_error("the call was dropped", None)),
            Err(_) => {
                return Ok(CallToolResult::error(vec![ContentBlock::text(
                    "whiteboxed did not answer in time; is the window minimised?",
                )])
                .into());
            }
        };
        Ok(match result {
            Ok(Output::Json(v)) => CallToolResult::success(vec![ContentBlock::text(
                serde_json::to_string_pretty(&v).unwrap_or_default(),
            )]),
            Ok(Output::Png(png)) => CallToolResult::success(vec![ContentBlock::image(
                Output::png_base64(&png),
                "image/png",
            )]),
            Err(ApiError::UnknownTool(t)) => {
                return Err(ErrorData::invalid_params(format!("unknown tool {t}"), None));
            }
            Err(e) => CallToolResult::error(vec![ContentBlock::text(e.to_string())]),
        }
        .into())
    }
}

/// A running endpoint. Dropping it stops the server.
pub struct Server {
    pub addr: SocketAddr,
    cancel: CancellationToken,
    thread: Option<JoinHandle<()>>,
}

impl Server {
    /// Binds `127.0.0.1:port` right away (so a taken port fails here) and serves
    /// `/mcp` on a background thread. Port 0 picks a free port.
    pub fn start(
        port: u16,
        token: String,
        calls: Sender<Call>,
        wake: Wake,
    ) -> std::io::Result<Server> {
        let listener = TcpListener::bind(("127.0.0.1", port))?;
        listener.set_nonblocking(true)?;
        let addr = listener.local_addr()?;
        let cancel = CancellationToken::new();
        let stop = cancel.clone();
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()?;
        let thread = std::thread::Builder::new()
            .name("whiteboxed-mcp".into())
            .spawn(move || {
                runtime.block_on(async move {
                    let Ok(listener) = tokio::net::TcpListener::from_std(listener) else {
                        return;
                    };
                    let handler = Handler { calls, wake };
                    let config = StreamableHttpServerConfig::default()
                        .with_cancellation_token(stop.child_token());
                    let service: StreamableHttpService<Handler, LocalSessionManager> =
                        StreamableHttpService::new(
                            move || Ok(handler.clone()),
                            Default::default(),
                            config,
                        );
                    let token = Arc::new(token);
                    let router = axum::Router::new().nest_service("/mcp", service).layer(
                        axum::middleware::from_fn(move |req, next| {
                            let token = token.clone();
                            async move { authorise(&token, req, next).await }
                        }),
                    );
                    let _ = axum::serve(listener, router)
                        .with_graceful_shutdown(async move { stop.cancelled_owned().await })
                        .await;
                });
            })?;
        Ok(Server {
            addr,
            cancel,
            thread: Some(thread),
        })
    }

    /// The URL clients connect to.
    pub fn url(&self) -> String {
        format!("http://{}/mcp", self.addr)
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.cancel.cancel();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Lets a request through only with the token, as `Authorization: Bearer <token>`
/// or as `?token=<token>`.
async fn authorise(token: &str, req: Request, next: Next) -> Response {
    let bearer = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .is_some_and(|t| constant_time_eq(t.trim(), token));
    let query = req.uri().query().is_some_and(|q| {
        q.split('&')
            .filter_map(|pair| pair.strip_prefix("token="))
            .any(|t| constant_time_eq(t, token))
    });
    if bearer || query {
        next.run(req).await
    } else {
        (
            StatusCode::UNAUTHORIZED,
            "whiteboxed: missing or wrong access token",
        )
            .into_response()
    }
}

fn constant_time_eq(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}
