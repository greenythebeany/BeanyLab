//! Local HTTP server that exposes the session API and serves the built UI.
//! Development aid: lets the desktop UI run in a normal browser. Binds to 127.0.0.1 only.

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use beany_core::Session;
use tiny_http::{Header, Method, Response, Server};

pub fn run(args: &[String]) {
    let mut port = 7777u16;
    let mut ui = PathBuf::from("dist");
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--port" => {
                port = args.get(i + 1).and_then(|p| p.parse().ok()).unwrap_or(port);
                i += 1;
            }
            "--ui" => {
                ui = PathBuf::from(args.get(i + 1).cloned().unwrap_or_default());
                i += 1;
            }
            "--cwd" => {
                if let Some(d) = args.get(i + 1) {
                    let _ = std::env::set_current_dir(d);
                }
                i += 1;
            }
            _ => {}
        }
        i += 1;
    }
    let ui = std::fs::canonicalize(&ui).unwrap_or(ui);
    let session = Arc::new(Mutex::new(Session::new()));
    let interrupt = session.lock().unwrap().interp.interrupt.clone();
    let server = Server::http(("127.0.0.1", port)).expect("failed to bind port");
    println!(
        "BeanyLab UI on http://127.0.0.1:{}  (serving {})",
        port,
        ui.display()
    );
    for mut req in server.incoming_requests() {
        let session = session.clone();
        let interrupt = interrupt.clone();
        let ui = ui.clone();
        std::thread::spawn(move || {
            let url = req.url().to_string();
            if let Some(method) = url.strip_prefix("/api/") {
                if *req.method() != Method::Post {
                    let _ = req.respond(Response::from_string("POST only").with_status_code(405));
                    return;
                }
                if method == "interrupt" {
                    interrupt.store(true, Ordering::Relaxed);
                    let _ = req.respond(json_response("null".into(), 200));
                    return;
                }
                let mut body = String::new();
                let _ = req.as_reader().read_to_string(&mut body);
                let args: serde_json::Value =
                    serde_json::from_str(&body).unwrap_or(serde_json::Value::Null);
                let result = session.lock().unwrap().call(method, &args);
                let resp = match result {
                    Ok(v) => json_response(v.to_string(), 200),
                    Err(e) => json_response(serde_json::json!({ "error": e }).to_string(), 400),
                };
                let _ = req.respond(resp);
                return;
            }
            let _ = req.respond(static_file(&ui, &url));
        });
    }
}

fn json_response(body: String, code: u16) -> Response<std::io::Cursor<Vec<u8>>> {
    Response::from_string(body)
        .with_status_code(code)
        .with_header(Header::from_bytes("Content-Type", "application/json").unwrap())
}

fn static_file(root: &Path, url: &str) -> Response<std::io::Cursor<Vec<u8>>> {
    let path = url.split('?').next().unwrap_or("/");
    let rel = if path == "/" {
        "index.html"
    } else {
        path.trim_start_matches('/')
    };
    let full = root.join(rel);
    // Refuse paths that escape the UI folder.
    let ok = std::fs::canonicalize(&full)
        .map(|p| p.starts_with(root))
        .unwrap_or(false);
    let full = if ok { full } else { root.join("index.html") };
    match std::fs::read(&full) {
        Ok(bytes) => {
            let ct = match full.extension().and_then(|e| e.to_str()).unwrap_or("") {
                "html" => "text/html; charset=utf-8",
                "js" | "mjs" => "text/javascript",
                "css" => "text/css",
                "svg" => "image/svg+xml",
                "png" => "image/png",
                "ttf" => "font/ttf",
                "woff2" => "font/woff2",
                "json" => "application/json",
                _ => "application/octet-stream",
            };
            Response::from_data(bytes).with_header(Header::from_bytes("Content-Type", ct).unwrap())
        }
        Err(_) => Response::from_string("not found").with_status_code(404),
    }
}
