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
    /// Open a widget file and watch for changes
    Show {
        /// Path to the widget HTML file
        file: PathBuf,
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
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Show { file } => {
            let html = match std::fs::read_to_string(&file) {
                Ok(h) => h,
                Err(e) => {
                    eprintln!("Failed to read {}: {}", file.display(), e);
                    std::process::exit(1);
                }
            };
            viewer::run(viewer::Mode::Show {
                html,
                watch_file: file,
            });
        }
        Commands::Listen => {
            let pipe_id = ipc::generate_pipe_id();
            ipc::write_pipe_id_file(&pipe_id);
            viewer::run(viewer::Mode::Daemon { pipe_id });
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
                    // No daemon — spawn a detached `show` process and exit immediately.
                    // Uses Windows creation flags to avoid any console window flash.
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
    }
}

/// Spawn `claude-widget-viewer show <file>` as a fully detached GUI process.
/// No console window flash, no stdio inheritance. Exits immediately after spawn.
#[cfg(windows)]
fn spawn_detached_show(file: &std::path::Path) {
    use std::os::windows::process::CommandExt;

    // CREATE_NEW_PROCESS_GROUP: detach from parent's console group
    // DETACHED_PROCESS: no console window inheritance (GUI windows still work)
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    const DETACHED_PROCESS: u32 = 0x0000_0008;

    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("claude-widget-viewer"));

    match std::process::Command::new(&exe)
        .args(["show", &file.to_string_lossy()])
        .creation_flags(CREATE_NEW_PROCESS_GROUP | DETACHED_PROCESS)
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
