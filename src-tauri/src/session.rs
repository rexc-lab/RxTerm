use serde::{Deserialize, Serialize};

/// Connection protocol for a session.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Protocol {
    /// SSH terminal session.
    Ssh,
    /// VNC remote desktop session.
    Vnc,
    /// RDP remote desktop session.
    Rdp,
}

impl Default for Protocol {
    fn default() -> Self {
        Self::Ssh
    }
}

/// Authentication method for an SSH session.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AuthMethod {
    /// Authenticate with a password.
    Password,
    /// Authenticate with an SSH private key (optionally passphrase-protected).
    Key,
}

/// Represents all user-configurable fields for a single session.
///
/// Fields that are SSH-specific (e.g. `username`, `auth_method`, `private_key_path`)
/// are still present but ignored when `protocol` is `Vnc`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshSession {
    /// Unique identifier (UUID v4).
    pub id: String,
    /// Human-readable label for this session.
    pub label: String,
    /// Connection protocol (defaults to SSH for backward compatibility).
    #[serde(default)]
    pub protocol: Protocol,
    /// Remote hostname or IP address.
    pub host: String,
    /// Port (default 22 for SSH, 5900 for VNC).
    pub port: u16,
    /// Username for authentication (SSH only).
    pub username: String,
    /// Authentication method (SSH only).
    pub auth_method: AuthMethod,
    /// Password — used for SSH password auth or VNC authentication.
    /// In a future iteration this should be moved to a secure credential store.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    /// Path to the private key file (SSH key auth only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub private_key_path: Option<String>,
    /// Optional notes / description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// Windows domain for RDP authentication (RDP only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
}
