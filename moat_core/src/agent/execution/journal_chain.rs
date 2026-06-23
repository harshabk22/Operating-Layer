use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum JournalOperation {
    Allow { tool_name: String, args: String },
    Deny { tool_name: String, reason: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalEntry {
    pub seq: u64,
    pub timestamp_ns: u64,
    pub agent_id: String,
    pub operation: JournalOperation,
    pub prev_hash: [u8; 32],
    pub entry_hash: [u8; 32],
}

#[derive(Debug, Error)]
pub enum ChainError {
    #[error(
        "chain integrity broken at seq {seq}: expected hash {expected:02x?}, got {actual:02x?}"
    )]
    HashMismatch {
        seq: u64,
        expected: [u8; 32],
        actual: [u8; 32],
    },
    #[error("missing entry at seq {0}")]
    MissingEntry(u64),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

pub fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    let mut k_padded = [0u8; 64];
    if key.len() > 64 {
        let hash = Sha256::digest(key);
        k_padded[..32].copy_from_slice(&hash);
    } else {
        k_padded[..key.len()].copy_from_slice(key);
    }

    let mut ipad = [0x36u8; 64];
    let mut opad = [0x5cu8; 64];
    for i in 0..64 {
        ipad[i] ^= k_padded[i];
        opad[i] ^= k_padded[i];
    }

    let mut inner_hasher = Sha256::new();
    inner_hasher.update(ipad);
    inner_hasher.update(message);
    let inner_hash = inner_hasher.finalize();

    let mut outer_hasher = Sha256::new();
    outer_hasher.update(opad);
    outer_hasher.update(inner_hash);
    outer_hasher.finalize().into()
}

fn get_journal_signing_key(session_id: &str) -> Vec<u8> {
    let path = dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".ageex")
        .join("journal.key");
    let mut base_key = if path.exists() {
        std::fs::read(&path).unwrap_or_else(|_| b"MoatAgeexDefaultJournalSigningKeySalt".to_vec())
    } else {
        let mut key = Vec::new();
        key.extend_from_slice(uuid::Uuid::new_v4().as_bytes());
        key.extend_from_slice(uuid::Uuid::new_v4().as_bytes());
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&path, &key);
        key
    };
    base_key.extend_from_slice(session_id.as_bytes());
    base_key
}
impl JournalEntry {
    fn compute_hash(
        seq: u64,
        timestamp_ns: u64,
        agent_id: &str,
        operation: &JournalOperation,
        prev_hash: &[u8; 32],
    ) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(seq.to_le_bytes());
        hasher.update(timestamp_ns.to_le_bytes());
        hasher.update(agent_id.as_bytes());
        let op_bytes = bincode::serialize(operation).unwrap_or_default();
        hasher.update(&op_bytes);
        hasher.update(prev_hash);
        hasher.finalize().into()
    }

    pub fn new(
        seq: u64,
        timestamp_ns: u64,
        agent_id: String,
        operation: JournalOperation,
        prev_hash: [u8; 32],
    ) -> Self {
        let entry_hash = Self::compute_hash(seq, timestamp_ns, &agent_id, &operation, &prev_hash);
        Self {
            seq,
            timestamp_ns,
            agent_id,
            operation,
            prev_hash,
            entry_hash,
        }
    }
}

pub struct ChainJournal {
    file: File,
    next_seq: u64,
    last_hash: [u8; 32],
    session_id: String,
}

impl ChainJournal {
    /// Open an existing journal or create a new one at `path`.
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, ChainError> {
        Self::open_scoped(path, "system-session")
    }

    pub fn open_scoped(path: impl Into<PathBuf>, session_id: &str) -> Result<Self, ChainError> {
        let path = path.into();
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .read(true)
            .open(&path)?;

        let mut buf = Vec::new();
        file.read_to_end(&mut buf)?;

        let (next_seq, last_hash) = if buf.is_empty() {
            (0, [0u8; 32])
        } else {
            let data_len = if buf.len() >= 32 {
                buf.len() - 32
            } else {
                buf.len()
            };
            Self::scan_buf(&buf[..data_len])?
        };

        Ok(Self {
            file,
            next_seq,
            last_hash,
            session_id: session_id.to_string(),
        })
    }

