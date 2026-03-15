/// Named Pipe IPC for daemon communication.
///
/// Fixes from v2 plan:
/// - Timer thread self-connects to unblock ConnectNamedPipe on idle timeout
/// - Unified ack/disconnect/close path eliminates double-free risk
/// - send_message iterates all pipe ID files, cleaning stale ones

use crate::viewer::ViewerEvent;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use winit::event_loop::EventLoopProxy;

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type")]
pub enum IpcMessage {
    LoadWidget { file: String, title: Option<String> },
    UpdateWidget { html: String },
    Show,
    Close,
}

pub fn generate_pipe_id() -> String {
    format!("{}", std::process::id())
}

pub fn pipe_name_from_id(id: &str) -> String {
    format!(r"\\.\pipe\claude-widget-viewer-{}", id)
}

fn pipe_id_file_path() -> PathBuf {
    let pid = std::process::id();
    pipe_id_dir().join(format!(".widget-viewer-pipe-{}", pid))
}

pub(crate) fn pipe_id_dir() -> PathBuf {
    // Override for test isolation — all pipe state goes to a temp dir
    if let Ok(dir) = std::env::var("WIDGET_VIEWER_STATE_DIR") {
        return PathBuf::from(dir);
    }
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| ".".to_string());
    Path::new(&home).join(".claude")
}

pub fn write_pipe_id_file(id: &str) {
    let path = pipe_id_file_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&path, id);
}

/// Returns all (pipe_id, file_path) pairs found in ~/.claude/
fn find_all_pipe_ids() -> Vec<(String, PathBuf)> {
    let dir = pipe_id_dir();
    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };
    let mut result = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str.starts_with(".widget-viewer-pipe-") {
            if let Ok(id) = std::fs::read_to_string(entry.path()) {
                let id = id.trim().to_string();
                if !id.is_empty() {
                    result.push((id, entry.path()));
                }
            }
        }
    }
    result
}

pub fn cleanup_pipe_id_file() {
    let _ = std::fs::remove_file(pipe_id_file_path());
}

#[cfg(windows)]
mod win32 {
    pub const PIPE_ACCESS_DUPLEX: u32 = 0x00000003;
    pub const PIPE_TYPE_MESSAGE: u32 = 0x00000004;
    pub const PIPE_READMODE_MESSAGE: u32 = 0x00000002;
    pub const PIPE_WAIT: u32 = 0x00000000;
    pub const OPEN_EXISTING: u32 = 3;
    pub const ERROR_PIPE_CONNECTED: u32 = 535;
}

