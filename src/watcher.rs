/// File system watcher with debounce.
/// Uses the `notify` crate to watch for changes and trigger widget reload.

use notify::{Watcher, RecursiveMode, Event, EventKind};
use std::path::Path;
use std::sync::mpsc;
use std::time::{Duration, Instant};

const DEBOUNCE_MS: u64 = 200;

/// Watch a file for changes. Calls `on_change` with the new file contents
/// whenever the file is modified. Blocks the calling thread.
pub fn watch_file<F>(path: &Path, on_change: F)
where
    F: Fn(String) + Send + 'static,
{
    let (tx, rx) = mpsc::channel();

    let mut watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
        if let Ok(event) = res {
            match event.kind {
                EventKind::Modify(_) | EventKind::Create(_) => {
                    let _ = tx.send(());
                }
                _ => {}
            }
        }
    })
    .expect("Failed to create file watcher");

    // Watch the parent directory (more reliable on Windows for atomic writes)
    let watch_dir = path.parent().unwrap_or(path);
    watcher
        .watch(watch_dir, RecursiveMode::NonRecursive)
        .expect("Failed to watch directory");

    let target_path = path.to_path_buf();
    let mut last_fire = Instant::now() - Duration::from_secs(10);

    loop {
        // Block until we get a change notification
        if rx.recv().is_err() {
            break;
        }

        // Drain any queued events (Windows fires multiples for one write)
        while rx.try_recv().is_ok() {}

        // Debounce: skip if we fired too recently
        let now = Instant::now();
        if now.duration_since(last_fire) < Duration::from_millis(DEBOUNCE_MS) {
            continue;
        }

        // Small delay to let the write finish
        std::thread::sleep(Duration::from_millis(50));

        // Read the updated file
        match std::fs::read_to_string(&target_path) {
            Ok(content) if !content.is_empty() => {
                last_fire = Instant::now();
                on_change(content);
            }
            _ => {}
        }
    }
}