    fn scan_buf(buf: &[u8]) -> Result<(u64, [u8; 32]), ChainError> {
        if buf.is_empty() {
            return Ok((0, [0u8; 32]));
        }

        let mut cursor = 0usize;
        let mut seq = 0u64;
        let mut last_hash = [0u8; 32];
        while cursor < buf.len() {
            match bincode::deserialize::<JournalEntry>(&buf[cursor..]) {
                Ok(entry) => {
                    if cursor > 0 {
                        let expected = JournalEntry::compute_hash(
                            entry.seq,
                            entry.timestamp_ns,
                            &entry.agent_id,
                            &entry.operation,
                            &entry.prev_hash,
                        );
                        if entry.entry_hash != expected {
                            return Err(ChainError::HashMismatch {
                                seq: entry.seq,
                                expected,
                                actual: entry.entry_hash,
                            });
                        }
                    }
                    seq = entry.seq + 1;
                    last_hash = entry.entry_hash;
                    let entry_bytes = bincode::serialize(&entry).unwrap_or_default();
                    cursor += entry_bytes.len();
                }
                Err(_) => break,
            }
        }
        Ok((seq, last_hash))
    }

    pub fn append(
        &mut self,
        agent_id: String,
        operation: JournalOperation,
    ) -> Result<u64, ChainError> {
        let seq = self.next_seq;
        let timestamp_ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;

        let entry = JournalEntry::new(seq, timestamp_ns, agent_id, operation, self.last_hash);

        let bytes = bincode::serialize(&entry).map_err(|e| std::io::Error::other(e.to_string()))?;

        use std::io::Seek;
        self.file.seek(std::io::SeekFrom::Start(0))?;
        let mut buf = Vec::new();
        self.file.read_to_end(&mut buf)?;

        let data_len = if buf.len() >= 32 {
            buf.len() - 32
        } else {
            buf.len()
        };
        self.file.seek(std::io::SeekFrom::Start(data_len as u64))?;

        self.file.write_all(&bytes)?;

        self.file.seek(std::io::SeekFrom::Start(0))?;
        let mut new_data = Vec::new();
        let final_data_len = data_len + bytes.len();
        new_data.resize(final_data_len, 0);
        self.file.read_exact(&mut new_data)?;

        let key = get_journal_signing_key(&self.session_id);
        let sig = hmac_sha256(&key, &new_data);
        self.file
            .seek(std::io::SeekFrom::Start(final_data_len as u64))?;
        self.file.write_all(&sig)?;

        self.file.set_len((final_data_len + 32) as u64)?;
        self.file.flush()?;

        self.last_hash = entry.entry_hash;
        self.next_seq = seq + 1;

        Ok(seq)
    }
}

pub fn verify_chain(path: impl Into<PathBuf>) -> Result<u64, ChainError> {
    verify_chain_scoped(path, "system-session")
}

