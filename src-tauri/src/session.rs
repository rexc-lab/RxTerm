use serde::{Deserialize, Serialize};

/// Authentication method for an SSH session.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AuthMethod {
    /// Authenticate with a password.
    Password,
    /// Authenticate with an SSH private key (optionally passphrase-protected).
    Key,
}

/// Represents all user-configurable fields for a single SSH session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshSession {
    /// Unique identifier (UUID v4).
    pub id: String,
    /// Human-readable label for this session.
    pub label: String,
    /// Remote hostname or IP address.
    pub host: String,
    /// SSH port (default 22).
    pub port: u16,
    /// Username for authentication.
    pub username: String,
    /// Authentication method.
    pub auth_method: AuthMethod,
    /// Password (stored only when auth_method == Password).
    /// In a future iteration this should be moved to a secure credential store.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    /// Path to the private key file (used when auth_method == Key).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub private_key_path: Option<String>,
    /// Optional notes / description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}
