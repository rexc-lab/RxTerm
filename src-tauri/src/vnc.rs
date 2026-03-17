use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

/// Maximum number of concurrent VNC proxy connections allowed.
const MAX_VNC_CONNECTIONS: usize = 16;

/// A running VNC WebSocket proxy instance.
#[allow(dead_code)]
struct VncProxy {
    /// The local port the WebSocket server is listening on.
    ws_port: u16,
    /// Handle to the listener task (for cleanup).
    listener_task: JoinHandle<()>,
}

/// Thread-safe manager for all active VNC proxy connections.
pub struct VncConnectionManager {
    proxies: Arc<Mutex<HashMap<String, VncProxy>>>,
}

impl VncConnectionManager {
    pub fn new() -> Self {
        Self {
            proxies: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Start a local WebSocket-to-TCP proxy for a VNC server.
    ///
    /// The proxy binds to `127.0.0.1` on a random available port, accepts
    /// exactly one WebSocket connection, and forwards traffic between the
    /// WebSocket client (noVNC in the frontend) and the remote VNC server
    /// at `vnc_host:vnc_port`.
    ///
    /// Returns the local WebSocket port the frontend should connect to.
    pub async fn start_proxy(
        &self,
        connection_id: &str,
        vnc_host: &str,
        vnc_port: u16,
    ) -> Result<u16, VncError> {
        // Enforce connection limit to prevent resource exhaustion
        {
            let proxies = self.proxies.lock().await;
            if proxies.len() >= MAX_VNC_CONNECTIONS {
                return Err(VncError::TooManyConnections);
            }
        }

        // Validate host format
        validate_vnc_host(vnc_host)?;

        // Bind to a random port on localhost
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(VncError::Io)?;
        let local_addr = listener.local_addr().map_err(VncError::Io)?;
        let ws_port = local_addr.port();

        let vnc_addr = format!("{}:{}", vnc_host, vnc_port);
        let cid = connection_id.to_string();

        // Spawn the listener task — accepts one connection then proxies
        let listener_task = tokio::spawn(async move {
            if let Err(e) = run_proxy(listener, &vnc_addr, &cid).await {
                log::error!("[VNC proxy {}] connection error", cid);
                log::debug!("[VNC proxy {}] error details: {}", cid, e);
            }
        });

        let proxy = VncProxy {
            ws_port,
            listener_task,
        };
        self.proxies
            .lock()
            .await
            .insert(connection_id.to_string(), proxy);

        Ok(ws_port)
    }

    /// Stop and clean up a VNC proxy connection.
    pub async fn stop_proxy(&self, connection_id: &str) -> Result<(), VncError> {
        let mut proxies = self.proxies.lock().await;
        if let Some(proxy) = proxies.remove(connection_id) {
            proxy.listener_task.abort();
        }
        Ok(())
    }
}

/// Validate that the VNC host is a reasonable hostname or IP address.
fn validate_vnc_host(host: &str) -> Result<(), VncError> {
    if host.is_empty() || host.len() > 253 {
        return Err(VncError::InvalidHost(
            "Host must be between 1 and 253 characters".to_string(),
        ));
    }

    // Try parsing as an IP address first
    if host.parse::<std::net::IpAddr>().is_ok() {
        return Ok(());
    }

    // Validate as a hostname: alphanumeric, dots, hyphens, and underscores
    if !host
        .chars()
        .all(|c| c.is_alphanumeric() || c == '.' || c == '-' || c == '_')
    {
        return Err(VncError::InvalidHost(format!(
            "Host contains invalid characters: {}",
            host
        )));
    }

    Ok(())
}

/// Run the WebSocket proxy: accept one WebSocket connection, connect to
/// the VNC server, and bidirectionally forward data until either side closes.
async fn run_proxy(
    listener: TcpListener,
    vnc_addr: &str,
    _connection_id: &str,
) -> Result<(), VncError> {
    // Wait for the noVNC WebSocket client to connect
    let (ws_stream, peer_addr): (TcpStream, SocketAddr) =
        listener.accept().await.map_err(VncError::Io)?;

    // Only accept connections from localhost
    if !peer_addr.ip().is_loopback() {
        return Err(VncError::Io(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "only localhost connections are allowed",
        )));
    }

    // Upgrade the TCP connection to a WebSocket connection
    let ws = tokio_tungstenite::accept_async(ws_stream)
        .await
        .map_err(|e| VncError::WebSocket(e.to_string()))?;

    // Connect to the actual VNC server
    let vnc_tcp = TcpStream::connect(vnc_addr)
        .await
        .map_err(VncError::Io)?;
    let (mut vnc_reader, mut vnc_writer) = tokio::io::split(vnc_tcp);

    let (mut ws_sink, mut ws_source) = ws.split();

    // Forward: VNC server → WebSocket client
    let vnc_to_ws = tokio::spawn(async move {
        let mut buf = [0u8; 16384];
        loop {
            match vnc_reader.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    let msg =
                        tokio_tungstenite::tungstenite::Message::Binary(buf[..n].to_vec().into());
                    if ws_sink.send(msg).await.is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    // Forward: WebSocket client → VNC server
    let ws_to_vnc = tokio::spawn(async move {
        while let Some(msg) = ws_source.next().await {
            match msg {
                Ok(tokio_tungstenite::tungstenite::Message::Binary(data)) => {
                    if vnc_writer.write_all(&data).await.is_err() {
                        break;
                    }
                    if vnc_writer.flush().await.is_err() {
                        break;
                    }
                }
                Ok(tokio_tungstenite::tungstenite::Message::Close(_)) => break,
                Err(_) => break,
                _ => {}
            }
        }
    });

    // Wait for either direction to finish, then abort the other
    tokio::select! {
        _ = vnc_to_ws => {}
        _ = ws_to_vnc => {}
    }

    Ok(())
}

/// Errors that can occur during VNC proxy lifecycle.
#[derive(Debug, thiserror::Error)]
pub enum VncError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("WebSocket error: {0}")]
    WebSocket(String),
    #[error("Invalid VNC host: {0}")]
    InvalidHost(String),
    #[error("Too many VNC connections (max {MAX_VNC_CONNECTIONS})")]
    TooManyConnections,
}

impl serde::Serialize for VncError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}
