use std::fmt;
use std::fs;
use std::path::PathBuf;

use russh_keys::key::PublicKey;
use russh_keys::PublicKeyBase64;

/// Result of checking a host key against the known hosts store.
#[derive(Debug)]
pub enum HostKeyStatus {
    /// Key matches a previously accepted entry.
    Known,
    /// No entry exists for this host+port combination.
    Unknown {
        fingerprint: String,
        key_data: String,
    },
    /// An entry exists but the key has changed (potential MITM).
    Changed {
        fingerprint: String,
        key_data: String,
    },
}

/// Manages known SSH host keys persisted to `%APPDATA%/RxTerm/known_hosts`.
///
/// File format (one entry per line):
/// ```text
/// [host]:port algorithm base64-key
/// ```
pub struct KnownHostsStore {
    path: PathBuf,
}

impl KnownHostsStore {
    /// Create a new store using the default data directory.
    pub fn new() -> Result<Self, std::io::Error> {
        let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
        let dir = base.join("RxTerm");
        if !dir.exists() {
            fs::create_dir_all(&dir)?;
        }
        Ok(Self {
            path: dir.join("known_hosts"),
        })
    }

    /// Check whether the given host key is known, unknown, or changed.
    pub fn check(&self, host: &str, port: u16, key: &PublicKey) -> HostKeyStatus {
        let algo = key_algorithm_name(key);
        let encoded = key.public_key_base64();
        let fingerprint = key.fingerprint();
        let entry_host = format_host(host, port);

        let entries = self.load_entries();
        for entry in &entries {
            if entry.host == entry_host && entry.algorithm == algo {
                if entry.key_data == encoded {
                    return HostKeyStatus::Known;
                }
                return HostKeyStatus::Changed {
                    fingerprint,
                    key_data: encoded,
                };
            }
        }

        HostKeyStatus::Unknown {
            fingerprint,
            key_data: encoded,
        }
    }

    /// Persist a host key as accepted.
    pub fn accept(
        &self,
        host: &str,
        port: u16,
        key_data: &str,
        algorithm: &str,
    ) -> Result<(), std::io::Error> {
        let entry_host = format_host(host, port);
        let mut entries = self.load_entries();

        // Replace existing entry for same host+algo, or append.
        if let Some(pos) = entries
            .iter()
            .position(|e| e.host == entry_host && e.algorithm == algorithm)
        {
            entries[pos].key_data = key_data.to_string();
        } else {
            entries.push(KnownHostEntry {
                host: entry_host,
                algorithm: algorithm.to_string(),
                key_data: key_data.to_string(),
            });
        }

        self.save_entries(&entries)
    }

    // ── Internal helpers ────────────────────────────────────────────

    fn load_entries(&self) -> Vec<KnownHostEntry> {
        let Ok(data) = fs::read_to_string(&self.path) else {
            return Vec::new();
        };
        data.lines().filter_map(KnownHostEntry::parse).collect()
    }

    fn save_entries(&self, entries: &[KnownHostEntry]) -> Result<(), std::io::Error> {
        let content: String = entries.iter().map(|e| format!("{}\n", e)).collect();
        fs::write(&self.path, content)
    }
}

// ── Entry type ────────────────────────────────────────────────────

struct KnownHostEntry {
    host: String,
    algorithm: String,
    key_data: String,
}

impl KnownHostEntry {
    fn parse(line: &str) -> Option<Self> {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() != 3 {
            return None;
        }
        Some(Self {
            host: parts[0].to_string(),
            algorithm: parts[1].to_string(),
            key_data: parts[2].to_string(),
        })
    }
}

impl fmt::Display for KnownHostEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {} {}", self.host, self.algorithm, self.key_data)
    }
}

// ── Public key helpers ────────────────────────────────────────────

/// Returns the SSH algorithm name for a public key.
fn key_algorithm_name(key: &PublicKey) -> String {
    key.name().to_string()
}

/// Produce a fingerprint string for display to the user.
/// Uses russh-keys' built-in SHA-256 fingerprint.
fn _key_fingerprint(key: &PublicKey) -> String {
    let algo = key.name();
    let fp = key.fingerprint();
    format!("{} SHA256:{}", algo, fp)
}

/// Format the canonical host key for storage: `[host]:port`
fn format_host(host: &str, port: u16) -> String {
    format!("[{}]:{}", host, port)
}

/// Extract algorithm name from a public key (public API for commands).
pub fn key_algorithm(key: &PublicKey) -> String {
    key.name().to_string()
}

/// Encode a public key to base64 for transport/storage (public API for commands).
pub fn key_to_base64(key: &PublicKey) -> String {
    key.public_key_base64()
}
