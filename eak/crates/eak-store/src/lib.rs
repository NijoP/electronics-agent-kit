//! Persistence adapters (outer ring) implementing the event-log boundary.
//!
//! [`FileEventLog`] realizes event-sourcing (ADR-0004) as the simplest substrate that
//! demonstrates the Phase-1 replay exit criterion: an append-only JSON-lines file (one
//! [`EventRecord`] per line) plus an in-memory cache that is its fold. The state
//! projection lives in the kernel; this crate only persists the history.

use eak_ports::{Event, EventLog, EventRecord, Seq, StoreError, Timestamp};
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

/// Append-only event log backed by a JSON-lines file.
pub struct FileEventLog {
    path: PathBuf,
    records: Vec<EventRecord>,
}

impl FileEventLog {
    /// Open (or create on first append) the log at `path`, loading any existing history.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let path = path.as_ref().to_path_buf();
        let mut records = Vec::new();
        if path.exists() {
            let file = std::fs::File::open(&path).map_err(|e| StoreError::Io(e.to_string()))?;
            for line in BufReader::new(file).lines() {
                let line = line.map_err(|e| StoreError::Io(e.to_string()))?;
                if line.trim().is_empty() {
                    continue;
                }
                let rec: EventRecord = serde_json::from_str(&line)
                    .map_err(|e| StoreError::Serialization(e.to_string()))?;
                records.push(rec);
            }
        }
        Ok(Self { path, records })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl EventLog for FileEventLog {
    fn append(&mut self, events: &[(Timestamp, Event)]) -> Result<Vec<Seq>, StoreError> {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|e| StoreError::Io(e.to_string()))?;
        let mut seqs = Vec::with_capacity(events.len());
        let start = self.records.len() as u64;
        for (i, (ts, ev)) in events.iter().enumerate() {
            let seq = start + i as u64;
            let rec = EventRecord {
                seq,
                timestamp: *ts,
                event: ev.clone(),
            };
            let line = serde_json::to_string(&rec)
                .map_err(|e| StoreError::Serialization(e.to_string()))?;
            writeln!(file, "{line}").map_err(|e| StoreError::Io(e.to_string()))?;
            seqs.push(seq);
            self.records.push(rec);
        }
        file.flush().map_err(|e| StoreError::Io(e.to_string()))?; // persist before observe
        Ok(seqs)
    }

    fn read_all(&self) -> Result<Vec<EventRecord>, StoreError> {
        Ok(self.records.clone())
    }

    fn next_seq(&self) -> Seq {
        self.records.len() as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eak_ports::{Event, Timestamp};

    #[test]
    fn append_persists_and_reloads() {
        let mut path = std::env::temp_dir();
        path.push(format!("eak-store-test-{}.jsonl", std::process::id()));
        let _ = std::fs::remove_file(&path);

        {
            let mut log = FileEventLog::open(&path).unwrap();
            let seqs = log
                .append(&[(
                    Timestamp(1),
                    Event::PhaseCompleted {
                        phase: "RequirementPlanning".into(),
                        outcome: "success".into(),
                    },
                )])
                .unwrap();
            assert_eq!(seqs, vec![0]);
        }

        let reloaded = FileEventLog::open(&path).unwrap();
        assert_eq!(reloaded.read_all().unwrap().len(), 1);
        assert_eq!(reloaded.next_seq(), 1);

        let _ = std::fs::remove_file(&path);
    }
}