pub fn verify_chain_scoped(path: impl Into<PathBuf>, session_id: &str) -> Result<u64, ChainError> {
    let path = path.into();
    let mut file = File::open(&path)?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf)?;

    if buf.is_empty() {
        return Ok(0);
    }

    if buf.len() < 32 {
        return Err(ChainError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Journal file size is too small to contain signature",
        )));
    }

    let data_len = buf.len() - 32;
    let (data, signature) = buf.split_at(data_len);

    let key1 = get_journal_signing_key(session_id);
    let computed_sig1 = hmac_sha256(&key1, data);
    let mut verified = computed_sig1 == signature;

    if !verified && session_id != "system-session" {
        let key2 = get_journal_signing_key("system-session");
        let computed_sig2 = hmac_sha256(&key2, data);
        verified = computed_sig2 == signature;
    }

    if !verified {
        return Err(ChainError::Io(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "HMAC signature verification failed - file tampered",
        )));
    }

    let mut cursor = 0usize;
    let mut prev_hash = [0u8; 32];
    let mut count = 0u64;

    while cursor < data.len() {
        let entry: JournalEntry =
            bincode::deserialize(&data[cursor..]).map_err(|_| ChainError::MissingEntry(count))?;

        if entry.seq != count {
            return Err(ChainError::MissingEntry(count));
        }
        if entry.prev_hash != prev_hash {
            return Err(ChainError::HashMismatch {
                seq: entry.seq,
                expected: prev_hash,
                actual: entry.prev_hash,
            });
        }

        let expected = JournalEntry::compute_hash(
            entry.seq,
            entry.timestamp_ns,
            &entry.agent_id,
            &entry.operation,
            &entry.prev_hash,
        );
        if entry.entry_hash != expected {
            return Err(ChainError::HashMismatch {
                seq: entry.seq,
                expected,
                actual: entry.entry_hash,
            });
        }

        let entry_bytes = bincode::serialize(&entry).unwrap_or_default();
        cursor += entry_bytes.len();
        prev_hash = entry.entry_hash;
        count += 1;
    }

    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        let uuid = uuid::Uuid::new_v4();
        std::env::temp_dir().join(format!("chain_journal_test_{}_{}.bin", name, uuid))
    }

    #[test]
    fn test_genesis_entry_has_zero_prev_hash() {
        let path = temp_path("genesis");
        let mut journal = ChainJournal::open(&path).expect("open");
        let seq = journal
            .append(
                "agent1".into(),
                JournalOperation::Allow {
                    tool_name: "fs_read".into(),
                    args: r#"{"path":"/tmp/a.txt"}"#.into(),
                },
            )
            .expect("append");
        assert_eq!(seq, 0);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_chain_verification_passes_on_valid_chain() {
        let path = temp_path("valid");
        let mut journal = ChainJournal::open(&path).expect("open");
        journal
            .append(
                "agent1".into(),
                JournalOperation::Allow {
                    tool_name: "fs_read".into(),
                    args: r#"{"path":"/tmp/a.txt"}"#.into(),
                },
            )
            .expect("append 1");
        journal
            .append(
                "agent1".into(),
                JournalOperation::Deny {
                    tool_name: "fs_delete".into(),
                    reason: "consent denied".into(),
                },
            )
            .expect("append 2");
        journal
            .append(
                "business_runtime".into(),
                JournalOperation::Allow {
                    tool_name: "process_spawn".into(),
                    args: r#"{"command":"ls"}"#.into(),
                },
            )
            .expect("append 3");
        drop(journal);

        let count = verify_chain(&path).expect("verify");
        assert_eq!(count, 3);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_verify_detects_tampered_entry() {
        let path = temp_path("tampered");
        let mut journal = ChainJournal::open(&path).expect("open");
        journal
            .append(
                "agent1".into(),
                JournalOperation::Allow {
                    tool_name: "fs_read".into(),
                    args: r#"{"path":"/tmp/a.txt"}"#.into(),
                },
            )
            .expect("append 1");
        journal
            .append(
                "agent1".into(),
                JournalOperation::Allow {
                    tool_name: "fs_write".into(),
                    args: r#"{"path":"/tmp/b.txt"}"#.into(),
                },
            )
            .expect("append 2");
        drop(journal);

        // Tamper with a byte in the middle of the file
        let mut raw = std::fs::read(&path).expect("read");
        if raw.len() > 10 {
            raw[10] ^= 0xFF; // flip bits
        }
        std::fs::write(&path, &raw).expect("write tampered");

        let result = verify_chain(&path);
        assert!(result.is_err(), "Tampered chain must fail verification");
        std::fs::remove_file(&path).ok();
    }
}
