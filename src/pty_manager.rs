//! PTY (Pseudo-Terminal) spawning and management.
//!
//! Handles spawning shells, reading output, and writing input.

use std::collections::HashMap;
use std::io::Write;
use std::sync::{Arc, Mutex};
use std::thread;

use portable_pty::{native_pty_system, CommandBuilder, PtySize, Child, MasterPty};
use tokio::sync::{mpsc, broadcast};
use tracing::{info, error, warn};

use crate::state::AppState;

/// Returns the platform's preferred interactive shell.
pub fn default_shell_program() -> String {
    #[cfg(windows)]
    {
        "powershell.exe".to_string()
    }

    #[cfg(not(windows))]
    {
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string())
    }
}

/// Maps frontend shell aliases to a platform-appropriate executable.
pub fn resolve_shell_program(shell: &str) -> String {
    let trimmed = shell.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("default") {
        return default_shell_program();
    }

    #[cfg(windows)]
    {
        match trimmed.to_ascii_lowercase().as_str() {
            "powershell" => "powershell.exe".to_string(),
            "cmd" => "cmd.exe".to_string(),
            "wsl" => "wsl.exe".to_string(),
            _ => trimmed.to_string(),
        }
    }

    #[cfg(not(windows))]
    {
        match trimmed.to_ascii_lowercase().as_str() {
            "powershell" | "powershell.exe" | "cmd" | "cmd.exe" | "wsl" | "wsl.exe" => {
                default_shell_program()
            }
            _ => trimmed.to_string(),
        }
    }
}

/// Manages PTY instances and their associated tasks.
pub struct PtyManager {
    /// Map of pane_id to PtyInstance
    instances: Arc<Mutex<HashMap<String, PtyInstance>>>,
    /// Map of pane_id to master writer for input forwarding
    master_writer: Arc<Mutex<HashMap<String, Box<dyn Write + Send>>>>,
    /// Map of pane_id to master PTY for resize operations
    master_pty: Arc<Mutex<HashMap<String, Box<dyn MasterPty + Send>>>>,
}

struct PtyInstance {
    #[allow(dead_code)]
    child: Box<dyn Child + Send + Sync>,
}

