use std::fmt;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex as StdMutex;

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
///
/// All file operations are serialised through an internal mutex to prevent
/// concurrent read-modify-write races (RES-7).
pub struct KnownHostsStore {
    path: PathBuf,
    /// Guards read-modify-write cycles against concurrent access.
    file_lock: StdMutex<()>,
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
            file_lock: StdMutex::new(()),
        })
    }

    /// Create a store backed by a specific file path (for testing).
    #[cfg(test)]
    pub fn with_path(path: PathBuf) -> Self {
        Self {
            path,
            file_lock: StdMutex::new(()),
        }
    }

    /// Check whether the given host key is known, unknown, or changed.
    pub fn check(&self, host: &str, port: u16, key: &PublicKey) -> HostKeyStatus {
        let algo = key_algorithm_name(key);
        let encoded = key.public_key_base64();
        let fingerprint = key.fingerprint();
        let entry_host = format_host(host, port);

        let _guard = self.file_lock.lock().unwrap_or_else(|e| e.into_inner());
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
    ///
    /// All inputs are sanitised to prevent newline/whitespace injection
    /// into the known_hosts file (SEC-2).
    pub fn accept(
        &self,
        host: &str,
        port: u16,
        key_data: &str,
        algorithm: &str,
    ) -> Result<(), std::io::Error> {
        // SEC-2: reject inputs containing whitespace or newlines that could
        // inject additional entries into the known_hosts file.
        if contains_whitespace_or_newline(host)
            || contains_whitespace_or_newline(key_data)
            || contains_whitespace_or_newline(algorithm)
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "host, key_data, and algorithm must not contain whitespace or newlines",
            ));
        }

        let entry_host = format_host(host, port);

        let _guard = self.file_lock.lock().unwrap_or_else(|e| e.into_inner());
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

/// Returns true if `s` contains any whitespace or newline characters.
fn contains_whitespace_or_newline(s: &str) -> bool {
    s.chars().any(|c| c.is_whitespace())
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

// ── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: create a KnownHostsStore backed by a temporary file.
    fn temp_store() -> (KnownHostsStore, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("known_hosts");
        let store = KnownHostsStore::with_path(path);
        (store, dir)
    }

    // ── SEC-2: Known-hosts injection ─────────────────────────────

    #[test]
    fn accept_rejects_newline_in_algorithm() {
        let (store, _dir) = temp_store();
        let result = store.accept(
            "example.com",
            22,
            "AAAAB3NzaC1yc2EAAAA",
            "ssh-rsa\n[evil.host]:22 ssh-ed25519 AAAA",
        );
        assert!(result.is_err(), "should reject algorithm with newline");
    }

    #[test]
    fn accept_rejects_newline_in_key_data() {
        let (store, _dir) = temp_store();
        let result = store.accept(
            "example.com",
            22,
            "AAAAB3Nz\n[evil]:22 ssh-rsa BBBB",
            "ssh-rsa",
        );
        assert!(result.is_err(), "should reject key_data with newline");
    }

    #[test]
    fn accept_rejects_whitespace_in_algorithm() {
        let (store, _dir) = temp_store();
        let result = store.accept("example.com", 22, "AAAA", "ssh rsa");
        assert!(result.is_err(), "should reject algorithm with space");
    }

    #[test]
    fn accept_rejects_whitespace_in_host() {
        let (store, _dir) = temp_store();
        let result = store.accept("evil host", 22, "AAAA", "ssh-rsa");
        assert!(result.is_err(), "should reject host with space");
    }

    #[test]
    fn accept_valid_entry_persists_correctly() {
        let (store, _dir) = temp_store();
        store
            .accept("example.com", 22, "AAAAB3NzaC1yc2EAAAA", "ssh-rsa")
            .expect("valid accept should succeed");

        let entries = store.load_entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].host, "[example.com]:22");
        assert_eq!(entries[0].algorithm, "ssh-rsa");
        assert_eq!(entries[0].key_data, "AAAAB3NzaC1yc2EAAAA");
    }

    #[test]
    fn accept_replaces_existing_entry_for_same_host_algo() {
        let (store, _dir) = temp_store();
        store
            .accept("example.com", 22, "OLD_KEY", "ssh-rsa")
            .unwrap();
        store
            .accept("example.com", 22, "NEW_KEY", "ssh-rsa")
            .unwrap();

        let entries = store.load_entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].key_data, "NEW_KEY");
    }

    #[test]
    fn injection_does_not_create_extra_entries() {
        let (store, _dir) = temp_store();
        // First, add a legitimate entry
        store.accept("legit.host", 22, "LEGIT_KEY", "ssh-rsa").unwrap();

        // Try to inject via algorithm field
        let result = store.accept(
            "target.host",
            22,
            "AAAA",
            "ssh-rsa\n[evil.host]:22 ssh-ed25519 INJECTED",
        );
        assert!(result.is_err());

        // Verify only the legitimate entry exists
        let entries = store.load_entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].host, "[legit.host]:22");
    }

    // ── RES-7: Concurrent access safety ──────────────────────────

    #[test]
    fn concurrent_accepts_do_not_lose_entries() {
        let (store, _dir) = temp_store();
        let store = std::sync::Arc::new(store);

        let mut handles = vec![];
        for i in 0..10 {
            let store = store.clone();
            let handle = std::thread::spawn(move || {
                store
                    .accept(
                        &format!("host-{}.example.com", i),
                        22,
                        &format!("KEY_{}", i),
                        "ssh-rsa",
                    )
                    .expect("accept should succeed");
            });
            handles.push(handle);
        }

        for h in handles {
            h.join().unwrap();
        }

        let entries = store.load_entries();
        assert_eq!(entries.len(), 10, "all 10 entries should be present");
    }

    // ── DC-1: Removed dead code _key_fingerprint ─────────────────
    // The unused function has been removed. No test needed.
}
