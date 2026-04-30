//! `groundwork-mcp` — Model Context Protocol server for the Groundwork
//! catalogue. Speaks JSON-RPC 2.0 over stdio (one request per line).
//!
//! Methods supported:
//!   - `initialize`            — server info + capabilities
//!   - `notifications/initialized` — no-op acknowledgement
//!   - `tools/list`            — tool catalogue
//!   - `tools/call`            — { name, arguments } → result
//!   - `ping`                  — health check
//!
//! Reads `GROUNDWORK_URL` from env (default `http://localhost:3000`).

use groundwork::mcp::client::GroundworkClient;
use groundwork::mcp::tools::{all_tools, wrap_text_result, Tool};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

const PROTOCOL_VERSION: &str = "2025-06-18";
const SERVER_NAME: &str = "groundwork-mcp";
const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = Arc::new(GroundworkClient::from_env());
    let tools = all_tools();
    eprintln!(
        "{SERVER_NAME} v{SERVER_VERSION} → {} ({} tools)",
        client.base_url(),
        tools.len()
    );

    let stdin = tokio::io::stdin();
    let mut reader = BufReader::new(stdin).lines();
    let stdout = tokio::io::stdout();
    let mut stdout = stdout;

    while let Some(line) = reader.next_line().await? {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let req: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(e) => {
                write_error(&mut stdout, Value::Null, -32700, &format!("parse error: {e}")).await?;
                continue;
            }
        };

        let id = req.get("id").cloned().unwrap_or(Value::Null);
        let method = req
            .get("method")
            .and_then(|m| m.as_str())
            .unwrap_or("")
            .to_string();
        let params = req.get("params").cloned().unwrap_or(Value::Null);
        let is_notification = req.get("id").is_none();

        if method.starts_with("notifications/") {
            // No response expected for notifications.
            continue;
        }

        let result = match method.as_str() {
            "initialize" => Ok(initialize_response()),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(list_tools_response(&tools)),
            "tools/call" => call_tool(&client, &tools, params).await,
            other => Err((-32601, format!("method not found: {other}"))),
        };

        if is_notification {
            continue;
        }

        match result {
            Ok(value) => write_result(&mut stdout, id, value).await?,
            Err((code, msg)) => write_error(&mut stdout, id, code, &msg).await?,
        }
    }
    Ok(())
}

fn initialize_response() -> Value {
    json!({
        "protocolVersion": PROTOCOL_VERSION,
        "capabilities": {
            "tools": { "listChanged": false }
        },
        "serverInfo": {
            "name": SERVER_NAME,
            "version": SERVER_VERSION
        }
    })
}

fn list_tools_response(tools: &[Tool]) -> Value {
    let entries: Vec<Value> = tools
        .iter()
        .map(|t| {
            json!({
                "name": t.name,
                "description": t.description,
                "inputSchema": t.input_schema,
            })
        })
        .collect();
    json!({ "tools": entries })
}

async fn call_tool(
    client: &Arc<GroundworkClient>,
    tools: &[Tool],
    params: Value,
) -> Result<Value, (i64, String)> {
    let name = params
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or((-32602, "missing 'name' in tools/call params".to_string()))?;
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let tool = tools
        .iter()
        .find(|t| t.name == name)
        .ok_or((-32601, format!("unknown tool: {name}")))?;

    match (tool.handler)(client.clone(), args).await {
        Ok(value) => Ok(wrap_text_result(&value)),
        Err(e) => Ok(json!({
            "content": [{ "type": "text", "text": format!("error: {e}") }],
            "isError": true,
        })),
    }
}

async fn write_result<W: AsyncWriteExt + Unpin>(
    out: &mut W,
    id: Value,
    result: Value,
) -> anyhow::Result<()> {
    let frame = json!({ "jsonrpc": "2.0", "id": id, "result": result });
    write_line(out, &frame).await
}

async fn write_error<W: AsyncWriteExt + Unpin>(
    out: &mut W,
    id: Value,
    code: i64,
    message: &str,
) -> anyhow::Result<()> {
    let frame = json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message }
    });
    write_line(out, &frame).await
}

async fn write_line<W: AsyncWriteExt + Unpin>(out: &mut W, frame: &Value) -> anyhow::Result<()> {
    let line = serde_json::to_string(frame)?;
    out.write_all(line.as_bytes()).await?;
    out.write_all(b"\n").await?;
    out.flush().await?;
    Ok(())
}
