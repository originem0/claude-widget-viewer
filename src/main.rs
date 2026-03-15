mod viewer;
mod protocol;
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
            let html = std::fs::read_to_string(&file).unwrap_or_else(|e| {
                eprintln!("Failed to read {}: {}", file.display(), e);
                std::process::exit(1);
            });
            viewer::run_viewer(Some(html), Some(file), true);
        }
        Commands::Listen => {
            let pipe_id = ipc::generate_pipe_id();
            ipc::write_pipe_id_file(&pipe_id);

            viewer::run_viewer_daemon(&pipe_id);
        }
        Commands::Send { file } => {
            let file = std::fs::canonicalize(&file).unwrap_or_else(|e| {
                eprintln!("Failed to resolve {}: {}", file.display(), e);
                std::process::exit(1);
            });

            match ipc::try_send_load_widget(&file) {
                Ok(()) => {}
                Err(_) => {
                    eprintln!("No daemon found, starting viewer directly...");
                    let html = std::fs::read_to_string(&file).unwrap_or_else(|e| {
                        eprintln!("Failed to read {}: {}", file.display(), e);
                        std::process::exit(1);
                    });
                    viewer::run_viewer(Some(html), Some(file), false);
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
