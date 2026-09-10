//! Per-run append-only JSONL transcripts. Design §10 / D7: high-volume run
//! events go to sequential files, never through SQLite.

use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

use crate::TranscriptLine;

/// File name for a run's transcript inside the transcripts directory.
pub fn transcript_path(dir: impl AsRef<Path>, run_id: &ruagent_core::RunId) -> PathBuf {
    dir.as_ref().join(format!("run-{run_id}.jsonl"))
}

/// Appender for one run's transcript.
pub struct TranscriptWriter {
    file: BufWriter<File>,
    seq: u64,
}

impl TranscriptWriter {
    pub fn create(path: impl AsRef<Path>) -> std::io::Result<Self> {
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        Ok(Self {
            file: BufWriter::new(file),
            seq: 0,
        })
    }

    /// Append one event, flushed immediately: `BufWriter::flush` is a
    /// plain write syscall (no fsync), so per-event flushing is cheap and
    /// keeps replay-from-file correct for live SSE subscribers.
    pub fn append(&mut self, event: &ruagent_core::RunEvent) -> std::io::Result<()> {
        let line = TranscriptLine {
            ts: chrono::Utc::now(),
            seq: self.seq,
            event: event.clone(),
        };
        serde_json::to_writer(&mut self.file, &line)?;
        self.file.write_all(b"\n")?;
        self.seq += 1;
        self.file.flush()?;
        Ok(())
    }

    pub fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }

    /// The sequence number the next appended event will receive.
    pub fn next_seq(&self) -> u64 {
        self.seq
    }
}

/// Read a transcript back. Malformed lines (e.g. a torn final line after a
/// crash) are skipped with a warning, not fatal — the transcript is
/// evidence, and partial evidence beats none.
pub fn read_transcript(path: impl AsRef<Path>) -> std::io::Result<Vec<TranscriptLine>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut out = Vec::new();
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str(&line) {
            Ok(parsed) => out.push(parsed),
            Err(e) => tracing::warn!("skipping malformed transcript line: {e}"),
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ruagent_core::{RunEvent, RunStatus};

    #[test]
    fn transcript_roundtrip() {
        let dir = std::env::temp_dir().join(format!("ruagent-tr-test-{}", std::process::id()));
        let run_id = ruagent_core::RunId::generate();
        let path = transcript_path(&dir, &run_id);
        {
            let mut w = TranscriptWriter::create(&path).unwrap();
            w.append(&RunEvent::StateChanged {
                status: RunStatus::Running,
            })
            .unwrap();
            w.append(&RunEvent::Error {
                message: "boom".into(),
            })
            .unwrap();
            w.flush().unwrap();
        }
        let lines = read_transcript(&path).unwrap();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].seq, 0);
        assert_eq!(lines[1].seq, 1);
        assert!(matches!(lines[1].event, RunEvent::Error { .. }));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn torn_or_malformed_lines_are_skipped() {
        let dir = std::env::temp_dir().join(format!("ruagent-tr-torn-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("torn.jsonl");
        std::fs::write(&path, "{\"seq\":0}\n{\"seq\":1,\"ts\":\"xx").unwrap();
        let lines = read_transcript(&path).unwrap();
        assert!(lines.is_empty(), "both lines are malformed/incomplete");
        std::fs::remove_dir_all(&dir).ok();
    }
}
