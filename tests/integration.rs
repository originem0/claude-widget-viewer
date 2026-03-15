/// Integration tests for claude-widget-viewer.
/// These spawn the binary as a subprocess to test real-world scenarios.
///
/// All tests that touch pipe state use `WIDGET_VIEWER_STATE_DIR` pointed at
/// a per-test tempdir. This ensures:
/// - No writes to the user's real ~/.claude/
/// - No cross-test interference
/// - Cleanup only kills processes spawned by this test

use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_claude-widget-viewer")
}

/// Drop guard: ensures viewer processes are killed and pipe files cleaned up after tests.
#[cfg(windows)]
struct ViewerGuard {
    pids: Vec<u32>,
    pipe_files: Vec<std::path::PathBuf>,
}

#[cfg(windows)]
impl ViewerGuard {
    fn new() -> Self {
        Self {
            pids: Vec::new(),
            pipe_files: Vec::new(),
        }
    }

    fn track_pid(&mut self, pid: u32) {
        self.pids.push(pid);
    }

    fn track_pipe_file(&mut self, path: std::path::PathBuf) {
        self.pipe_files.push(path);
    }
}

#[cfg(windows)]
impl Drop for ViewerGuard {
    fn drop(&mut self) {
        for pid in &self.pids {
            let _ = Command::new("taskkill")
                .args(["/F", "/PID", &pid.to_string()])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
        for f in &self.pipe_files {
            let _ = std::fs::remove_file(f);
        }
    }
}

/// Find pipe ID files inside a specific directory (NOT the user's home).
#[cfg(windows)]
fn find_pipe_files_in_dir(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut result = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.starts_with(".widget-viewer-pipe-") {
                result.push(entry.path());
            }
        }
    }
    result
}

/// Read all pipe files in a dir, register PIDs and paths in the guard.
#[cfg(windows)]
fn track_all_pipe_files(dir: &std::path::Path, guard: &mut ViewerGuard) {
    for pf in find_pipe_files_in_dir(dir) {
        if let Ok(content) = std::fs::read_to_string(&pf) {
            if let Ok(pid) = content.trim().parse::<u32>() {
                guard.track_pid(pid);
            }
        }
        guard.track_pipe_file(pf);
    }
}

// === Scenario 1: Handle inheritance ===
// The hook subprocess must NOT inherit pipe handles from the parent.
// If it did, the parent's stdout would stay open as long as the viewer lives,
// blocking Claude Code forever.

#[test]
#[cfg(windows)]
fn test_hook_does_not_inherit_handles() {
    let tmp = tempfile::tempdir().unwrap();
    let state_dir = tempfile::tempdir().unwrap();

    // Create a widget file inside .claude/widgets/ so the hook recognizes it
    let widgets_dir = tmp.path().join(".claude").join("widgets");
    std::fs::create_dir_all(&widgets_dir).unwrap();
    let widget_in_dir = widgets_dir.join("test.html");
    std::fs::write(&widget_in_dir, "<p>test</p>").unwrap();

    let hook_json = serde_json::json!({
        "tool_input": {
            "file_path": widget_in_dir.to_string_lossy().to_string()
        }
    });

    let mut child = Command::new(bin())
        .arg("hook")
        .env("WIDGET_VIEWER_STATE_DIR", state_dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to spawn hook process");

    let mut stdin = child.stdin.take().unwrap();
    writeln!(stdin, "{}", hook_json).unwrap();
    drop(stdin); // Close stdin so hook can process

    // The hook must exit within 2 seconds. If viewer inherited handles,
    // wait_with_output would block because stdout pipe stays open.
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = child.wait_with_output();
        let _ = tx.send(result);
    });

    let result = rx.recv_timeout(Duration::from_secs(2));
    assert!(
        result.is_ok(),
        "Hook process did not exit within 2 seconds — viewer likely inherited pipe handles"
    );

    // Clean up only processes found in our isolated state dir
    std::thread::sleep(Duration::from_millis(500));
    let mut guard = ViewerGuard::new();
    track_all_pipe_files(state_dir.path(), &mut guard);
}

// === Scenario 2: Hook stdin doesn't block ===
// Claude Code pipes JSON followed by newline but may NOT close stdin.
// The hook must use read_line (not from_reader) and return immediately.

