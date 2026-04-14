use serde::{Deserialize, Serialize};

/// Connection protocol for a session.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Protocol {
    /// SSH terminal session.
    Ssh,
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
/// are still present but may be ignored for non-SSH protocols.
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
    /// Port (default 22 for SSH, 3389 for RDP).
    pub port: u16,
    /// Username for authentication (SSH only).
    pub username: String,
    /// Authentication method (SSH only).
    pub auth_method: AuthMethod,
    /// Password — accepted from the frontend but **never persisted to disk**.
    ///
    /// The `skip_serializing` attribute ensures passwords are stripped when
    /// sessions are written to `sessions.json` (SEC-1).  The frontend must
    /// supply the password at connect-time (from user input or its own
    /// in-memory state).
    #[serde(default, skip_serializing)]
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

/// Errors that can occur when validating a session.
#[derive(Debug, thiserror::Error)]
pub enum ValidationError {
    #[error("Session ID must not be empty")]
    EmptyId,
    #[error("Host must not be empty")]
    EmptyHost,
    #[error("Host contains invalid characters")]
    InvalidHost,
    #[error("Port must be between 1 and 65535")]
    InvalidPort,
    #[error("Private key path contains path traversal")]
    PathTraversal,
}

/// Validate that an `SshSession` has reasonable field values (SEC-7).
///
/// This prevents accepting malformed sessions from import or user input.
pub fn validate_session(session: &SshSession) -> Result<(), ValidationError> {
    if session.id.is_empty() {
        return Err(ValidationError::EmptyId);
    }
    if session.host.is_empty() {
        return Err(ValidationError::EmptyHost);
    }
    if session.port == 0 {
        return Err(ValidationError::InvalidPort);
    }
    // Validate host: must be a valid IP or hostname characters
    if !is_valid_host(&session.host) {
        return Err(ValidationError::InvalidHost);
    }
    // Check private_key_path for path traversal
    if let Some(ref path) = session.private_key_path {
        if path.contains("..") {
            return Err(ValidationError::PathTraversal);
        }
    }
    Ok(())
}

/// Validate that a host string is a reasonable IP address or hostname.
///
/// Also used by SSH connect (SEC-4) for consistent validation across
/// all protocols.
pub fn is_valid_host(host: &str) -> bool {
    if host.is_empty() || host.len() > 253 {
        return false;
    }
    // Accept valid IP addresses
    if host.parse::<std::net::IpAddr>().is_ok() {
        return true;
    }
    // Hostname: alphanumeric, dots, hyphens, underscores
    host.chars()
        .all(|c| c.is_alphanumeric() || c == '.' || c == '-' || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_session(id: &str, host: &str, port: u16) -> SshSession {
        SshSession {
            id: id.to_string(),
            label: "Test".to_string(),
            protocol: Protocol::Ssh,
            host: host.to_string(),
            port,
            username: "user".to_string(),
            auth_method: AuthMethod::Password,
            password: Some("secret".to_string()),
            private_key_path: None,
            notes: None,
            domain: None,
        }
    }

    // ── SEC-7: Session validation ────────────────────────────────

    #[test]
    fn validate_rejects_empty_id() {
        let s = make_session("", "example.com", 22);
        assert!(matches!(validate_session(&s), Err(ValidationError::EmptyId)));
    }

    #[test]
    fn validate_rejects_empty_host() {
        let s = make_session("abc", "", 22);
        assert!(matches!(validate_session(&s), Err(ValidationError::EmptyHost)));
    }

    #[test]
    fn validate_rejects_port_zero() {
        let s = make_session("abc", "example.com", 0);
        assert!(matches!(validate_session(&s), Err(ValidationError::InvalidPort)));
    }

    #[test]
    fn validate_rejects_invalid_host_chars() {
        let s = make_session("abc", "host;rm -rf /", 22);
        assert!(matches!(validate_session(&s), Err(ValidationError::InvalidHost)));
    }

    #[test]
    fn validate_rejects_path_traversal_in_key_path() {
        let mut s = make_session("abc", "example.com", 22);
        s.private_key_path = Some("../../etc/shadow".to_string());
        assert!(matches!(validate_session(&s), Err(ValidationError::PathTraversal)));
    }

    #[test]
    fn validate_accepts_valid_session() {
        let s = make_session("abc-123", "example.com", 22);
        assert!(validate_session(&s).is_ok());
    }

    #[test]
    fn validate_accepts_ip_address_host() {
        let s = make_session("abc", "192.168.1.1", 22);
        assert!(validate_session(&s).is_ok());
    }

    #[test]
    fn validate_accepts_ipv6_host() {
        let s = make_session("abc", "::1", 22);
        assert!(validate_session(&s).is_ok());
    }

    // ── SEC-1: Password not persisted to disk ────────────────────

    #[test]
    fn password_is_stripped_on_serialization() {
        let s = make_session("abc", "example.com", 22);
        assert!(s.password.is_some(), "password should exist in memory");

        let json = serde_json::to_string(&s).expect("serialize");
        assert!(
            !json.contains("secret"),
            "serialized JSON must not contain the password value, got: {}",
            json
        );
    }

    #[test]
    fn password_is_accepted_on_deserialization() {
        let json = r#"{
            "id": "abc",
            "label": "Test",
            "host": "example.com",
            "port": 22,
            "username": "user",
            "auth_method": "password",
            "password": "secret123"
        }"#;
        let s: SshSession = serde_json::from_str(json).expect("deserialize");
        assert_eq!(s.password.as_deref(), Some("secret123"));
    }

    // ── SEC-4: Host validation function ──────────────────────────

    #[test]
    fn is_valid_host_rejects_empty() {
        assert!(!is_valid_host(""));
    }

    #[test]
    fn is_valid_host_rejects_shell_injection() {
        assert!(!is_valid_host("host;whoami"));
        assert!(!is_valid_host("host$(cmd)"));
        assert!(!is_valid_host("host\ninjection"));
    }

    #[test]
    fn is_valid_host_accepts_hostname() {
        assert!(is_valid_host("example.com"));
        assert!(is_valid_host("my-host.local"));
        assert!(is_valid_host("192.168.1.1"));
        assert!(is_valid_host("::1"));
    }
}
