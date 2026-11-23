//! Cross-platform Unix socket IPC fallback (used for single-instance raise and
//! environments without a D-Bus session bus, such as minimal WMs or macOS).

use crate::{IpcCommand, IpcResponse, SessionStatusSnapshot};
use parking_lot::RwLock;
use region_platform::AppPaths;
use std::io::{BufRead, BufReader, Write};
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;

#[cfg(unix)]
use std::os::unix::net::{UnixListener, UnixStream};

/// Send a single command to an already running Region to Share instance.
#[cfg(unix)]
pub fn send_socket_command(cmd: &IpcCommand) -> Result<IpcResponse, String> {
    let path = AppPaths::ipc_socket_path();
    let mut stream =
        UnixStream::connect(&path).map_err(|e| format!("No running instance at {:?}: {e}", path))?;
    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(2)));
    let _ = stream.set_write_timeout(Some(std::time::Duration::from_secs(2)));

    let line = serde_json::to_string(cmd).map_err(|e| e.to_string())?;
    stream
        .write_all(format!("{line}\n").as_bytes())
        .map_err(|e| e.to_string())?;
    stream.flush().map_err(|e| e.to_string())?;

    let mut reader = BufReader::new(stream);
    let mut resp_line = String::new();
    reader.read_line(&mut resp_line).map_err(|e| e.to_string())?;
    if resp_line.trim().is_empty() {
        return Ok(IpcResponse::Ok {
            message: "accepted".into(),
        });
    }
    serde_json::from_str(&resp_line).map_err(|e| format!("Invalid IPC response: {e}"))
}

#[cfg(not(unix))]
pub fn send_socket_command(_cmd: &IpcCommand) -> Result<IpcResponse, String> {
    Err("Named pipe IPC client is planned for Phase 6 on Windows".into())
}

/// Spawn a lightweight background Unix socket listener feeding `cmd_tx`.
#[cfg(unix)]
pub fn spawn_socket_server(
    cmd_tx: UnboundedSender<IpcCommand>,
    shared_status: Arc<RwLock<SessionStatusSnapshot>>,
    on_wake: Option<Box<dyn Fn() + Send + 'static>>,
) {
    let path = AppPaths::ipc_socket_path();
    let _ = std::fs::remove_file(&path);
    std::thread::Builder::new()
        .name("region-ipc-socket".into())
        .spawn(move || {
            let listener = match UnixListener::bind(&path) {
                Ok(l) => l,
                Err(e) => {
                    log::warn!("[IPC] Failed to bind {:?}: {}", path, e);
                    return;
                }
            };
            for stream in listener.incoming().flatten() {
                let mut reader = BufReader::new(&stream);
                let mut line = String::new();
                if reader.read_line(&mut line).is_ok() {
                    let trimmed = line.trim();
                    let cmd: IpcCommand = if trimmed == "raise" {
                        IpcCommand::Raise
                    } else if let Ok(parsed) = serde_json::from_str(trimmed) {
                        parsed
                    } else {
                        IpcCommand::Raise
                    };

                    let response = match cmd {
                        IpcCommand::GetStatus => IpcResponse::Status(shared_status.read().clone()),
                        other => {
                            let _ = cmd_tx.send(other);
                            if let Some(ref wake) = on_wake {
                                wake();
                            }
                            IpcResponse::Ok {
                                message: "dispatched".into(),
                            }
                        }
                    };

                    if let Ok(mut out) = stream.try_clone() {
                        if let Ok(json) = serde_json::to_string(&response) {
                            let _ = out.write_all(format!("{json}\n").as_bytes());
                            let _ = out.flush();
                        }
                    }
                }
            }
        })
        .ok();
}

#[cfg(not(unix))]
pub fn spawn_socket_server(
    _cmd_tx: UnboundedSender<IpcCommand>,
    _shared_status: Arc<RwLock<SessionStatusSnapshot>>,
    _on_wake: Option<Box<dyn Fn() + Send + 'static>>,
) {
}