#[cfg(windows)]
pub fn run_pipe_server(pipe_name: &str, proxy: EventLoopProxy<ViewerEvent>) {
    use windows_sys::Win32::Foundation::{INVALID_HANDLE_VALUE, CloseHandle, GENERIC_READ, GENERIC_WRITE};
    use windows_sys::Win32::Storage::FileSystem::{CreateFileW, ReadFile, WriteFile, FILE_ATTRIBUTE_NORMAL};
    use windows_sys::Win32::System::Pipes::{CreateNamedPipeW, ConnectNamedPipe, DisconnectNamedPipe};

    let pipe_name_wide: Vec<u16> = pipe_name.encode_utf16().chain(std::iter::once(0)).collect();

    let idle_timeout_secs: u64 = std::env::var("WIDGET_VIEWER_IDLE_TIMEOUT_SECS")
        .ok().and_then(|v| v.parse().ok())
        .unwrap_or(30 * 60);
    let should_stop = Arc::new(AtomicBool::new(false));

    let stop_flag = should_stop.clone();
    let timer_proxy = proxy.clone();
    let last_activity = Arc::new(std::sync::atomic::AtomicU64::new(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    ));
    let timer_activity = last_activity.clone();

    // Clone pipe name for timer thread's self-connect
    let timer_pipe_name = pipe_name.to_string();

    std::thread::spawn(move || {
        loop {
            let poll_interval = std::env::var("WIDGET_VIEWER_IDLE_POLL_SECS")
                .ok().and_then(|v| v.parse().ok())
                .unwrap_or(60);
            std::thread::sleep(std::time::Duration::from_secs(poll_interval));
            if stop_flag.load(Ordering::Relaxed) {
                break;
            }
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let last = timer_activity.load(Ordering::Relaxed);
            if now - last > idle_timeout_secs {
                eprintln!("Daemon idle timeout (30 min), shutting down.");
                let _ = timer_proxy.send_event(ViewerEvent::Close);
                stop_flag.store(true, Ordering::Relaxed);

                // Self-connect to unblock the blocking ConnectNamedPipe call.
                // Retry a few times in case the server is between loop iterations.
                let pipe_wide: Vec<u16> = timer_pipe_name
                    .encode_utf16()
                    .chain(std::iter::once(0))
                    .collect();
                for _ in 0..5 {
                    let h = unsafe {
                        CreateFileW(
                            pipe_wide.as_ptr(),
                            GENERIC_READ | GENERIC_WRITE,
                            0,
                            std::ptr::null(),
                            win32::OPEN_EXISTING,
                            FILE_ATTRIBUTE_NORMAL,
                            std::ptr::null_mut(),
                        )
                    };
                    if h != INVALID_HANDLE_VALUE {
                        unsafe { CloseHandle(h); }
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(200));
                }
                break;
            }
        }
    });

    loop {
        if should_stop.load(Ordering::Relaxed) {
            cleanup_pipe_id_file();
            break;
        }

        let handle = unsafe {
            CreateNamedPipeW(
                pipe_name_wide.as_ptr(),
                win32::PIPE_ACCESS_DUPLEX,
                win32::PIPE_TYPE_MESSAGE | win32::PIPE_READMODE_MESSAGE | win32::PIPE_WAIT,
                1,    // Max instances
                4096, // Out buffer
                4096, // In buffer
                5000, // Default timeout ms
                std::ptr::null(),
            )
        };

        if handle == INVALID_HANDLE_VALUE {
            eprintln!("Failed to create named pipe, retrying in 1s...");
            std::thread::sleep(std::time::Duration::from_secs(1));
            continue;
        }

        // Blocking wait for client
        let connected = unsafe { ConnectNamedPipe(handle, std::ptr::null_mut()) };

        if connected == 0 {
            let err = unsafe { windows_sys::Win32::Foundation::GetLastError() };
            if err != win32::ERROR_PIPE_CONNECTED {
                unsafe { CloseHandle(handle); }
                if should_stop.load(Ordering::Relaxed) {
                    break;
                }
                continue;
            }
        }

        // Update activity timestamp
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        last_activity.store(now, Ordering::Relaxed);

        // Read message
        let mut buffer = [0u8; 65536];
        let mut bytes_read: u32 = 0;
        let read_ok = unsafe {
            ReadFile(handle, buffer.as_mut_ptr().cast(), buffer.len() as u32, &mut bytes_read, std::ptr::null_mut())
        };

        let mut should_break = false;

        if read_ok != 0 && bytes_read > 0 {
            let msg_str = String::from_utf8_lossy(&buffer[..bytes_read as usize]);
            if let Ok(msg) = serde_json::from_str::<IpcMessage>(msg_str.trim()) {
                match msg {
                    IpcMessage::LoadWidget { file, title } => {
                        match std::fs::read_to_string(&file) {
                            Ok(html) => {
                                let title = title.unwrap_or_else(|| {
                                    Path::new(&file)
                                        .file_stem()
                                        .map(|s| s.to_string_lossy().to_string())
                                        .unwrap_or_else(|| "Widget".to_string())
                                });
                                let _ = proxy.send_event(ViewerEvent::LoadWidget { html, title, file: Some(file) });
                                let _ = proxy.send_event(ViewerEvent::ShowWindow);
                            }
                            Err(e) => eprintln!("Failed to read widget file {}: {}", file, e),
                        }
                    }
                    IpcMessage::UpdateWidget { html } => {
                        let _ = proxy.send_event(ViewerEvent::UpdateWidget(html));
                    }
                    IpcMessage::Show => {
                        let _ = proxy.send_event(ViewerEvent::ShowWindow);
                    }
                    IpcMessage::Close => {
                        let _ = proxy.send_event(ViewerEvent::Close);
                        cleanup_pipe_id_file();
                        should_stop.store(true, Ordering::Relaxed);
                        should_break = true;
                    }
                }

                // Unified ack for all message types
                let ack = b"OK";
                let mut written: u32 = 0;
                unsafe {
                    WriteFile(handle, ack.as_ptr().cast(), ack.len() as u32, &mut written, std::ptr::null_mut());
                }
            }
        }

        // Unified disconnect/close — exactly one per connection, no double-free
        unsafe {
            DisconnectNamedPipe(handle);
            CloseHandle(handle);
        }

        if should_break {
            break;
        }
    }
}

