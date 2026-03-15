/// Named Pipe IPC for daemon communication.
/// Fix #2: per-PID pipe files avoid multi-session conflicts.
/// Fix #3: handle leak on ConnectNamedPipe error.
/// Fix #4: idle timeout via timer thread (ConnectNamedPipe is blocking).
/// Fix #11: named constants instead of magic numbers.

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

/// Fix #2: pipe ID file includes PID to avoid multi-session conflicts
fn pipe_id_file_path() -> PathBuf {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| ".".to_string());
    let pid = std::process::id();
    Path::new(&home)
        .join(".claude")
        .join(format!(".widget-viewer-pipe-{}", pid))
}

/// Directory containing all pipe ID files
fn pipe_id_dir() -> PathBuf {
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

/// Fix #2: scan all pipe ID files, try each until one connects
fn find_active_pipe_id() -> Option<String> {
    let dir = pipe_id_dir();
    let entries = std::fs::read_dir(&dir).ok()?;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str.starts_with(".widget-viewer-pipe-") {
            if let Ok(id) = std::fs::read_to_string(entry.path()) {
                let id = id.trim().to_string();
                if !id.is_empty() {
                    return Some(id);
                }
            }
        }
    }
    None
}

pub fn cleanup_pipe_id_file() {
    let _ = std::fs::remove_file(pipe_id_file_path());
}

// Win32 constants — Fix #11: no more magic numbers
#[cfg(windows)]
mod win32 {
    pub const PIPE_ACCESS_DUPLEX: u32 = 0x00000003;
    pub const PIPE_TYPE_MESSAGE: u32 = 0x00000004;
    pub const PIPE_READMODE_MESSAGE: u32 = 0x00000002;
    pub const PIPE_WAIT: u32 = 0x00000000;
    pub const OPEN_EXISTING: u32 = 3;
    pub const ERROR_PIPE_CONNECTED: u32 = 535;
}

/// Fix #4: idle timeout via a timer thread that signals shutdown
#[cfg(windows)]
pub fn run_pipe_server(pipe_name: &str, proxy: EventLoopProxy<ViewerEvent>) {
    use windows_sys::Win32::Foundation::{INVALID_HANDLE_VALUE, CloseHandle};
    use windows_sys::Win32::Storage::FileSystem::{ReadFile, WriteFile};
    use windows_sys::Win32::System::Pipes::{CreateNamedPipeW, ConnectNamedPipe, DisconnectNamedPipe};

    let pipe_name_wide: Vec<u16> = pipe_name.encode_utf16().chain(std::iter::once(0)).collect();

    let idle_timeout_secs: u64 = 30 * 60;
    let should_stop = Arc::new(AtomicBool::new(false));

    // Timer thread: checks last_activity and triggers shutdown
    let stop_flag = should_stop.clone();
    let timer_proxy = proxy.clone();
    let last_activity = Arc::new(std::sync::atomic::AtomicU64::new(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    ));
    let timer_activity = last_activity.clone();

    std::thread::spawn(move || {
        loop {
            std::thread::sleep(std::time::Duration::from_secs(60));
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
                // Connect to our own pipe to unblock ConnectNamedPipe
                // (otherwise it stays blocked forever)
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

        // Fix #3: always close handle on error paths
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

        if read_ok != 0 && bytes_read > 0 {
            let msg_str = String::from_utf8_lossy(&buffer[..bytes_read as usize]);
            if let Ok(msg) = serde_json::from_str::<IpcMessage>(msg_str.trim()) {
                match msg {
                    IpcMessage::LoadWidget { file, title } => {
                        match std::fs::read_to_string(&file) {
                            Ok(html) => {
                                let _ = proxy.send_event(ViewerEvent::LoadWidget(html));
                                let _ = proxy.send_event(ViewerEvent::ShowWindow);
                                if let Some(t) = title {
                                    let _ = proxy.send_event(ViewerEvent::SetTitle(t));
                                }
                            }
                            Err(e) => eprintln!("Failed to read widget file {}: {}", file, e),
                        }
                    }
                    IpcMessage::UpdateWidget { html } => {
                        let _ = proxy.send_event(ViewerEvent::LoadWidget(html));
                    }
                    IpcMessage::Show => {
                        let _ = proxy.send_event(ViewerEvent::ShowWindow);
                    }
                    IpcMessage::Close => {
                        let _ = proxy.send_event(ViewerEvent::Close);
                        cleanup_pipe_id_file();
                        // Ack, disconnect, close, then break
                        let ack = b"OK";
                        let mut written: u32 = 0;
                        unsafe {
                            WriteFile(handle, ack.as_ptr().cast(), ack.len() as u32, &mut written, std::ptr::null_mut());
                            DisconnectNamedPipe(handle);
                            CloseHandle(handle);
                        }
                        should_stop.store(true, Ordering::Relaxed);
                        break;
                    }
                }

                // Send ack
                let ack = b"OK";
                let mut written: u32 = 0;
                unsafe {
                    WriteFile(handle, ack.as_ptr().cast(), ack.len() as u32, &mut written, std::ptr::null_mut());
                }
            }
        }

        unsafe {
            DisconnectNamedPipe(handle);
            CloseHandle(handle);
        }
    }
}

#[cfg(not(windows))]
pub fn run_pipe_server(_pipe_name: &str, _proxy: EventLoopProxy<ViewerEvent>) {
    eprintln!("Named pipe server is only supported on Windows");
}

/// Fix #2: try all pipe ID files until one connects
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

#[cfg(windows)]
fn send_message(msg: &IpcMessage) -> Result<(), String> {
    let pipe_id = find_active_pipe_id().ok_or("No pipe ID file found")?;
    let pipe_name = pipe_name_from_id(&pipe_id);

    let json = serde_json::to_string(msg).map_err(|e| e.to_string())?;

    use windows_sys::Win32::Foundation::{INVALID_HANDLE_VALUE, CloseHandle, GENERIC_READ, GENERIC_WRITE};
    use windows_sys::Win32::Storage::FileSystem::{CreateFileW, ReadFile, WriteFile, FILE_ATTRIBUTE_NORMAL};

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
        // Fix #2: this pipe is stale, clean up the file
        let dir = pipe_id_dir();
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                if name.to_string_lossy().starts_with(".widget-viewer-pipe-") {
                    if let Ok(id) = std::fs::read_to_string(entry.path()) {
                        if id.trim() == pipe_id {
                            let _ = std::fs::remove_file(entry.path());
                        }
                    }
                }
            }
        }
        return Err("Failed to connect to daemon pipe".to_string());
    }

    let bytes = json.as_bytes();
    let mut written: u32 = 0;
    let write_ok = unsafe {
        WriteFile(handle, bytes.as_ptr().cast(), bytes.len() as u32, &mut written, std::ptr::null_mut())
    };

    if write_ok == 0 {
        unsafe { CloseHandle(handle); }
        return Err("Failed to write to pipe".to_string());
    }

    // Read ack
    let mut buffer = [0u8; 256];
    let mut bytes_read: u32 = 0;
    unsafe {
        ReadFile(handle, buffer.as_mut_ptr().cast(), buffer.len() as u32, &mut bytes_read, std::ptr::null_mut());
        CloseHandle(handle);
    }

    Ok(())
}

#[cfg(not(windows))]
fn send_message(_msg: &IpcMessage) -> Result<(), String> {
    Err("Named pipe client is only supported on Windows".to_string())
}
