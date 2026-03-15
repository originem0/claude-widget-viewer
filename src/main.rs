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
                    // Fix #1: send fallback must be visible
                    eprintln!("No daemon found, starting viewer directly...");
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
