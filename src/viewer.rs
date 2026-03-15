/// Window creation and WebView management.
/// All modes start a pipe server and scan widgets on startup.

use crate::watcher;
use crate::ipc;
use crate::shell;
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
    /// Hidden window, prewarmed, pipe server only
    Daemon,
}

/// Custom events sent to the winit event loop
#[derive(Debug)]
pub enum ViewerEvent {
    /// New widget — add to sidebar, display it. `file` enables dedup.
    LoadWidget { html: String, title: String, file: Option<String> },
    /// Hot-reload — morphdom patch the active widget
    UpdateWidget(String),
    ShowWindow,
    Close,
}

struct ViewerApp {
    window: Option<Window>,
    webview: Option<WebView>,
    mode: AppMode,
    proxy: EventLoopProxy<ViewerEvent>,
    /// Whether the initial directory scan has been done
    scanned: bool,
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
    fn inject_widget(&self, html: &str, title: &str, file: Option<&str>) {
        if let Some(ref webview) = self.webview {
            let js = shell::make_inject_js(html, title, file);
            if let Err(e) = webview.evaluate_script(&js) {
                eprintln!("Failed to evaluate script: {:?}", e);
            }
        }
    }

    fn update_widget(&self, html: &str) {
        if let Some(ref webview) = self.webview {
            let js = shell::make_update_js(html);
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
            .with_inner_size(winit::dpi::LogicalSize::new(860.0, 620.0))
            .with_visible(visible);

        let window = match event_loop.create_window(attrs) {
            Ok(w) => w,
            Err(e) => {
                eprintln!("Failed to create window: {}", e);
                event_loop.exit();
                return;
            }
        };

        // --- Build initialization script ---
        // Both modes scan the widgets directory on startup.
        // The filesystem is the source of truth — not IPC messages.
        // Build init script as a batch data array.
        // The page's IIFE reads window.__WIDGET_BATCH and calls injectWidget for each.
        let init_script = {
            let mut batch: Vec<(String, String, String)> = Vec::new();

            let (widgets_dir, active_file) = match &self.mode {
                AppMode::Show { watch_file, .. } => (
                    watch_file.parent().map(|p| p.to_path_buf()),
                    Some(watch_file.clone()),
                ),
                AppMode::Daemon => (shell::find_widgets_dir(), None),
            };

            if let Some(dir) = widgets_dir {
                let canonical_active = active_file
                    .as_ref()
                    .and_then(|f| std::fs::canonicalize(f).ok());

                let widgets = shell::scan_widgets_dir(&dir);
                for (path, w_title) in &widgets {
                    let canonical_path = std::fs::canonicalize(path).ok();
                    if canonical_active.is_some() && canonical_path == canonical_active {
                        continue;
                    }
                    if let Ok(html) = std::fs::read_to_string(path) {
                        if !html.is_empty() {
                            batch.push((html, w_title.clone(), path.to_string_lossy().to_string()));
                        }
                    }
                }
            }

            // Show mode: active file goes last
            if let AppMode::Show { initial_html, watch_file } = &self.mode {
                let title = watch_file
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "Widget".to_string());
                batch.push((initial_html.clone(), title, watch_file.to_string_lossy().to_string()));
            }

            shell::make_batch_init_js(&batch)
        };

        let shell_html = shell::generate_shell();
        let shell_bytes = shell_html.as_bytes().to_vec();

        let mut builder = wry::WebViewBuilder::new()
            .with_custom_protocol("wry".to_string(), move |_id, request| {
                let uri = request.uri().to_string();
                let (body, mime) = if uri.contains("shell.html")
                    || uri.ends_with("localhost")
                    || uri.ends_with("localhost/")
                {
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

        // File watcher for Show mode (hot-reload via morphdom)
        if let AppMode::Show { watch_file, .. } = &self.mode {
            let proxy = self.proxy.clone();
            let path = watch_file.clone();
            std::thread::spawn(move || {
                watcher::watch_file(&path, move |new_html| {
                    let _ = proxy.send_event(ViewerEvent::UpdateWidget(new_html));
                });
            });
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: ViewerEvent) {
        match event {
            ViewerEvent::LoadWidget { html, title, file } => {
                // Fallback: if the init script didn't scan (no widgets dir found),
                // scan the incoming file's parent directory on first load.
                if !self.scanned {
                    self.scanned = true;
                    if let Some(ref file_path) = file {
                        let path = std::path::Path::new(file_path);
                        if let Some(dir) = path.parent() {
                            let canonical_file = std::fs::canonicalize(path).ok();
                            let existing = shell::scan_widgets_dir(dir);
                            for (p, t) in &existing {
                                let canonical_p = std::fs::canonicalize(&p).ok();
                                if canonical_file.is_some() && canonical_p == canonical_file {
                                    continue;
                                }
                                if let Ok(h) = std::fs::read_to_string(&p) {
                                    if !h.is_empty() {
                                        self.inject_widget(&h, &t, Some(&p.to_string_lossy()));
                                    }
                                }
                            }
                        }
                    }
                }

                self.inject_widget(&html, &title, file.as_deref());
                if let Some(ref window) = self.window {
                    window.set_title(&title);
                }
            }
            ViewerEvent::UpdateWidget(html) => {
                self.update_widget(&html);
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

/// Single entry point for all modes. Every mode gets a pipe server.
pub fn run(mode: Mode) {
    let pipe_id = ipc::generate_pipe_id();
    ipc::write_pipe_id_file(&pipe_id);

    let event_loop = EventLoop::<ViewerEvent>::with_user_event()
        .build()
        .unwrap_or_else(|e| {
            eprintln!("Failed to create event loop: {}", e);
            std::process::exit(1);
        });
    let proxy = event_loop.create_proxy();

    let pipe_proxy = proxy.clone();
    let pipe_name = ipc::pipe_name_from_id(&pipe_id);
    std::thread::spawn(move || {
        ipc::run_pipe_server(&pipe_name, pipe_proxy);
    });

    // The init script scans the widgets directory.
    // Daemon might not find one (no CWD context) — fallback scan on first IPC.
    let scanned = match &mode {
        Mode::Show { .. } => true,
        Mode::Daemon => shell::find_widgets_dir().is_some(),
    };

    let app_mode = match &mode {
        Mode::Show { html, watch_file } => AppMode::Show {
            initial_html: html.clone(),
            watch_file: watch_file.clone(),
        },
        Mode::Daemon => AppMode::Daemon,
    };

    let mut app = ViewerApp {
        window: None,
        webview: None,
        mode: app_mode,
        proxy,
        scanned,
    };

    if let Err(e) = event_loop.run_app(&mut app) {
        eprintln!("Event loop error: {}", e);
    }
}
