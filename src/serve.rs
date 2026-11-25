//! A small development server (stands in for `@11ty/eleventy-dev-server`, `Serve.js`).
//!
//! Serves the output directory, resolves `/dir/` to `/dir/index.html`, applies
//! the path prefix, and live-reloads pages through a Server-Sent Events endpoint.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;

use anyhow::{Result, anyhow};

const RELOAD_PATH: &str = "/.12ty/reload";

const RELOAD_SCRIPT: &str = r#"<script type="module">
(() => {
  const source = new EventSource("/.12ty/reload");
  source.onmessage = (e) => { if (e.data === "reload") location.reload(); };
})();
</script>"#;

pub struct DevServer {
    pub port: u16,
    output_dir: PathBuf,
    path_prefix: String,
    build_version: Arc<AtomicUsize>,
}

fn mime_type(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase()
        .as_str()
    {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "xml" => "application/xml; charset=utf-8",
        "txt" => "text/plain; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "pdf" => "application/pdf",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "wasm" => "application/wasm",
        _ => "application/octet-stream",
    }
}

impl DevServer {
    pub fn new(port: u16, output_dir: &str, path_prefix: &str) -> Self {
        DevServer {
            port,
            output_dir: PathBuf::from(output_dir),
            path_prefix: path_prefix.to_string(),
            build_version: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Notify connected browsers to reload
    pub fn reload(&self) {
        self.build_version.fetch_add(1, Ordering::SeqCst);
    }

    pub fn reload_handle(&self) -> Arc<AtomicUsize> {
        self.build_version.clone()
    }

    fn resolve(&self, url_path: &str) -> Option<PathBuf> {
        let decoded = percent_encoding::percent_decode_str(url_path)
            .decode_utf8_lossy()
            .to_string();
        let mut path = decoded.split(['?', '#']).next().unwrap_or("/").to_string();
        let prefix = self.path_prefix.trim_end_matches('/');
        if !prefix.is_empty() && prefix != "/" {
            path = path.strip_prefix(prefix)?.to_string();
            if path.is_empty() {
                path = "/".into();
            }
        }
        if path.split('/').any(|seg| seg == "..") {
            return None;
        }
        let rel = path.trim_start_matches('/');
        let mut file = self.output_dir.join(rel);
        if file.is_dir() || path.ends_with('/') {
            file = file.join("index.html");
        } else if !file.exists() {
            let with_html = self.output_dir.join(format!("{rel}.html"));
            if with_html.exists() {
                file = with_html;
            }
        }
        if file.is_file() { Some(file) } else { None }
    }

    /// Start serving in a background thread
    pub fn start(&self) -> Result<()> {
        let server = tiny_http::Server::http(("0.0.0.0", self.port))
            .map_err(|e| anyhow!("Could not start the dev server on port {}: {e}", self.port))?;
        let server = Arc::new(server);
        let output_dir = self.output_dir.clone();
        let path_prefix = self.path_prefix.clone();
        let version = self.build_version.clone();
        thread::spawn(move || {
            let me = DevServer {
                port: 0,
                output_dir,
                path_prefix,
                build_version: version.clone(),
            };
            for mut request in server.incoming_requests() {
                let url = request.url().to_string();
                if url.starts_with(RELOAD_PATH) {
                    let version = version.clone();
                    thread::spawn(move || {
                        let start = version.load(Ordering::SeqCst);
                        let reader = SseReader {
                            version,
                            start,
                            sent_initial: false,
                            pending: Vec::new(),
                        };
                        let response = tiny_http::Response::new(
                            tiny_http::StatusCode(200),
                            vec![
                                tiny_http::Header::from_bytes("Content-Type", "text/event-stream").unwrap(),
                                tiny_http::Header::from_bytes("Cache-Control", "no-cache").unwrap(),
                            ],
                            reader,
                            None,
                            None,
                        );
                        let _ = request.respond(response);
                    });
                    continue;
                }
                let _ = request.as_reader().read_to_end(&mut vec![]);
                match me.resolve(&url) {
                    Some(file) => {
                        let mut body = std::fs::read(&file).unwrap_or_default();
                        let mime = mime_type(&file);
                        if mime.starts_with("text/html") {
                            let html = String::from_utf8_lossy(&body).to_string();
                            let injected = match html.rfind("</body>") {
                                Some(i) => format!("{}{RELOAD_SCRIPT}{}", &html[..i], &html[i..]),
                                None => format!("{html}{RELOAD_SCRIPT}"),
                            };
                            body = injected.into_bytes();
                        }
                        let response = tiny_http::Response::from_data(body).with_header(
                            tiny_http::Header::from_bytes("Content-Type", mime).unwrap(),
                        );
                        let _ = request.respond(response);
                    }
                    None => {
                        let not_found = me.output_dir.join("404.html");
                        let body = std::fs::read(&not_found)
                            .unwrap_or_else(|_| b"<h1>404 Not Found</h1>".to_vec());
                        let response = tiny_http::Response::from_data(body)
                            .with_status_code(404)
                            .with_header(
                                tiny_http::Header::from_bytes("Content-Type", "text/html; charset=utf-8")
                                    .unwrap(),
                            );
                        let _ = request.respond(response);
                    }
                }
            }
        });
        Ok(())
    }
}

/// An endless reader that emits SSE `reload` events when the build version changes
struct SseReader {
    version: Arc<AtomicUsize>,
    start: usize,
    sent_initial: bool,
    pending: Vec<u8>,
}

impl Read for SseReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        loop {
            if !self.pending.is_empty() {
                let n = buf.len().min(self.pending.len());
                buf[..n].copy_from_slice(&self.pending[..n]);
                self.pending.drain(..n);
                return Ok(n);
            }
            if !self.sent_initial {
                self.sent_initial = true;
                self.pending = b": connected\n\n".to_vec();
                continue;
            }
            let current = self.version.load(Ordering::SeqCst);
            if current != self.start {
                self.start = current;
                self.pending = b"data: reload\n\n".to_vec();
                continue;
            }
            thread::sleep(Duration::from_millis(150));
        }
    }
}