impl PtyManager {
    /// Creates a new PTY manager.
    pub fn new() -> Self {
        Self {
            instances: Arc::new(Mutex::new(HashMap::new())),
            master_writer: Arc::new(Mutex::new(HashMap::new())),
            master_pty: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Spawns a new PTY with the specified shell.
    ///
    /// Returns the pane ID and PID on success.
    pub fn spawn_pty(
        &self,
        shell: &str,
        cols: u16,
        rows: u16,
        state: AppState,
        broadcast_tx: &broadcast::Sender<crate::messages::ServerMessage>,
    ) -> Result<(String, u32), Box<dyn std::error::Error + Send + Sync>> {
        let pty_system = native_pty_system();

        let pair = pty_system.openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        let shell_program = resolve_shell_program(shell);

        let mut cmd = CommandBuilder::new(&shell_program);
        cmd.env("TERM", "xterm-256color");
        // Set window title
        cmd.env("PROMPT", "$P$G");

        info!("Spawning shell: {} with cols={}, rows={}", shell_program, cols, rows);

        let child = pair.slave.spawn_command(cmd)?;
        let pid = child.process_id().ok_or("Failed to get process ID")?;

        // Create pane ID
        let pane_id = uuid::Uuid::new_v4().to_string();

        // Clone references for the async task
        let pane_id_clone = pane_id.clone();
        let broadcast_tx_clone = broadcast_tx.clone(); // Now this is correct since broadcast_tx is &Sender and .clone() gives Sender
        let instances_clone = self.instances.clone();
        let master_writer_clone = self.master_writer.clone();
        let master_pty_clone = self.master_pty.clone();
        let state_clone = state.clone();

        // Get master writer for input
        let master_writer = pair.master.take_writer()?;

        // Get the master reader for output
        let mut reader = pair.master.try_clone_reader()?;

        // We need to take ownership of the master for resize capability
        // Since we already have a try_clone_reader, we need another approach
        // For now, we'll use the raw master after taking the writer
        let master_for_resize: Box<dyn MasterPty + Send> = unsafe {
            // Transmute to get the master PTY - this is safe because we're the only owner
            // and we won't use the original pair.master after this
            std::mem::transmute(pair.master)
        };

        // Spawn async task to read PTY output
        tokio::spawn(async move {
            // Store the instance, writer, and master pty
            if let Ok(mut instances) = instances_clone.lock() {
                instances.insert(pane_id_clone.clone(), PtyInstance { child });
            }
            if let Ok(mut writers) = master_writer_clone.lock() {
                writers.insert(pane_id_clone.clone(), master_writer);
            }
            if let Ok(mut master_map) = master_pty_clone.lock() {
                master_map.insert(pane_id_clone.clone(), master_for_resize);
            }

            // Use a channel to communicate between blocking thread and async context
            let (read_tx, mut read_rx) = mpsc::channel::<Result<Vec<u8>, std::io::Error>>(100);

            // Spawn a blocking thread to read from the PTY
            thread::spawn(move || {
                let mut buf = [0u8; 8192];
                loop {
                    match std::io::Read::read(&mut reader, &mut buf) {
                        Ok(0) => {
                            // EOF
                            break;
                        }
                        Ok(n) => {
                            let data = buf[..n].to_vec();
                            if read_tx.blocking_send(Ok(data)).is_err() {
                                // Receiver dropped
                                break;
                            }
                        }
                        Err(e) => {
                            let _ = read_tx.blocking_send(Err(e));
                            break;
                        }
                    }
                }
            });

            // Receive data in async loop and forward to WebSocket
            while let Some(result) = read_rx.recv().await {
                match result {
                    Ok(data) => {
                        if data.is_empty() {
                            break;
                        }
                        // Avoid unnecessary string allocation: if data is valid UTF-8,
                        // use it directly without lossy conversion. Only fall back to
                        // lossy conversion for invalid sequences (rare on Unix, more common on Windows ConPTY).
                        let text = String::from_utf8(data)
                            .unwrap_or_else(|e| String::from_utf8_lossy(&e.into_bytes()).to_string());
                        tracing::debug!("PTY output ({} bytes) for pane {}", text.len(), pane_id_clone);

                        // Store in scrollback buffer
                        state_clone.append_pane_buffer(&pane_id_clone, text.as_bytes()).await;

                        let msg = crate::messages::ServerMessage::Output {
                            pane_id: pane_id_clone.clone(),
                            data: text,
                        };

                        if broadcast_tx_clone.send(msg).is_err() {
                            warn!("No active receivers for broadcast");
                        }
                    }
                    Err(e) => {
                        error!("PTY read error: {}", e);
                        break;
                    }
                }
            }

            // Remove instance on exit
            if let Ok(mut instances) = instances_clone.lock() {
                instances.remove(&pane_id_clone);
            }
            if let Ok(mut writers) = master_writer_clone.lock() {
                writers.remove(&pane_id_clone);
            }
            if let Ok(mut master_map) = master_pty_clone.lock() {
                master_map.remove(&pane_id_clone);
            }

            // Notify that pane was killed (via broadcast so all clients see it)
            let _ = state_clone.remove_pane(&pane_id_clone).await;
            let panes = state_clone.get_panes_info().await;
            let active_panes = state_clone.get_active_panes().await;
            let floating_panes = state_clone.get_floating_panes().await;
            let groups = state_clone.get_all_groups().await;
            let _ = broadcast_tx_clone.send(crate::messages::ServerMessage::StateUpdate { panes, active_panes, floating_panes, groups });
        });

        // Return (pane_id, pid) - the caller should add the pane to state with correct ID
        Ok((pane_id, pid))
    }

    /// Writes input to a PTY pane.
    pub fn write_input(
        &self,
        pane_id: &str,
        data: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut writers = self.master_writer.lock().map_err(|e| format!("Lock error: {}", e))?;

        if let Some(writer) = writers.get_mut(pane_id) {
            // Write to the PTY master
            writer.write_all(data.as_bytes())?;
            writer.flush()?;
            tracing::info!("Writing {} bytes to pane {}", data.len(), pane_id);
        } else {
            tracing::error!("No writer found for pane {}", pane_id);
            return Err(format!("No writer found for pane {}", pane_id).into());
        }
        Ok(())
    }

    /// Resizes a PTY pane.
    pub fn resize_pty(
        &self,
        pane_id: &str,
        cols: u16,
        rows: u16,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let master_map = self.master_pty.lock().map_err(|e| format!("Lock error: {}", e))?;

        if let Some(master) = master_map.get(pane_id) {
            master.resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })?;
            tracing::info!("Resize pane {} to {}x{}", pane_id, cols, rows);
        } else {
            return Err(format!("No master PTY found for pane {}", pane_id).into());
        }
        Ok(())
    }

    /// Kills a PTY pane.
    pub fn kill_pty(
        &self,
        pane_id: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Remove from instances first, which will drop the child and kill the process
        let mut instances = self.instances.lock().map_err(|e| format!("Lock error: {}", e))?;
        if instances.remove(pane_id).is_some() {
            tracing::info!("Killing pane {}", pane_id);
        }

        // Also remove writer and master
        drop(instances);
        if let Ok(mut writers) = self.master_writer.lock() {
            writers.remove(pane_id);
        }
        if let Ok(mut master_map) = self.master_pty.lock() {
            master_map.remove(pane_id);
        }

        Ok(())
    }

    /// Writes input to a PTY pane and flushes immediately.
    pub fn write_input_raw(
        &self,
        pane_id: &str,
        data: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut writers = self.master_writer.lock().map_err(|e| format!("Lock error: {}", e))?;

        if let Some(writer) = writers.get_mut(pane_id) {
            writer.write_all(data.as_bytes())?;
            writer.flush()?;
        }
        Ok(())
    }

