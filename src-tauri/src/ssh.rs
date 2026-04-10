use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use russh::client::{self, Handle, Msg};
use russh::keys::key::PublicKey;
use russh::{Channel, ChannelMsg, Disconnect};
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncWriteExt;
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;

use crate::known_hosts::{self, HostKeyStatus, KnownHostsStore};

/// Payload emitted to the frontend when SSH data arrives.
#[derive(Clone, serde::Serialize)]
struct SshOutputEvent {
    data: Vec<u8>,
}

/// Payload for connection-closed events.
#[derive(Clone, serde::Serialize)]
struct SshClosedEvent {
    reason: String,
}

/// Control messages sent from the main thread to the channel-owning reader task.
enum ControlMsg {
    Resize { cols: u32, rows: u32 },
}

/// Host key information captured during connection (SEC-3).
#[derive(Debug, Clone)]
pub struct CapturedHostKeyInfo {
    pub fingerprint: String,
    pub key_data: String,
    pub algorithm: String,
}

/// A live SSH connection with its IO handles.
struct SshConnection {
    /// AsyncWrite handle to send data to the SSH channel.
    writer: Arc<Mutex<Box<dyn tokio::io::AsyncWrite + Send + Unpin>>>,
    /// Send control messages (resize) to the reader task which owns the Channel.
    control_tx: mpsc::Sender<ControlMsg>,
    /// Background task reading SSH output.
    reader_task: JoinHandle<()>,
    /// The russh session handle (for disconnect).
    handle: Handle<ClientHandler>,
}

/// Thread-safe manager for all active SSH connections.
pub struct SshConnectionManager {
    connections: Arc<Mutex<HashMap<String, SshConnection>>>,
}

