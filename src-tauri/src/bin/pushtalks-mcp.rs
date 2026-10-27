//! `pushtalks-mcp`: the PushTalks connector for Claude Code and Claude Desktop (MCP over stdio).
//! Reads one JSON-RPC message per line on stdin and answers on stdout. Never logs transcript text.

use std::io::{BufRead, Write};

use pushtalks_lib::db::Library;
use pushtalks_lib::mcp::Server;
use pushtalks_lib::settings::app_data_dir;

fn main() {
    let lib = match Library::open(&app_data_dir().join("library.sqlite")) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("pushtalks-mcp: can't open the PushTalks library: {e}");
            std::process::exit(1);
        }
    };
    let mut server = Server::new(lib);
    let stdin = std::io::stdin();
    let mut out = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<serde_json::Value>(&line) {
            Ok(msg) => server.handle(&msg),
            Err(_) => Some(serde_json::json!({
                "jsonrpc": "2.0", "id": null,
                "error": { "code": -32700, "message": "parse error" }
            })),
        };
        if let Some(r) = reply {
            if writeln!(out, "{r}").and_then(|_| out.flush()).is_err() {
                break;
            }
        }
    }
}
