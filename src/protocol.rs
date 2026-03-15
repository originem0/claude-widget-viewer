/// Custom protocol handler for wry:// scheme.
/// Serves embedded assets and the current widget HTML.

use std::sync::{Arc, Mutex};

pub struct ProtocolState {
    /// Current widget HTML content
    pub widget_html: Arc<Mutex<String>>,
    /// Pre-generated shell HTML
    pub shell_html: String,
}

impl ProtocolState {
    pub fn new() -> Self {
        Self {
            widget_html: Arc::new(Mutex::new(String::new())),
            shell_html: crate::shell::generate_shell(),
        }
    }

    /// Handle a request to the wry:// custom protocol.
    /// Returns (body_bytes, mime_type).
    pub fn handle_request(&self, uri: &str) -> (Vec<u8>, String) {
        // Parse path from URI like "wry://localhost/shell.html"
        let path = uri
            .strip_prefix("wry://localhost")
            .or_else(|| uri.strip_prefix("wry://localhost/"))
            .unwrap_or(uri);
        let path = if path.is_empty() { "/" } else { path };

        match path {
            "/" | "/shell.html" => {
                (self.shell_html.as_bytes().to_vec(), "text/html".to_string())
            }
            "/widget/current" => {
                let html = self.widget_html.lock().unwrap().clone();
                (html.into_bytes(), "text/html".to_string())
            }
            _ => {
                let body = format!("404 Not Found: {}", path);
                (body.into_bytes(), "text/plain".to_string())
            }
        }
    }
}
