/// Named Pipe IPC for daemon communication.
/// Server runs in the daemon process, client used by `send` and `stop` commands.

use crate::viewer::ViewerEvent;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use winit::event_loop::EventLoopProxy;

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type")]
pub enum IpcMessage {
    LoadWidget {
        file: String,
        title: Option<String>,
    },
    UpdateWidget {
        html: String,
    },
    Show,
    Close,
}

/// Generate a unique pipe ID based on process ID
pub fn generate_pipe_id() -> String {
    format!("{}", std::process::id())
}

/// Get the pipe name from an ID
pub fn pipe_name_from_id(id: &str) -> String {
    format!(r"\\.\pipe\claude-widget-viewer-{}", id)
}

/// Path to the file that stores the current daemon's pipe ID
fn pipe_id_file_path() -> PathBuf {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| ".".to_string());
    Path::new(&home)
        .join(".claude")
        .join(".widget-viewer-pipe")
}

/// Write the pipe ID to the discovery file
pub fn write_pipe_id_file(id: &str) {
    let path = pipe_id_file_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&path, id);
}

/// Read the pipe ID from the discovery file
fn read_pipe_id_file() -> Option<String> {
    std::fs::read_to_string(pipe_id_file_path()).ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Remove the pipe ID file on shutdown
pub fn cleanup_pipe_id_file() {
    let _ = std::fs::remove_file(pipe_id_file_path());
}

/// Run the Named Pipe server (blocking, run in a background thread).
/// Listens for JSON messages and forwards them as ViewerEvents.
#[cfg(windows)]
pub fn run_pipe_server(pipe_name: &str, proxy: EventLoopProxy<ViewerEvent>) {
    use windows_sys::Win32::Foundation::{INVALID_HANDLE_VALUE, CloseHandle};
    use windows_sys::Win32::Storage::FileSystem::{ReadFile, WriteFile};
    use windows_sys::Win32::System::Pipes::CreateNamedPipeW;

    let pipe_name_wide: Vec<u16> = pipe_name.encode_utf16().chain(std::iter::once(0)).collect();

    let idle_timeout = std::time::Duration::from_secs(30 * 60); // 30 minutes
    let mut last_activity = std::time::Instant::now();

    loop {
        // Check idle timeout
        if last_activity.elapsed() > idle_timeout {
            eprintln!("Daemon idle timeout (30 min), shutting down.");
            let _ = proxy.send_event(ViewerEvent::Close);
            cleanup_pipe_id_file();
            break;
        }

        // Create a new pipe instance
        let handle = unsafe {
            CreateNamedPipeW(
                pipe_name_wide.as_ptr(),
                0x00000003, // PIPE_ACCESS_DUPLEX
                0x00000004 | 0x00000002 | 0x00000000, // PIPE_TYPE_MESSAGE | PIPE_READMODE_MESSAGE | PIPE_WAIT
                1,          // Max instances
                4096,       // Out buffer size
                4096,       // In buffer size
                5000,       // Default timeout ms
                std::ptr::null(),
            )
        };

        if handle == INVALID_HANDLE_VALUE {
            eprintln!("Failed to create named pipe, retrying in 1s...");
            std::thread::sleep(std::time::Duration::from_secs(1));
            continue;
        }

        // Wait for a client to connect (blocking)
        let connected = unsafe {
            windows_sys::Win32::System::Pipes::ConnectNamedPipe(handle, std::ptr::null_mut())
        };

        // ConnectNamedPipe returns 0 on success if client connected after creation,
        // or if client was already connected (ERROR_PIPE_CONNECTED)
        if connected == 0 {
            let err = unsafe { windows_sys::Win32::Foundation::GetLastError() };
            if err != 535 { // ERROR_PIPE_CONNECTED
                unsafe { CloseHandle(handle); }
                continue;
            }
        }

        last_activity = std::time::Instant::now();

        // Read message from client
        let mut buffer = [0u8; 65536];
        let mut bytes_read: u32 = 0;
        let read_ok = unsafe {
            ReadFile(
                handle,
                buffer.as_mut_ptr().cast(),
                buffer.len() as u32,
                &mut bytes_read,
                std::ptr::null_mut(),
            )
        };

        if read_ok != 0 && bytes_read > 0 {
            let msg_str = String::from_utf8_lossy(&buffer[..bytes_read as usize]);
            if let Ok(msg) = serde_json::from_str::<IpcMessage>(msg_str.trim()) {
                match msg {
                    IpcMessage::LoadWidget { file, title } => {
                        // Read the file content
                        match std::fs::read_to_string(&file) {
                            Ok(html) => {
                                let _ = proxy.send_event(ViewerEvent::LoadWidget(html));
                                let _ = proxy.send_event(ViewerEvent::ShowWindow);
                                if let Some(t) = title {
                                    let _ = proxy.send_event(ViewerEvent::SetTitle(t));
                                }
                            }
                            Err(e) => {
                                eprintln!("Failed to read widget file {}: {}", file, e);
                            }
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
                        unsafe { CloseHandle(handle); }
                        break;
                    }
                }

                // Send acknowledgment
                let ack = b"OK";
                let mut written: u32 = 0;
                unsafe {
                    WriteFile(handle, ack.as_ptr().cast(), ack.len() as u32, &mut written, std::ptr::null_mut());
                }
            }
        }

        // Disconnect and close this pipe instance, loop to create a new one
        unsafe {
            windows_sys::Win32::System::Pipes::DisconnectNamedPipe(handle);
            CloseHandle(handle);
        }
    }
}

#[cfg(not(windows))]
pub fn run_pipe_server(_pipe_name: &str, _proxy: EventLoopProxy<ViewerEvent>) {
    eprintln!("Named pipe server is only supported on Windows");
}

/// Try to send a LoadWidget message to the daemon.
pub fn try_send_load_widget(file: &Path) -> Result<(), String> {
    let msg = IpcMessage::LoadWidget {
        file: file.to_string_lossy().to_string(),
        title: file.file_stem().map(|s| s.to_string_lossy().to_string()),
    };
    send_message(&msg)
}

/// Try to send a Close message to the daemon.
pub fn try_send_close() -> Result<(), String> {
    send_message(&IpcMessage::Close)
}

/// Send a message to the daemon via Named Pipe.
#[cfg(windows)]
fn send_message(msg: &IpcMessage) -> Result<(), String> {
    let pipe_id = read_pipe_id_file().ok_or("No pipe ID file found")?;
    let pipe_name = pipe_name_from_id(&pipe_id);

    let json = serde_json::to_string(msg).map_err(|e| e.to_string())?;

    // Open the named pipe as a file
    use windows_sys::Win32::Foundation::{INVALID_HANDLE_VALUE, CloseHandle, GENERIC_READ, GENERIC_WRITE};
    use windows_sys::Win32::Storage::FileSystem::{CreateFileW, ReadFile, WriteFile, FILE_ATTRIBUTE_NORMAL};

    let pipe_name_wide: Vec<u16> = pipe_name.encode_utf16().chain(std::iter::once(0)).collect();

    let handle = unsafe {
        CreateFileW(
            pipe_name_wide.as_ptr(),
            GENERIC_READ | GENERIC_WRITE,
            0,
            std::ptr::null(),
            3, // OPEN_EXISTING
            FILE_ATTRIBUTE_NORMAL,
            std::ptr::null_mut(),
        )
    };

    if handle == INVALID_HANDLE_VALUE {
        return Err("Failed to connect to daemon pipe".to_string());
    }

    // Write the message
    let bytes = json.as_bytes();
    let mut written: u32 = 0;
    let write_ok = unsafe {
        WriteFile(handle, bytes.as_ptr().cast(), bytes.len() as u32, &mut written, std::ptr::null_mut())
    };

    if write_ok == 0 {
        unsafe { CloseHandle(handle); }
        return Err("Failed to write to pipe".to_string());
    }

    // Read acknowledgment
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