    /// Spawns a Lazygit PTY in the specified directory.
    ///
    /// Returns the pane ID and PID on success.
    pub async fn spawn_lazygit(
        &self,
        cwd: &str,
        cols: u16,
        rows: u16,
        state: AppState,
        broadcast_tx: &broadcast::Sender<crate::messages::ServerMessage>,
    ) -> Result<(String, u32), Box<dyn std::error::Error + Send + Sync>> {
        let pty_system = native_pty_system();

        let pair = pty_system.openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        // Build lazygit command
        let mut cmd = CommandBuilder::new("lazygit");
        cmd.env("TERM", "xterm-256color");
        // Set the working directory
        if !cwd.is_empty() {
            if cfg!(windows) {
                cmd.cwd(cwd);
            } else {
                cmd.cwd(cwd);
            }
        }

        info!("Spawning Lazygit at {} with cols={}, rows={}", cwd, cols, rows);

        let child = pair.slave.spawn_command(cmd)?;
        let pid = child.process_id().ok_or("Failed to get process ID")?;

        // Create pane ID
        let pane_id = uuid::Uuid::new_v4().to_string();

        // Clone references for the async task
        let pane_id_clone = pane_id.clone();
        let broadcast_tx_clone = broadcast_tx.clone();
        let instances_clone = self.instances.clone();
        let master_writer_clone = self.master_writer.clone();
        let master_pty_clone = self.master_pty.clone();
        let state_clone = state.clone();

        // Get master writer for input
        let master_writer = pair.master.take_writer()?;

        // Get the master reader for output
        let mut reader = pair.master.try_clone_reader()?;

        let master_for_resize: Box<dyn MasterPty + Send> = unsafe {
            std::mem::transmute(pair.master)
        };

        // Spawn async task to read PTY output
        tokio::spawn(async move {
            // Store the instance, writer, and master pty
            if let Ok(mut instances) = instances_clone.lock() {
                instances.insert(pane_id_clone.clone(), PtyInstance { child });
            }
            if let Ok(mut writers) = master_writer_clone.lock() {
                writers.insert(pane_id_clone.clone(), master_writer);
            }
            if let Ok(mut master_map) = master_pty_clone.lock() {
                master_map.insert(pane_id_clone.clone(), master_for_resize);
            }

            // Use a channel to communicate between blocking thread and async context
            let (read_tx, mut read_rx) = mpsc::channel::<Result<Vec<u8>, std::io::Error>>(100);

            // Spawn a blocking thread to read from the PTY
            thread::spawn(move || {
                let mut buf = [0u8; 8192];
                loop {
                    match std::io::Read::read(&mut reader, &mut buf) {
                        Ok(0) => {
                            // EOF
                            break;
                        }
                        Ok(n) => {
                            let data = buf[..n].to_vec();
                            if read_tx.blocking_send(Ok(data)).is_err() {
                                // Receiver dropped
                                break;
                            }
                        }
                        Err(e) => {
                            let _ = read_tx.blocking_send(Err(e));
                            break;
                        }
                    }
                }
            });

            // Receive data in async loop and forward to WebSocket
            while let Some(result) = read_rx.recv().await {
                match result {
                    Ok(data) => {
                        if data.is_empty() {
                            break;
                        }
                        let text = String::from_utf8(data)
                            .unwrap_or_else(|e| String::from_utf8_lossy(&e.into_bytes()).to_string());
                        tracing::debug!("Lazygit output ({} bytes) for pane {}", text.len(), pane_id_clone);

                        // Store in scrollback buffer
                        state_clone.append_pane_buffer(&pane_id_clone, text.as_bytes()).await;

                        let msg = crate::messages::ServerMessage::Output {
                            pane_id: pane_id_clone.clone(),
                            data: text,
                        };

                        if broadcast_tx_clone.send(msg).is_err() {
                            warn!("No active receivers for broadcast");
                        }
                    }
                    Err(e) => {
                        error!("Lazygit PTY read error: {}", e);
                        break;
                    }
                }
            }

            // Remove instance on exit
            if let Ok(mut instances) = instances_clone.lock() {
                instances.remove(&pane_id_clone);
            }
            if let Ok(mut writers) = master_writer_clone.lock() {
                writers.remove(&pane_id_clone);
            }
            if let Ok(mut master_map) = master_pty_clone.lock() {
                master_map.remove(&pane_id_clone);
            }

            // Notify that pane was killed (via broadcast so all clients see it)
            let _ = state_clone.remove_pane(&pane_id_clone).await;
            let panes = state_clone.get_panes_info().await;
            let active_panes = state_clone.get_active_panes().await;
            let floating_panes = state_clone.get_floating_panes().await;
            let groups = state_clone.get_all_groups().await;
            let _ = broadcast_tx_clone.send(crate::messages::ServerMessage::StateUpdate { panes, active_panes, floating_panes, groups });
        });

        // Add the pane to state with correct ID
        let mut pane = crate::state::Pane::new(pid, "lazygit".to_string(), cols, rows);
        pane.id = pane_id.clone();
        pane.cwd = Some(cwd.to_string());
        state.add_pane(pane).await;

        // Return (pane_id, pid)
        Ok((pane_id, pid))
    }
}

impl Default for PtyManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pty_manager_creation() {
        let manager = PtyManager::new();
        assert!(manager.instances.lock().is_ok());
        assert!(manager.master_writer.lock().is_ok());
    }
}
