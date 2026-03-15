/// Window creation and WebView management.
/// Handles both visible (show) and hidden (daemon) modes.

use crate::protocol::ProtocolState;
use crate::watcher;
use crate::ipc;
use std::path::PathBuf;
use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::window::{Window, WindowId};
use wry::WebView;

/// Custom events sent to the winit event loop
#[derive(Debug)]
pub enum ViewerEvent {
    /// Load new widget HTML content
    LoadWidget(String),
    /// Set window title
    SetTitle(String),
    /// Show the window (for daemon mode)
    ShowWindow,
    /// Close and exit
    Close,
}

struct ViewerApp {
    window: Option<Window>,
    webview: Option<WebView>,
    protocol_state: Arc<ProtocolState>,
    initial_html: Option<String>,
    watch_file: Option<PathBuf>,
    start_visible: bool,
    proxy: EventLoopProxy<ViewerEvent>,
}

impl ViewerApp {
    fn load_widget(&self, html: &str) {
        if let Some(ref webview) = self.webview {
            let js = crate::shell::make_inject_js(html);
            let _ = webview.evaluate_script(&js);
        }
    }
}

impl ApplicationHandler<ViewerEvent> for ViewerApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return; // Already created
        }

        let attrs = Window::default_attributes()
            .with_title("Claude Widget")
            .with_inner_size(winit::dpi::LogicalSize::new(740.0, 600.0))
            .with_visible(self.start_visible);

        let window = event_loop.create_window(attrs).expect("Failed to create window");

        // Build the initialization script: if we have initial HTML, inject it after page load
        let init_script = match &self.initial_html {
            Some(html) => crate::shell::make_inject_js(html),
            None => String::new(),
        };

        // Clone state for the protocol closure
        let state = self.protocol_state.clone();

        let mut builder = wry::WebViewBuilder::new()
            .with_custom_protocol("wry".to_string(), move |_id, request| {
                let uri = request.uri().to_string();
                let (body, mime) = state.handle_request(&uri);
                wry::http::Response::builder()
                    .header("Content-Type", &mime)
                    .header("Access-Control-Allow-Origin", "*")
                    .body(std::borrow::Cow::Owned(body))
                    .unwrap()
            })
            .with_ipc_handler(|msg| {
                eprintln!("[IPC from JS] {}", msg.body());
            });

        // Inject initial widget via initialization script (runs after page load)
        if !init_script.is_empty() {
            builder = builder.with_initialization_script(&init_script);
        }

        let webview = builder
            .with_url("wry://localhost/shell.html")
            .build(&window)
            .expect("Failed to create WebView");

        self.window = Some(window);
        self.webview = Some(webview);

        // Start file watcher if a file was specified
        if let Some(ref path) = self.watch_file {
            let proxy = self.proxy.clone();
            let path = path.clone();
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

/// Run viewer in standalone mode (show subcommand)
pub fn run_viewer(initial_html: Option<String>, watch_file: Option<PathBuf>, visible: bool) {
    let event_loop = EventLoop::<ViewerEvent>::with_user_event()
        .build()
        .expect("Failed to create event loop");
    let proxy = event_loop.create_proxy();

    let protocol_state = Arc::new(ProtocolState::new());

    let mut app = ViewerApp {
        window: None,
        webview: None,
        protocol_state,
        initial_html,
        watch_file,
        start_visible: visible,
        proxy,
    };

    event_loop.run_app(&mut app).expect("Event loop failed");
}

/// Run viewer in daemon mode (listen subcommand)
pub fn run_viewer_daemon(pipe_id: &str) {
    let event_loop = EventLoop::<ViewerEvent>::with_user_event()
        .build()
        .expect("Failed to create event loop");
    let proxy = event_loop.create_proxy();

    let protocol_state = Arc::new(ProtocolState::new());

    // Start the Named Pipe server in a background thread
    let pipe_proxy = proxy.clone();
    let pipe_name = ipc::pipe_name_from_id(pipe_id);
    std::thread::spawn(move || {
        ipc::run_pipe_server(&pipe_name, pipe_proxy);
    });

    let mut app = ViewerApp {
        window: None,
        webview: None,
        protocol_state,
        initial_html: None,
        watch_file: None,
        start_visible: false, // Hidden until a widget arrives
        proxy,
    };

    event_loop.run_app(&mut app).expect("Event loop failed");
}