#[cfg(not(windows))]
pub fn run_pipe_server(_pipe_name: &str, _proxy: EventLoopProxy<ViewerEvent>) {
    eprintln!("Named pipe server is only supported on Windows");
}

pub fn try_send_load_widget(file: &Path) -> Result<(), String> {
    let msg = IpcMessage::LoadWidget {
        file: file.to_string_lossy().to_string(),
        title: file.file_stem().map(|s| s.to_string_lossy().to_string()),
    };
    send_message(&msg)
}

pub fn try_send_close() -> Result<(), String> {
    send_message(&IpcMessage::Close)
}

/// Try all known pipe IDs until one connects. Cleans up stale files along the way.
#[cfg(windows)]
fn send_message(msg: &IpcMessage) -> Result<(), String> {
    let pipe_ids = find_all_pipe_ids();
    if pipe_ids.is_empty() {
        return Err("No pipe ID file found".to_string());
    }

    let json = serde_json::to_string(msg).map_err(|e| e.to_string())?;

    use windows_sys::Win32::Foundation::{INVALID_HANDLE_VALUE, CloseHandle, GENERIC_READ, GENERIC_WRITE};
    use windows_sys::Win32::Storage::FileSystem::{CreateFileW, ReadFile, WriteFile, FILE_ATTRIBUTE_NORMAL};

    for (pipe_id, file_path) in &pipe_ids {
        let pipe_name = pipe_name_from_id(pipe_id);
        let pipe_name_wide: Vec<u16> = pipe_name.encode_utf16().chain(std::iter::once(0)).collect();

        let handle = unsafe {
            CreateFileW(
                pipe_name_wide.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                0,
                std::ptr::null(),
                win32::OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                std::ptr::null_mut(),
            )
        };

        if handle == INVALID_HANDLE_VALUE {
            // Stale pipe — clean up and try next
            let _ = std::fs::remove_file(file_path);
            continue;
        }

        let bytes = json.as_bytes();
        let mut written: u32 = 0;
        let write_ok = unsafe {
            WriteFile(handle, bytes.as_ptr().cast(), bytes.len() as u32, &mut written, std::ptr::null_mut())
        };

        if write_ok == 0 {
            unsafe { CloseHandle(handle); }
            let _ = std::fs::remove_file(file_path);
            continue;
        }

        // Read ack
        let mut buffer = [0u8; 256];
        let mut bytes_read: u32 = 0;
        unsafe {
            ReadFile(handle, buffer.as_mut_ptr().cast(), buffer.len() as u32, &mut bytes_read, std::ptr::null_mut());
            CloseHandle(handle);
        }

        return Ok(());
    }

    Err("Failed to connect to any daemon pipe".to_string())
}

#[cfg(not(windows))]
fn send_message(_msg: &IpcMessage) -> Result<(), String> {
    Err("Named pipe client is only supported on Windows".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Scenario 5: stale pipe file is cleaned up when send fails.
    ///
    /// Uses WIDGET_VIEWER_STATE_DIR to redirect pipe state to a temp dir,
    /// avoiding writes to the user's real ~/.claude/.
    /// SAFETY: this is the only test that sets WIDGET_VIEWER_STATE_DIR,
    /// so no concurrent reader conflict exists.
    #[test]
    #[cfg(windows)]
    fn test_stale_pipe_file_cleaned_up() {
        let tmp = tempfile::tempdir().unwrap();
        let state_dir = tmp.path();

        // Redirect pipe_id_dir() to our temp dir
        unsafe { std::env::set_var("WIDGET_VIEWER_STATE_DIR", state_dir); }

        // Create a fake pipe ID file for a non-existent PID
        let fake_file = state_dir.join(".widget-viewer-pipe-99999");
        std::fs::write(&fake_file, "99999").unwrap();

        // try_send_load_widget → send_message → find_all_pipe_ids scans state_dir,
        // finds our fake file, fails to connect, and should remove it.
        let dummy = std::path::Path::new("C:\\nonexistent\\widget.html");
        let _ = try_send_load_widget(dummy);

        unsafe { std::env::remove_var("WIDGET_VIEWER_STATE_DIR"); }

        assert!(
            !fake_file.exists(),
            "Stale pipe file should have been cleaned up, but still exists: {:?}",
            fake_file
        );
    }
}