#[test]
fn test_hook_returns_without_eof() {
    // Use a non-widget path so the hook exits early without spawning a viewer
    let hook_json = serde_json::json!({
        "tool_input": {
            "file_path": "src/main.rs"
        }
    });

    let mut child = Command::new(bin())
        .arg("hook")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to spawn hook process");

    // Write JSON + newline but do NOT close stdin (simulates Claude Code behavior).
    // stdin_hold stays alive in the main thread to keep the pipe open.
    let mut stdin_hold = child.stdin.take().unwrap();
    writeln!(stdin_hold, "{}", hook_json).unwrap();

    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let status = child.wait();
        let _ = tx.send(status);
    });

    let result = rx.recv_timeout(Duration::from_secs(1));
    assert!(
        result.is_ok(),
        "Hook did not exit within 1 second — likely blocking on stdin EOF (from_reader bug)"
    );

    // stdin_hold dropped here after assert — correct lifetime
    drop(stdin_hold);
}

// === Scenario 4: Multiple sends reuse viewer ===
// Sending multiple files should reuse the same viewer process, not spawn new ones.

#[test]
#[ignore] // Requires display server (GUI)
#[cfg(windows)]
fn test_multiple_sends_reuse_viewer() {
    let tmp = tempfile::tempdir().unwrap();
    let state_dir = tempfile::tempdir().unwrap();

    let file1 = tmp.path().join("widget1.html");
    let file2 = tmp.path().join("widget2.html");
    std::fs::write(&file1, "<p>widget 1</p>").unwrap();
    std::fs::write(&file2, "<p>widget 2</p>").unwrap();

    let mut guard = ViewerGuard::new();

    // First send — should spawn a viewer
    let _status = Command::new(bin())
        .args(["send", &file1.to_string_lossy()])
        .env("WIDGET_VIEWER_STATE_DIR", state_dir.path())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("Failed to run send");

    // Give viewer time to start and write pipe file
    std::thread::sleep(Duration::from_millis(500));

    let initial_pipes = find_pipe_files_in_dir(state_dir.path());
    track_all_pipe_files(state_dir.path(), &mut guard);
    let initial_count = initial_pipes.len();

    // Second send — should reuse existing viewer
    let _status = Command::new(bin())
        .args(["send", &file2.to_string_lossy()])
        .env("WIDGET_VIEWER_STATE_DIR", state_dir.path())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("Failed to run send");

    std::thread::sleep(Duration::from_millis(500));

    // Track any new pipe files for cleanup
    for pf in find_pipe_files_in_dir(state_dir.path()) {
        if !initial_pipes.contains(&pf) {
            if let Ok(content) = std::fs::read_to_string(&pf) {
                if let Ok(pid) = content.trim().parse::<u32>() {
                    guard.track_pid(pid);
                }
            }
            guard.track_pipe_file(pf);
        }
    }

    let final_count = find_pipe_files_in_dir(state_dir.path()).len();
    assert_eq!(
        final_count, initial_count,
        "Second send should reuse the existing viewer (pipe count should not increase)"
    );
}

// === Scenario 8: Idle timeout exits daemon ===
// With short timeout env vars, the listen daemon should exit on its own.

#[test]
#[ignore] // Requires display server (GUI)
#[cfg(windows)]
fn test_idle_timeout_exits_daemon() {
    let state_dir = tempfile::tempdir().unwrap();

    let child = Command::new(bin())
        .arg("listen")
        .env("WIDGET_VIEWER_IDLE_TIMEOUT_SECS", "2")
        .env("WIDGET_VIEWER_IDLE_POLL_SECS", "1")
        .env("WIDGET_VIEWER_STATE_DIR", state_dir.path())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to spawn listen process");

    let pid = child.id();
    let mut guard = ViewerGuard::new();
    guard.track_pid(pid);

    // Give the daemon time to start and write its pipe file
    std::thread::sleep(Duration::from_millis(500));
    track_all_pipe_files(state_dir.path(), &mut guard);

    // Wait up to 5 seconds for the daemon to exit on idle timeout
    let (tx, rx) = mpsc::channel();
    let mut child_for_wait = child;
    std::thread::spawn(move || {
        let status = child_for_wait.wait();
        let _ = tx.send(status);
    });

    let result = rx.recv_timeout(Duration::from_secs(5));

    let exited = result.is_ok();
    if exited {
        // Process exited cleanly — clear PIDs so guard doesn't
        // risk killing a recycled PID on drop
        guard.pids.clear();
    }
    assert!(exited, "Daemon did not exit within 5 seconds — idle timeout not working");

    // Pipe ID file should have been cleaned up
    std::thread::sleep(Duration::from_millis(200));
    let remaining: Vec<_> = find_pipe_files_in_dir(state_dir.path())
        .into_iter()
        .filter(|p| {
            p.file_name()
                .map(|n| n.to_string_lossy().contains(&pid.to_string()))
                .unwrap_or(false)
        })
        .collect();
    assert!(
        remaining.is_empty(),
        "Pipe ID file should be cleaned up after idle timeout, but found: {:?}",
        remaining
    );
}
