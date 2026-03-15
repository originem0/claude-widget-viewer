mod viewer;
mod shell;
mod watcher;
mod ipc;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "claude-widget-viewer", about = "Lightweight widget renderer for Claude Code")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Open the widget viewer (auto-discovers all widgets from the directory)
    Show {
        /// Path to a widget HTML file (optional; auto-discovers if omitted)
        file: Option<PathBuf>,
    },
    /// Start daemon mode: hidden window, prewarmed WebView2, listening on Named Pipe
    Listen,
    /// Send a widget file to the running daemon (falls back to show if daemon unavailable)
    Send {
        /// Path to the widget HTML file
        file: PathBuf,
    },
    /// Stop the running daemon
    Stop,
    /// Process Claude Code hook input from stdin (replaces jq dependency)
    Hook,
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Show { file } => {
            // If file given: open it (its directory gets scanned for siblings).
            // If omitted: auto-discover .claude/widgets/ from CWD or HOME,
            //             open the newest widget.
            let (html, watch_file) = match file {
                Some(f) => {
                    let html = match std::fs::read_to_string(&f) {
                        Ok(h) => h,
                        Err(e) => {
                            eprintln!("Failed to read {}: {}", f.display(), e);
                            std::process::exit(1);
                        }
                    };
                    (html, f)
                }
                None => {
                    let dir = match shell::find_widgets_dir() {
                        Some(d) => d,
                        None => {
                            eprintln!("No widgets directory found (.claude/widgets/ in CWD or HOME).");
                            std::process::exit(1);
                        }
                    };
                    let widgets = shell::scan_widgets_dir(&dir);
                    match widgets.last() {
                        Some((path, _)) => {
                            let html = match std::fs::read_to_string(path) {
                                Ok(h) => h,
                                Err(e) => {
                                    eprintln!("Failed to read {}: {}", path.display(), e);
                                    std::process::exit(1);
                                }
                            };
                            (html, path.clone())
                        }
                        None => {
                            eprintln!("No .html files in {:?}", dir);
                            std::process::exit(1);
                        }
                    }
                }
            };
            viewer::run(viewer::Mode::Show {
                html,
                watch_file,
            });
        }
        Commands::Listen => {
            // Pipe ID generation and pipe server startup are handled inside viewer::run()
            viewer::run(viewer::Mode::Daemon);
        }
        Commands::Send { file } => {
            let file = match std::fs::canonicalize(&file) {
                Ok(f) => f,
                Err(e) => {
                    eprintln!("Failed to resolve {}: {}", file.display(), e);
                    std::process::exit(1);
                }
            };

            match ipc::try_send_load_widget(&file) {
                Ok(()) => {}
                Err(_) => {
                    // No daemon — spawn a detached `show` process.
                    // That process starts its own pipe server, so subsequent sends reuse it.
                    spawn_detached_show(&file);
                }
            }
        }
        Commands::Stop => {
            match ipc::try_send_close() {
                Ok(()) => eprintln!("Daemon stopped."),
                Err(e) => eprintln!("Failed to stop daemon: {}", e),
            }
        }
        Commands::Hook => {
            // Read one line of JSON from stdin. Claude Code pipes hook JSON followed
            // by a newline but may NOT close stdin — so from_reader would block forever
            // waiting for EOF. read_line stops at '\n' and returns immediately.
            use std::io::BufRead;
            let mut line = String::new();
            if std::io::stdin().lock().read_line(&mut line).is_err() || line.is_empty() {
                std::process::exit(0);
            }

            let json: serde_json::Value = match serde_json::from_str(line.trim()) {
                Ok(v) => v,
                Err(_) => std::process::exit(0),
            };

            let file_path = match json
                .get("tool_input")
                .and_then(|ti| ti.get("file_path"))
                .and_then(|fp| fp.as_str())
            {
                Some(p) => p,
                None => std::process::exit(0),
            };

            // Match both Unix and Windows path separators
            let is_widget = shell::is_widget_path(file_path);

            if !is_widget {
                std::process::exit(0);
            }

            let file = PathBuf::from(file_path);
            let file = std::fs::canonicalize(&file).unwrap_or(file);

            match ipc::try_send_load_widget(&file) {
                Ok(()) => {}
                Err(_) => {
                    spawn_detached_show(&file);
                }
            }
        }
    }
}

/// Spawn `claude-widget-viewer show <file>` as a fully detached GUI process.
///
/// Uses CreateProcessW with bInheritHandles=FALSE so the viewer does NOT inherit
/// any pipe handles from the parent. Without this, Claude Code's stdout/stderr
/// pipes stay open as long as the viewer runs, blocking Claude Code indefinitely.
#[cfg(windows)]
fn spawn_detached_show(file: &std::path::Path) {
    use windows_sys::Win32::System::Threading::{
        CreateProcessW, STARTUPINFOW, PROCESS_INFORMATION,
    };
    use windows_sys::Win32::Foundation::CloseHandle;

    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    const DETACHED_PROCESS: u32 = 0x0000_0008;

    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("claude-widget-viewer"));
    let cmd = format!("\"{}\" show \"{}\"", exe.display(), file.display());
    let mut cmd_wide: Vec<u16> = cmd.encode_utf16().chain(std::iter::once(0)).collect();

    let mut si: STARTUPINFOW = unsafe { std::mem::zeroed() };
    si.cb = std::mem::size_of::<STARTUPINFOW>() as u32;

    let mut pi: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };

    let ok = unsafe {
        CreateProcessW(
            std::ptr::null(),          // lpApplicationName (use command line)
            cmd_wide.as_mut_ptr(),     // lpCommandLine
            std::ptr::null(),          // lpProcessAttributes
            std::ptr::null(),          // lpThreadAttributes
            0,                         // bInheritHandles = FALSE ← the fix
            CREATE_NEW_PROCESS_GROUP | DETACHED_PROCESS,
            std::ptr::null(),          // lpEnvironment (inherit)
            std::ptr::null(),          // lpCurrentDirectory (inherit)
            &si,
            &mut pi,
        )
    };

    if ok != 0 {
        unsafe {
            CloseHandle(pi.hProcess);
            CloseHandle(pi.hThread);
        }
    } else {
        eprintln!("Failed to spawn viewer");
        std::process::exit(1);
    }
}

#[cfg(not(windows))]
fn spawn_detached_show(file: &std::path::Path) {
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("claude-widget-viewer"));
    match std::process::Command::new(&exe)
        .args(["show", &file.to_string_lossy()])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        Ok(_) => {}
        Err(e) => {
            eprintln!("Failed to spawn viewer: {}", e);
            std::process::exit(1);
        }
    }
}