impl SshConnectionManager {
    pub fn new() -> Self {
        Self {
            connections: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Open an SSH connection, authenticate, request a PTY, and start
    /// streaming output to the frontend via Tauri events.
    pub async fn connect(
        &self,
        app: &AppHandle,
        connection_id: &str,
        host: &str,
        port: u16,
        username: &str,
        password: Option<&str>,
        private_key_path: Option<&str>,
    ) -> Result<(), ConnectError> {
        let known_hosts = KnownHostsStore::new().map_err(ConnectError::Io)?;

        // SEC-3: The handler captures host key info from the *first* connection
        // attempt so we never need a second connection to retrieve it.
        let captured_key = Arc::new(tokio::sync::Mutex::new(None::<CapturedHostKeyInfo>));
        let handler = ClientHandler {
            known_hosts,
            host: host.to_string(),
            port,
            captured_key: captured_key.clone(),
        };

        let config = Arc::new(client::Config {
            keepalive_interval: Some(std::time::Duration::from_secs(30)),
            ..Default::default()
        });

        let addr = format!("{}:{}", host, port);
        let mut session = match client::connect(config, &addr, handler).await {
            Ok(s) => s,
            Err(russh::Error::UnknownKey) => {
                // SEC-3: retrieve key info captured during this same attempt
                let info = captured_key.lock().await;
                if let Some(key_info) = info.as_ref() {
                    return Err(ConnectError::HostKeyUnknown(key_info.clone()));
                }
                return Err(ConnectError::Ssh(russh::Error::UnknownKey));
            }
            Err(e) => return Err(ConnectError::Ssh(e)),
        };

        // Authenticate
        let authenticated = if let Some(key_path) = private_key_path {
            let key_pair = russh_keys::load_secret_key(key_path, None)
                .map_err(|e| ConnectError::Auth(format!("Failed to load key: {}", e)))?;
            session
                .authenticate_publickey(username, Arc::new(key_pair))
                .await
                .map_err(ConnectError::Ssh)?
        } else if let Some(pw) = password {
            session
                .authenticate_password(username, pw)
                .await
                .map_err(ConnectError::Ssh)?
        } else {
            return Err(ConnectError::Auth(
                "No password or private key provided".to_string(),
            ));
        };

        if !authenticated {
            return Err(ConnectError::Auth("Authentication failed".to_string()));
        }

        // Open channel, request PTY + shell
        let channel = session
            .channel_open_session()
            .await
            .map_err(ConnectError::Ssh)?;

        channel
            .request_pty(false, "xterm-256color", 80, 24, 0, 0, &[])
            .await
            .map_err(ConnectError::Ssh)?;

        channel
            .request_shell(false)
            .await
            .map_err(ConnectError::Ssh)?;

        // Get a writer before moving the channel into the reader task
        let writer = channel.make_writer();
        let writer: Arc<Mutex<Box<dyn tokio::io::AsyncWrite + Send + Unpin>>> =
            Arc::new(Mutex::new(Box::new(writer)));

        // Create control channel for resize commands
        let (control_tx, control_rx) = mpsc::channel::<ControlMsg>(16);

        // RES-1: pass a clone of the connections map so the reader task
        // can auto-remove its entry when the channel closes.
        let cid = connection_id.to_string();
        let app_clone = app.clone();
        let connections_clone = self.connections.clone();
        let reader_task = tokio::spawn(channel_reader_task(
            channel,
            control_rx,
            cid.clone(),
            app_clone,
            connections_clone,
        ));

        let conn = SshConnection {
            writer,
            control_tx,
            reader_task,
            handle: session,
        };
        self.connections
            .lock()
            .await
            .insert(connection_id.to_string(), conn);

        Ok(())
    }

    /// Send raw bytes (user keystrokes) to an active SSH channel.
    ///
    /// RES-5: The connections mutex is released before performing async I/O
    /// so that one slow write doesn't block all other SSH operations.
    pub async fn send(&self, connection_id: &str, data: &[u8]) -> Result<(), ConnectError> {
        let writer = {
            let conns = self.connections.lock().await;
            let conn = conns
                .get(connection_id)
                .ok_or_else(|| ConnectError::NotFound(connection_id.to_string()))?;
            conn.writer.clone()
        };
        // Lock dropped — now write without holding the connections mutex.
        let mut w = writer.lock().await;
        w.write_all(data).await.map_err(ConnectError::Io)?;
        w.flush().await.map_err(ConnectError::Io)?;
        Ok(())
    }

    /// Notify the remote side of a terminal resize.
    ///
    /// RES-5: The connections mutex is released before the async send.
    pub async fn resize(
        &self,
        connection_id: &str,
        cols: u32,
        rows: u32,
    ) -> Result<(), ConnectError> {
        let tx = {
            let conns = self.connections.lock().await;
            let conn = conns
                .get(connection_id)
                .ok_or_else(|| ConnectError::NotFound(connection_id.to_string()))?;
            conn.control_tx.clone()
        };
        tx.send(ControlMsg::Resize { cols, rows })
            .await
            .map_err(|_| ConnectError::ChannelClosed("Channel reader task ended".to_string()))?;
        Ok(())
    }

    /// Disconnect and clean up a connection.
    pub async fn disconnect(&self, connection_id: &str) -> Result<(), ConnectError> {
        let mut conns = self.connections.lock().await;
        if let Some(conn) = conns.remove(connection_id) {
            conn.reader_task.abort();
            let _ = conn
                .handle
                .disconnect(Disconnect::ByApplication, "user disconnect", "en")
                .await;
        }
        Ok(())
    }
}

/// Background task that reads SSH channel output and handles control messages.
///
/// RES-1: On exit (EOF, close, connection lost), the task auto-removes its
/// entry from the connections HashMap so dead connections don't accumulate.
async fn channel_reader_task(
    mut channel: Channel<Msg>,
    mut control_rx: mpsc::Receiver<ControlMsg>,
    connection_id: String,
    app: AppHandle,
    connections: Arc<Mutex<HashMap<String, SshConnection>>>,
) {
    let event_name = format!("ssh-output-{}", connection_id);
    let close_event = format!("ssh-closed-{}", connection_id);

    loop {
        tokio::select! {
            msg = channel.wait() => {
                match msg {
                    Some(ChannelMsg::Data { data }) => {
                        let _ = app.emit(&event_name, SshOutputEvent {
                            data: data.to_vec(),
                        });
                    }
                    Some(ChannelMsg::ExtendedData { data, ext }) => {
                        if ext == 1 {
                            let _ = app.emit(&event_name, SshOutputEvent {
                                data: data.to_vec(),
                            });
                        }
                    }
                    Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) => {
                        let _ = app.emit(&close_event, SshClosedEvent {
                            reason: "Connection closed".to_string(),
                        });
                        break;
                    }
                    Some(ChannelMsg::ExitStatus { exit_status }) => {
                        let _ = app.emit(&close_event, SshClosedEvent {
                            reason: format!("Exited with status {}", exit_status),
                        });
                        break;
                    }
                    None => {
                        let _ = app.emit(&close_event, SshClosedEvent {
                            reason: "Connection lost".to_string(),
                        });
                        break;
                    }
                    _ => {}
                }
            }
            ctrl = control_rx.recv() => {
                match ctrl {
                    Some(ControlMsg::Resize { cols, rows }) => {
                        let _ = channel.window_change(cols, rows, 0, 0).await;
                    }
                    None => break, // Control channel closed, connection is being dropped
                }
            }
        }
    }

    // RES-1: auto-remove this connection from the HashMap
    connections.lock().await.remove(&connection_id);
}

/// The russh client handler — validates server host keys.
///
/// SEC-3: captures key info from the first connection attempt so a
/// second connection is never needed.
pub struct ClientHandler {
    known_hosts: KnownHostsStore,
    host: String,
    port: u16,
    /// Stores the server key info on rejection so the caller can surface it.
    captured_key: Arc<tokio::sync::Mutex<Option<CapturedHostKeyInfo>>>,
}

#[async_trait]
impl client::Handler for ClientHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &PublicKey,
    ) -> Result<bool, Self::Error> {
        // Always capture key info so it is available if we reject.
        let algorithm = known_hosts::key_algorithm(server_public_key);
        let key_data = known_hosts::key_to_base64(server_public_key);
        let fingerprint = format!(
            "{} {}",
            algorithm,
            &key_data[..32.min(key_data.len())]
        );

        match self
            .known_hosts
            .check(&self.host, self.port, server_public_key)
        {
            HostKeyStatus::Known => Ok(true),
            HostKeyStatus::Unknown { .. } | HostKeyStatus::Changed { .. } => {
                // SEC-3: store key info for the caller before rejecting
                let mut captured = self.captured_key.lock().await;
                *captured = Some(CapturedHostKeyInfo {
                    fingerprint,
                    key_data,
                    algorithm,
                });
                Ok(false)
            }
        }
    }
}

/// Errors that can occur during SSH connection lifecycle.
#[derive(Debug, thiserror::Error)]
pub enum ConnectError {
    #[error("SSH error: {0}")]
    Ssh(#[from] russh::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Authentication failed: {0}")]
    Auth(String),
    #[error("Connection not found: {0}")]
    NotFound(String),
    #[error("Channel closed: {0}")]
    ChannelClosed(String),
    #[error("Host key unknown")]
    HostKeyUnknown(CapturedHostKeyInfo),
}

impl serde::Serialize for ConnectError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}
