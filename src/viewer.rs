/// Window creation and WebView management.
/// Fix #12: unified Mode enum replaces scattered bool params.

use crate::watcher;
use crate::ipc;
use std::path::PathBuf;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::window::{Window, WindowId};
use wry::WebView;

/// Viewer startup modes
pub enum Mode {
    /// Visible window, watch file for changes
    Show { html: String, watch_file: PathBuf },
    /// Hidden window, prewarmed, pipe server
    Daemon { pipe_id: String },
}

/// Custom events sent to the winit event loop
#[derive(Debug)]
pub enum ViewerEvent {
    LoadWidget(String),
    SetTitle(String),
    ShowWindow,
    Close,
}

struct ViewerApp {
    window: Option<Window>,
    webview: Option<WebView>,
    mode: AppMode,
    proxy: EventLoopProxy<ViewerEvent>,
}

/// Runtime state derived from Mode
enum AppMode {
    Show {
        initial_html: String,
        watch_file: PathBuf,
    },
    Daemon,
}

impl ViewerApp {
    fn load_widget(&self, html: &str) {
        if let Some(ref webview) = self.webview {
            let js = crate::shell::make_inject_js(html);
            if let Err(e) = webview.evaluate_script(&js) {
                eprintln!("Failed to evaluate script: {:?}", e);
            }
        }
    }
}

impl ApplicationHandler<ViewerEvent> for ViewerApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let visible = matches!(self.mode, AppMode::Show { .. });

        let attrs = Window::default_attributes()
            .with_title("Claude Widget")
            .with_inner_size(winit::dpi::LogicalSize::new(740.0, 600.0))
            .with_visible(visible);

        let window = match event_loop.create_window(attrs) {
            Ok(w) => w,
            Err(e) => {
                eprintln!("Failed to create window: {}", e);
                event_loop.exit();
                return;
            }
        };

        // Build initialization script for Show mode
        let init_script = match &self.mode {
            AppMode::Show { initial_html, .. } => crate::shell::make_inject_js(initial_html),
            AppMode::Daemon => String::new(),
        };

        // Fix #8: no protocol state needed — shell is served directly, widget via base64
        let shell_html = crate::shell::generate_shell();
        let shell_bytes = shell_html.into_bytes();

        let mut builder = wry::WebViewBuilder::new()
            .with_custom_protocol("wry".to_string(), move |_id, request| {
                let uri = request.uri().to_string();
                // Only need to serve shell.html — widget injection is via evaluate_script
                let (body, mime) = if uri.contains("shell.html") || uri.ends_with("localhost") || uri.ends_with("localhost/") {
                    (shell_bytes.clone(), "text/html")
                } else {
                    (format!("404 Not Found: {}", uri).into_bytes(), "text/plain")
                };
                wry::http::Response::builder()
                    .header("Content-Type", mime)
                    .body(std::borrow::Cow::Owned(body))
                    .unwrap()
            })
            .with_ipc_handler(|msg| {
                eprintln!("[IPC from JS] {}", msg.body());
            });

        if !init_script.is_empty() {
            builder = builder.with_initialization_script(&init_script);
        }

        let webview = match builder
            .with_url("wry://localhost/shell.html")
            .build(&window)
        {
            Ok(wv) => wv,
            Err(e) => {
                eprintln!("Failed to create WebView: {}", e);
                event_loop.exit();
                return;
            }
        };

        self.window = Some(window);
        self.webview = Some(webview);

        // Start file watcher for Show mode
        if let AppMode::Show { watch_file, .. } = &self.mode {
            let proxy = self.proxy.clone();
            let path = watch_file.clone();
            std::thread::spawn(move || {
                watcher::watch_file(&path, move |new_html| {
                    let _ = proxy.send_event(ViewerEvent::LoadWidget(new_html));
                });
            });
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: ViewerEvent) {
        match event {
            ViewerEvent::LoadWidget(html) => {
                self.load_widget(&html);
            }
            ViewerEvent::SetTitle(title) => {
                if let Some(ref window) = self.window {
                    window.set_title(&title);
                }
            }
            ViewerEvent::ShowWindow => {
                if let Some(ref window) = self.window {
                    window.set_visible(true);
                    window.focus_window();
                }
            }
            ViewerEvent::Close => {
                ipc::cleanup_pipe_id_file();
                event_loop.exit();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        if let WindowEvent::CloseRequested = event {
            ipc::cleanup_pipe_id_file();
            event_loop.exit();
        }
    }
}

/// Single entry point for all modes.
pub fn run(mode: Mode) {
    let event_loop = EventLoop::<ViewerEvent>::with_user_event()
        .build()
        .unwrap_or_else(|e| {
            eprintln!("Failed to create event loop: {}", e);
            std::process::exit(1);
        });
    let proxy = event_loop.create_proxy();

    let app_mode = match &mode {
        Mode::Show { html, watch_file } => AppMode::Show {
            initial_html: html.clone(),
            watch_file: watch_file.clone(),
        },
        Mode::Daemon { .. } => AppMode::Daemon,
    };

    // Start pipe server for daemon mode
    if let Mode::Daemon { pipe_id } = &mode {
        let pipe_proxy = proxy.clone();
        let pipe_name = ipc::pipe_name_from_id(pipe_id);
        std::thread::spawn(move || {
            ipc::run_pipe_server(&pipe_name, pipe_proxy);
        });
    }

    let mut app = ViewerApp {
        window: None,
        webview: None,
        mode: app_mode,
        proxy,
    };

    if let Err(e) = event_loop.run_app(&mut app) {
        eprintln!("Event loop error: {}", e);
    }
}
