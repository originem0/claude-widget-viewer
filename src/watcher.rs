/// File system watcher with debounce.
/// Fix #7: filters events to only the target file, ignores siblings.

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

    // Fix #7: capture the canonical target path to compare against event paths
    let target_path = match std::fs::canonicalize(path) {
        Ok(p) => p,
        Err(_) => path.to_path_buf(),
    };
    let target_for_filter = target_path.clone();

    let mut watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
        if let Ok(event) = res {
            match event.kind {
                EventKind::Modify(_) | EventKind::Create(_) => {
                    // Fix #7: only trigger for the specific file we're watching
                    let is_target = event.paths.iter().any(|p| {
                        match std::fs::canonicalize(p) {
                            Ok(canon) => canon == target_for_filter,
                            Err(_) => p.ends_with(target_for_filter.file_name().unwrap_or_default()),
                        }
                    });
                    if is_target {
                        let _ = tx.send(());
                    }
                }
                _ => {}
            }
        }
    })
    .unwrap_or_else(|e| {
        eprintln!("Failed to create file watcher: {}", e);
        std::process::exit(1);
    });

    // Watch parent directory (more reliable on Windows for atomic writes)
    let watch_dir = path.parent().unwrap_or(path);
    if let Err(e) = watcher.watch(watch_dir, RecursiveMode::NonRecursive) {
        eprintln!("Failed to watch {}: {}", watch_dir.display(), e);
        return;
    }

    let mut last_fire = Instant::now() - Duration::from_secs(10);

    loop {
        if rx.recv().is_err() {
            break;
        }

        // Drain queued events
        while rx.try_recv().is_ok() {}

        // Debounce
        let now = Instant::now();
        if now.duration_since(last_fire) < Duration::from_millis(DEBOUNCE_MS) {
            continue;
        }

        // Small delay for write completion
        std::thread::sleep(Duration::from_millis(50));

        match std::fs::read_to_string(&target_path) {
            Ok(content) if !content.is_empty() => {
                last_fire = Instant::now();
                on_change(content);
            }
            _ => {}
        }
    }
}
