//! Episodic memory — NDJSON append-only log + FTS5 dual-write index.
//!
//! Daily log files at `~/.bubbaloop/memory/daily_logs_YYYY-MM-DD.jsonl`.
//! Each append dual-writes to both NDJSON (source of truth) and an FTS5
//! virtual table (query index) in SQLite.

use chrono::SecondsFormat;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

/// A single episodic log entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    /// ISO 8601 timestamp.
    pub timestamp: String,
    /// Role: "user", "assistant", "tool", "system".
    pub role: String,
    /// Content text.
    pub content: String,
    /// Optional job ID linking to the semantic jobs table.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
    /// Whether this is a pre-compaction flush entry.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flush: Option<bool>,
}

/// Episodic log with NDJSON files + FTS5 index.
pub struct EpisodicLog {
    /// Directory for NDJSON files.
    log_dir: PathBuf,
    /// SQLite connection for FTS5 index.
    conn: Connection,
}

impl EpisodicLog {
    /// Create a new episodic log.
    ///
    /// - `log_dir`: directory for daily NDJSON files
    /// - `db_path`: SQLite database path for FTS5 index
    pub fn new(log_dir: &Path, db_path: &Path) -> super::Result<Self> {
        std::fs::create_dir_all(log_dir)?;

        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(db_path)?;

        // WAL mode + busy timeout
        conn.query_row("PRAGMA journal_mode=WAL", [], |_| Ok(()))?;
        conn.query_row("PRAGMA busy_timeout=5000", [], |_| Ok(()))?;

        // Create FTS5 virtual table (use query path for bundled-full compat)
        conn.prepare(
            "CREATE VIRTUAL TABLE IF NOT EXISTS fts_episodic USING fts5(\
                content,\
                id UNINDEXED,\
                role UNINDEXED,\
                timestamp UNINDEXED,\
                job_id UNINDEXED\
            )",
        )?
        .query([])?
        .next()?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if db_path.exists() {
                let perms = std::fs::Permissions::from_mode(0o600);
                std::fs::set_permissions(db_path, perms)?;
            }
        }

        Ok(Self {
            log_dir: log_dir.to_path_buf(),
            conn,
        })
    }

    /// Append an entry — dual-writes to NDJSON file + FTS5 index.
    pub fn append(&self, entry: &LogEntry) -> super::Result<()> {
        // 1. Write to NDJSON file
        let date = &entry.timestamp[..10]; // YYYY-MM-DD
        let filename = format!("daily_logs_{}.jsonl", date);
        let path = self.log_dir.join(filename);

        let mut file = OpenOptions::new().create(true).append(true).open(&path)?;

        let json_line = serde_json::to_string(entry)?;
        writeln!(file, "{}", json_line)?;

        // 2. Write to FTS5 index
        let id = uuid::Uuid::new_v4().to_string();
        self.conn.execute(
            "INSERT INTO fts_episodic (content, id, role, timestamp, job_id) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                entry.content,
                id,
                entry.role,
                entry.timestamp,
                entry.job_id.as_deref().unwrap_or(""),
            ],
        )?;

        Ok(())
    }

    /// BM25 full-text search over all episodic logs.
    pub fn search(&self, query: &str, limit: usize) -> super::Result<Vec<LogEntry>> {
        if query.is_empty() {
            return Ok(Vec::new());
        }
        let sanitized = sanitize_fts5_query(query);
        if sanitized.is_empty() {
            return Ok(Vec::new());
        }

        let mut stmt = self.conn.prepare(
            "SELECT content, role, timestamp, job_id \
             FROM fts_episodic \
             WHERE fts_episodic MATCH ?1 \
             ORDER BY rank \
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![sanitized, limit as i64], |row| {
            let job_id: String = row.get(3)?;
            Ok(LogEntry {
                timestamp: row.get(2)?,
                role: row.get(1)?,
                content: row.get(0)?,
                job_id: if job_id.is_empty() {
                    None
                } else {
                    Some(job_id)
                },
                flush: None,
            })
        })?;

        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(super::MemoryError::from)
    }

    /// BM25 search with optional temporal decay.
    ///
    /// When `decay_half_life_days` is 0, delegates to `search()` (no decay).
    /// When > 0, over-fetches 3x, applies exponential decay based on entry age,
    /// re-sorts, and truncates to `limit`.
    ///
    /// Decay formula: `effective_rank = rank * e^(-age_days * ln2 / half_life_days)`
    /// Since FTS5 rank is negative (more negative = better), decay makes old entries
    /// less negative (worse), naturally demoting them.
    pub fn search_with_decay(
        &self,
        query: &str,
        limit: usize,
        decay_half_life_days: u32,
    ) -> super::Result<Vec<LogEntry>> {
        if decay_half_life_days == 0 {
            return self.search(query, limit);
        }
        if query.is_empty() {
            return Ok(Vec::new());
        }
        let sanitized = sanitize_fts5_query(query);
        if sanitized.is_empty() {
            return Ok(Vec::new());
        }

        // Over-fetch 3x to have enough candidates after re-ranking
        let fetch_limit = limit * 3;

        let mut stmt = self.conn.prepare(
            "SELECT content, role, timestamp, job_id, rank \
             FROM fts_episodic \
             WHERE fts_episodic MATCH ?1 \
             ORDER BY rank \
             LIMIT ?2",
        )?;

        let now = chrono::Utc::now();
        let half_life = f64::from(decay_half_life_days);
        let ln2 = std::f64::consts::LN_2;

        let rows = stmt.query_map(params![sanitized, fetch_limit as i64], |row| {
            let content: String = row.get(0)?;
            let role: String = row.get(1)?;
            let timestamp: String = row.get(2)?;
            let job_id: String = row.get(3)?;
            let rank: f64 = row.get(4)?;
            Ok((content, role, timestamp, job_id, rank))
        })?;

        let mut scored: Vec<(LogEntry, f64)> = Vec::new();
        for row in rows {
            let (content, role, timestamp, job_id, rank) = row?;

            // Calculate age in days
            let age_days = chrono::DateTime::parse_from_rfc3339(&timestamp)
                .map(|dt| (now - dt.with_timezone(&chrono::Utc)).num_seconds() as f64 / 86400.0)
                .unwrap_or(0.0)
                .max(0.0);

            // Apply decay: rank * e^(-age * ln2 / half_life)
            let decay_factor = (-age_days * ln2 / half_life).exp();
            let effective_rank = rank * decay_factor;

            let entry = LogEntry {
                timestamp,
                role,
                content,
                job_id: if job_id.is_empty() {
                    None
                } else {
                    Some(job_id)
                },
                flush: None,
            };
            scored.push((entry, effective_rank));
        }

        // Sort by effective rank (more negative = better)
        scored.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(limit);

        Ok(scored.into_iter().map(|(entry, _)| entry).collect())
    }

    /// Create a log entry with the current timestamp.
    pub fn make_entry(role: &str, content: &str, job_id: Option<&str>) -> LogEntry {
        LogEntry {
            timestamp: super::now_rfc3339(),
            role: role.to_string(),
            content: content.to_string(),
            job_id: job_id.map(|s| s.to_string()),
            flush: None,
        }
    }

    /// Forget matching entries: remove from FTS5 index, write audit trail.
    ///
    /// - Finds entries matching `query` via FTS5 MATCH
    /// - Writes them to `{log_dir}/.deleted/forgotten_YYYY-MM-DD_HH-MM-SS.jsonl` (audit)
    /// - Hard-deletes from FTS5 index (derived data, safe to remove)
    /// - NDJSON source files are NOT modified (append-only principle)
    ///
    /// Returns the number of forgotten entries.
    pub fn forget(&self, query: &str, reason: &str) -> super::Result<usize> {
        if query.is_empty() {
            return Ok(0);
        }
        let sanitized = sanitize_fts5_query(query);
        if sanitized.is_empty() {
            return Ok(0);
        }

        // Find matching entries
        let mut stmt = self.conn.prepare(
            "SELECT content, role, timestamp, job_id, rowid \
             FROM fts_episodic \
             WHERE fts_episodic MATCH ?1",
        )?;
        let rows = stmt.query_map(params![sanitized], |row| {
            let content: String = row.get(0)?;
            let role: String = row.get(1)?;
            let timestamp: String = row.get(2)?;
            let job_id: String = row.get(3)?;
            let rowid: i64 = row.get(4)?;
            Ok((content, role, timestamp, job_id, rowid))
        })?;

        let matches: Vec<_> = rows.collect::<std::result::Result<Vec<_>, _>>()?;
        if matches.is_empty() {
            return Ok(0);
        }

        // Write audit trail
        let deleted_dir = self.log_dir.join(".deleted");
        std::fs::create_dir_all(&deleted_dir)?;

        let now = chrono::Utc::now();
        let audit_filename = format!("forgotten_{}.jsonl", now.format("%Y-%m-%d_%H-%M-%S"));
        let audit_path = deleted_dir.join(audit_filename);

        let mut audit_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&audit_path)?;

        let mut rowids = Vec::with_capacity(matches.len());
        for (content, role, timestamp, job_id, rowid) in &matches {
            let audit_entry = serde_json::json!({
                "forgotten_at": now.to_rfc3339_opts(SecondsFormat::Secs, true),
                "reason": reason,
                "original": {
                    "timestamp": timestamp,
                    "role": role,
                    "content": content,
                    "job_id": if job_id.is_empty() { None } else { Some(job_id) },
                }
            });
            writeln!(audit_file, "{}", serde_json::to_string(&audit_entry)?)?;
            rowids.push(*rowid);
        }

        // Hard-delete from FTS5
        for rowid in &rowids {
            self.conn
                .execute("DELETE FROM fts_episodic WHERE rowid = ?1", params![rowid])?;
        }

        let count = rowids.len();
        log::info!(
            "[Memory] Forgot {} entries matching '{}': {}",
            count,
            query,
            reason
        );

        Ok(count)
    }

    /// Prune old log files and their FTS5 entries.
    ///
    /// Deletes daily NDJSON files older than `retention_days` and removes
    /// the corresponding entries from the FTS5 index.
    /// Returns the number of pruned files. If `retention_days` is 0, keeps everything.
    pub fn prune_old_logs(&self, retention_days: u32) -> super::Result<usize> {
        if retention_days == 0 {
            return Ok(0);
        }

        let cutoff = chrono::Utc::now() - chrono::Duration::days(i64::from(retention_days));
        let cutoff_date = cutoff.format("%Y-%m-%d").to_string();

        let mut pruned = 0;

        let entries = match std::fs::read_dir(&self.log_dir) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
            Err(e) => return Err(e.into()),
        };

        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();

            // Match pattern: daily_logs_YYYY-MM-DD.jsonl
            if let Some(date) = name_str
                .strip_prefix("daily_logs_")
                .and_then(|s| s.strip_suffix(".jsonl"))
            {
                if date < cutoff_date.as_str() {
                    // Read file before deleting to extract content for FTS5 cleanup
                    let file_content = std::fs::read_to_string(entry.path()).ok();

                    // Delete NDJSON file
                    if let Err(e) = std::fs::remove_file(entry.path()) {
                        log::warn!("Failed to remove old log {}: {}", name_str, e);
                        continue;
                    }

                    // Delete corresponding FTS5 entries by matching content from the file.
                    // Uses AND matching (implicit in FTS5) to avoid collateral deletion.
                    if let Some(contents) = file_content {
                        for line in contents.lines() {
                            if let Ok(log_entry) = serde_json::from_str::<LogEntry>(line) {
                                let query = fts5_and_query(&log_entry.content);
                                if query.is_empty() {
                                    continue;
                                }
                                if let Ok(mut stmt) = self.conn.prepare(
                                    "SELECT rowid FROM fts_episodic WHERE fts_episodic MATCH ?1",
                                ) {
                                    let rowids: Vec<i64> = stmt
                                        .query_map(params![query], |row| row.get(0))
                                        .into_iter()
                                        .flatten()
                                        .flatten()
                                        .collect();
                                    for rowid in rowids {
                                        let _ = self.conn.execute(
                                            "DELETE FROM fts_episodic WHERE rowid = ?1",
                                            params![rowid],
                                        );
                                    }
                                }
                            }
                        }
                    }

                    pruned += 1;
                    log::info!("Pruned old episodic log: {}", name_str);
                }
            }
        }

        Ok(pruned)
    }

    /// Retrieve the most recent plan entry from episodic memory.
    ///
    /// Plans are stored with `role = "plan"` by the agent turn loop when the model
    /// outputs both text (reasoning) and tool calls (execution) in the same response.
    pub fn latest_plan(&self) -> super::Result<Option<LogEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT content, id, role, timestamp, job_id \
             FROM fts_episodic \
             WHERE role = 'plan' \
             ORDER BY timestamp DESC \
             LIMIT 1",
        )?;
        let mut rows = stmt.query_map([], |row| {
            let job_id: String = row.get(4)?;
            Ok(LogEntry {
                content: row.get(0)?,
                role: row.get(2)?,
                timestamp: row.get(3)?,
                job_id: if job_id.is_empty() {
                    None
                } else {
                    Some(job_id)
                },
                flush: None,
            })
        })?;
        match rows.next() {
            Some(Ok(entry)) => Ok(Some(entry)),
            Some(Err(e)) => Err(e.into()),
            None => Ok(None),
        }
    }

    /// Retrieve the most recent flush entry from episodic memory.
    ///
    /// Flush entries are written before context compaction to persist key state.
    /// After compaction, the agent can recover this context to maintain continuity.
    /// Flush entries are identified by the `CONTEXT_FLUSH:` prefix in their content.
    pub fn latest_flush(&self) -> super::Result<Option<LogEntry>> {
        // Search FTS5 for the flush marker token, then filter by prefix.
        let mut stmt = self.conn.prepare(
            "SELECT content, role, timestamp, job_id \
             FROM fts_episodic \
             WHERE fts_episodic MATCH '\"CONTEXT_FLUSH\"' \
             ORDER BY rank \
             LIMIT 5",
        )?;
        let rows = stmt.query_map([], |row| {
            let job_id: String = row.get(3)?;
            Ok(LogEntry {
                content: row.get(0)?,
                role: row.get(1)?,
                timestamp: row.get(2)?,
                job_id: if job_id.is_empty() {
                    None
                } else {
                    Some(job_id)
                },
                flush: Some(true),
            })
        })?;

        // Return the most recent flush entry (by timestamp, descending).
        let mut best: Option<LogEntry> = None;
        for row in rows {
            let entry = row?;
            if !entry.content.starts_with(FLUSH_PREFIX) {
                continue;
            }
            if best
                .as_ref()
                .map_or(true, |b| entry.timestamp > b.timestamp)
            {
                best = Some(entry);
            }
        }
        Ok(best)
    }

    /// Create a flush entry (for pre-compaction memory persistence).
    ///
    /// The content is prefixed with `CONTEXT_FLUSH:` so it can be identified
    /// and retrieved by `latest_flush()` via FTS5 search.
    pub fn make_flush_entry(content: &str, job_id: Option<&str>) -> LogEntry {
        LogEntry {
            timestamp: super::now_rfc3339(),
            role: "system".to_string(),
            content: format!("{}{}", FLUSH_PREFIX, content),
            job_id: job_id.map(|s| s.to_string()),
            flush: Some(true),
        }
    }

    /// Strip the flush prefix from a flush entry's content.
    pub fn strip_flush_prefix(content: &str) -> &str {
        content
            .strip_prefix(FLUSH_PREFIX)
            .unwrap_or(content)
    }
}

/// Prefix used to identify flush entries in FTS5.
const FLUSH_PREFIX: &str = "CONTEXT_FLUSH: ";

/// Build an FTS5 MATCH expression by quoting each word and joining with `separator`.
///
/// Takes up to 30 words, escapes double-quotes, and wraps each in quotes.
/// Used with "AND" for precise pruning matches and "OR" for search queries.
fn build_fts5_query(text: &str, separator: &str) -> String {
    text.split_whitespace()
        .take(30)
        .map(|word| {
            let escaped = word.replace('"', "\"\"");
            format!("\"{}\"", escaped)
        })
        .collect::<Vec<_>>()
        .join(separator)
}

/// Build an AND query for FTS5 — all words must be present.
fn fts5_and_query(text: &str) -> String {
    build_fts5_query(text, " AND ")
}

/// Sanitize user input for FTS5 MATCH queries (OR semantics).
fn sanitize_fts5_query(query: &str) -> String {
    build_fts5_query(query, " OR ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_episodic() -> (EpisodicLog, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let log_dir = dir.path().join("logs");
        let db_path = dir.path().join("test.db");
        let log = EpisodicLog::new(&log_dir, &db_path).unwrap();
        (log, dir)
    }

    #[test]
    fn append_creates_ndjson_file() {
        let (log, dir) = test_episodic();
        let entry = LogEntry {
            timestamp: "2026-03-03T10:00:00Z".to_string(),
            role: "user".to_string(),
            content: "hello world".to_string(),
            job_id: None,
            flush: None,
        };
        log.append(&entry).unwrap();

        let ndjson_path = dir.path().join("logs/daily_logs_2026-03-03.jsonl");
        assert!(ndjson_path.exists());
        let content = std::fs::read_to_string(&ndjson_path).unwrap();
        assert!(content.contains("hello world"));
    }

    #[test]
    fn append_multiple_entries() {
        let (log, dir) = test_episodic();
        for i in 0..5 {
            let entry = LogEntry {
                timestamp: "2026-03-03T10:00:00Z".to_string(),
                role: "user".to_string(),
                content: format!("message {}", i),
                job_id: Some("job-1".to_string()),
                flush: None,
            };
            log.append(&entry).unwrap();
        }

        let ndjson_path = dir.path().join("logs/daily_logs_2026-03-03.jsonl");
        let content = std::fs::read_to_string(&ndjson_path).unwrap();
        let lines: Vec<_> = content.lines().collect();
        assert_eq!(lines.len(), 5);
    }

    #[test]
    fn daily_rotation() {
        let (log, dir) = test_episodic();
        let entry1 = LogEntry {
            timestamp: "2026-03-03T10:00:00Z".to_string(),
            role: "user".to_string(),
            content: "day one".to_string(),
            job_id: None,
            flush: None,
        };
        let entry2 = LogEntry {
            timestamp: "2026-03-04T10:00:00Z".to_string(),
            role: "user".to_string(),
            content: "day two".to_string(),
            job_id: None,
            flush: None,
        };
        log.append(&entry1).unwrap();
        log.append(&entry2).unwrap();

        assert!(dir.path().join("logs/daily_logs_2026-03-03.jsonl").exists());
        assert!(dir.path().join("logs/daily_logs_2026-03-04.jsonl").exists());
    }

    #[test]
    fn fts5_search() {
        let (log, _dir) = test_episodic();
        log.append(&LogEntry {
            timestamp: "2026-03-03T10:00:00Z".to_string(),
            role: "assistant".to_string(),
            content: "Restarted front-door camera".to_string(),
            job_id: Some("job-1".to_string()),
            flush: None,
        })
        .unwrap();
        log.append(&LogEntry {
            timestamp: "2026-03-03T10:01:00Z".to_string(),
            role: "user".to_string(),
            content: "What is the weather?".to_string(),
            job_id: None,
            flush: None,
        })
        .unwrap();

        let results = log.search("camera", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].content.contains("camera"));
        assert_eq!(results[0].job_id.as_deref(), Some("job-1"));
    }

    #[test]
    fn fts5_empty_query() {
        let (log, _dir) = test_episodic();
        log.append(&LogEntry {
            timestamp: "2026-03-03T10:00:00Z".to_string(),
            role: "user".to_string(),
            content: "hello".to_string(),
            job_id: None,
            flush: None,
        })
        .unwrap();

        assert!(log.search("", 10).unwrap().is_empty());
    }

    #[test]
    fn make_entry_timestamp() {
        let entry = EpisodicLog::make_entry("user", "hello", None);
        assert_eq!(entry.role, "user");
        assert_eq!(entry.content, "hello");
        assert!(entry.timestamp.ends_with('Z'));
        assert!(entry.job_id.is_none());
        assert!(entry.flush.is_none());
    }

    #[test]
    fn make_flush_entry() {
        let entry = EpisodicLog::make_flush_entry("flush content", Some("job-123"));
        assert_eq!(entry.role, "system");
        assert_eq!(entry.flush, Some(true));
        assert_eq!(entry.job_id.as_deref(), Some("job-123"));
        assert!(entry.content.starts_with(FLUSH_PREFIX));
        assert_eq!(
            EpisodicLog::strip_flush_prefix(&entry.content),
            "flush content"
        );
    }

    #[test]
    fn flush_entry_serialization() {
        let entry = EpisodicLog::make_flush_entry("test flush", None);
        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("\"flush\":true"));
        assert!(json.contains("\"role\":\"system\""));

        // Non-flush entry should NOT have flush field
        let normal = EpisodicLog::make_entry("user", "hello", None);
        let json = serde_json::to_string(&normal).unwrap();
        assert!(!json.contains("flush"));
    }

    #[test]
    fn ndjson_format_is_valid() {
        let (log, dir) = test_episodic();
        log.append(&LogEntry {
            timestamp: "2026-03-03T10:00:00Z".to_string(),
            role: "user".to_string(),
            content: "test message".to_string(),
            job_id: None,
            flush: None,
        })
        .unwrap();

        let path = dir.path().join("logs/daily_logs_2026-03-03.jsonl");
        let content = std::fs::read_to_string(&path).unwrap();
        for line in content.lines() {
            let parsed: LogEntry = serde_json::from_str(line).unwrap();
            assert_eq!(parsed.content, "test message");
        }
    }

    #[test]
    fn sanitize_fts5_query_handles_special_chars() {
        let result = sanitize_fts5_query("AND OR NOT camera");
        assert!(result.contains("\"AND\""));
        assert!(result.contains("\"camera\""));
    }

    #[test]
    fn forget_removes_from_fts5() {
        let (log, _dir) = test_episodic();
        log.append(&LogEntry {
            timestamp: "2026-03-03T10:00:00Z".to_string(),
            role: "assistant".to_string(),
            content: "The password is hunter2".to_string(),
            job_id: None,
            flush: None,
        })
        .unwrap();
        log.append(&LogEntry {
            timestamp: "2026-03-03T10:01:00Z".to_string(),
            role: "user".to_string(),
            content: "weather looks good".to_string(),
            job_id: None,
            flush: None,
        })
        .unwrap();

        // Search finds both initially
        let results = log.search("password", 10).unwrap();
        assert_eq!(results.len(), 1);

        // Forget the sensitive entry
        let count = log
            .forget("password", "user requested PII removal")
            .unwrap();
        assert_eq!(count, 1);

        // No longer findable via search
        let results = log.search("password", 10).unwrap();
        assert!(results.is_empty());

        // Other entries still searchable
        let results = log.search("weather", 10).unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn forget_creates_audit_file() {
        let (log, dir) = test_episodic();
        log.append(&LogEntry {
            timestamp: "2026-03-03T10:00:00Z".to_string(),
            role: "user".to_string(),
            content: "sensitive data here".to_string(),
            job_id: None,
            flush: None,
        })
        .unwrap();

        log.forget("sensitive", "gdpr request").unwrap();

        let deleted_dir = dir.path().join("logs/.deleted");
        assert!(deleted_dir.exists());
        let entries: Vec<_> = std::fs::read_dir(&deleted_dir).unwrap().flatten().collect();
        assert_eq!(entries.len(), 1);

        let audit_content = std::fs::read_to_string(entries[0].path()).unwrap();
        assert!(audit_content.contains("gdpr request"));
        assert!(audit_content.contains("sensitive data here"));
    }

    #[test]
    fn forget_empty_query_returns_zero() {
        let (log, _dir) = test_episodic();
        assert_eq!(log.forget("", "no reason").unwrap(), 0);
    }

    #[test]
    fn search_with_decay_zero_equals_no_decay() {
        let (log, _dir) = test_episodic();
        log.append(&LogEntry {
            timestamp: "2026-03-03T10:00:00Z".to_string(),
            role: "user".to_string(),
            content: "camera restarted".to_string(),
            job_id: None,
            flush: None,
        })
        .unwrap();

        let no_decay = log.search("camera", 10).unwrap();
        let zero_decay = log.search_with_decay("camera", 10, 0).unwrap();
        assert_eq!(no_decay.len(), zero_decay.len());
        assert_eq!(no_decay[0].content, zero_decay[0].content);
    }

    #[test]
    fn search_with_decay_recent_ranks_higher() {
        let (log, _dir) = test_episodic();
        // Old entry
        log.append(&LogEntry {
            timestamp: "2025-01-01T10:00:00Z".to_string(),
            role: "assistant".to_string(),
            content: "camera was offline for maintenance".to_string(),
            job_id: None,
            flush: None,
        })
        .unwrap();
        // Recent entry
        log.append(&LogEntry {
            timestamp: "2026-03-03T10:00:00Z".to_string(),
            role: "assistant".to_string(),
            content: "camera is now online and healthy".to_string(),
            job_id: None,
            flush: None,
        })
        .unwrap();

        let results = log.search_with_decay("camera", 10, 30).unwrap();
        assert_eq!(results.len(), 2);
        // With 30-day half-life, the recent entry should rank first
        assert!(results[0].timestamp > results[1].timestamp);
    }

    #[test]
    fn prune_old_logs_removes_old_files() {
        let (log, dir) = test_episodic();
        // Use completely distinct words to avoid OR-search cross-matching
        log.append(&LogEntry {
            timestamp: "2025-01-01T10:00:00Z".to_string(),
            role: "user".to_string(),
            content: "ancient forgotten artifact".to_string(),
            job_id: None,
            flush: None,
        })
        .unwrap();
        log.append(&LogEntry {
            timestamp: "2026-03-03T10:00:00Z".to_string(),
            role: "user".to_string(),
            content: "recent important update".to_string(),
            job_id: None,
            flush: None,
        })
        .unwrap();

        assert!(dir.path().join("logs/daily_logs_2025-01-01.jsonl").exists());
        assert!(dir.path().join("logs/daily_logs_2026-03-03.jsonl").exists());

        // Prune with 30-day retention (the 2025 file is way older)
        let pruned = log.prune_old_logs(30).unwrap();
        assert_eq!(pruned, 1);
        assert!(!dir.path().join("logs/daily_logs_2025-01-01.jsonl").exists());
        assert!(dir.path().join("logs/daily_logs_2026-03-03.jsonl").exists());

        // FTS5 should also be cleaned — old entry gone
        let results = log.search("ancient forgotten", 10).unwrap();
        assert!(results.is_empty());
        // New entry still searchable
        let results = log.search("recent important", 10).unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn prune_zero_retention_keeps_all() {
        let (log, dir) = test_episodic();
        log.append(&LogEntry {
            timestamp: "2020-01-01T10:00:00Z".to_string(),
            role: "user".to_string(),
            content: "ancient".to_string(),
            job_id: None,
            flush: None,
        })
        .unwrap();

        let pruned = log.prune_old_logs(0).unwrap();
        assert_eq!(pruned, 0);
        assert!(dir.path().join("logs/daily_logs_2020-01-01.jsonl").exists());
    }

    #[test]
    fn prune_empty_dir_returns_zero() {
        let (log, _dir) = test_episodic();
        let pruned = log.prune_old_logs(30).unwrap();
        assert_eq!(pruned, 0);
    }

    #[test]
    fn latest_plan_returns_none_when_empty() {
        let (log, _dir) = test_episodic();
        assert!(log.latest_plan().unwrap().is_none());
    }

    #[test]
    fn latest_plan_returns_most_recent() {
        let (log, _dir) = test_episodic();
        // Non-plan entries should be ignored
        log.append(&LogEntry {
            timestamp: "2026-03-03T10:00:00Z".to_string(),
            role: "assistant".to_string(),
            content: "I checked the sensors.".to_string(),
            job_id: None,
            flush: None,
        })
        .unwrap();
        // First plan
        log.append(&LogEntry {
            timestamp: "2026-03-03T10:01:00Z".to_string(),
            role: "plan".to_string(),
            content: "Step 1: install node. Step 2: build.".to_string(),
            job_id: None,
            flush: None,
        })
        .unwrap();
        // Second (newer) plan
        log.append(&LogEntry {
            timestamp: "2026-03-03T10:05:00Z".to_string(),
            role: "plan".to_string(),
            content: "Revised plan: restart camera first.".to_string(),
            job_id: Some("job-42".to_string()),
            flush: None,
        })
        .unwrap();

        let plan = log.latest_plan().unwrap().expect("should find a plan");
        assert_eq!(plan.role, "plan");
        assert!(plan.content.contains("Revised plan"));
        assert_eq!(plan.job_id.as_deref(), Some("job-42"));
    }

    #[test]
    fn latest_plan_ignores_non_plan_roles() {
        let (log, _dir) = test_episodic();
        log.append(&LogEntry {
            timestamp: "2026-03-03T10:00:00Z".to_string(),
            role: "user".to_string(),
            content: "plan something".to_string(),
            job_id: None,
            flush: None,
        })
        .unwrap();
        log.append(&LogEntry {
            timestamp: "2026-03-03T10:01:00Z".to_string(),
            role: "assistant".to_string(),
            content: "here is a plan".to_string(),
            job_id: None,
            flush: None,
        })
        .unwrap();

        // No entries with role="plan" exist
        assert!(log.latest_plan().unwrap().is_none());
    }

    #[test]
    fn latest_flush_returns_none_when_empty() {
        let (log, _dir) = test_episodic();
        assert!(log.latest_flush().unwrap().is_none());
    }

    #[test]
    fn latest_flush_finds_flush_entry() {
        let (log, _dir) = test_episodic();
        // Regular entry
        log.append(&LogEntry {
            timestamp: "2026-03-03T10:00:00Z".to_string(),
            role: "user".to_string(),
            content: "hello world".to_string(),
            job_id: None,
            flush: None,
        })
        .unwrap();
        // Flush entry
        let flush = EpisodicLog::make_flush_entry("Camera is healthy. Job patrol running.", None);
        log.append(&flush).unwrap();

        let result = log.latest_flush().unwrap().expect("should find flush");
        assert!(result.content.starts_with(FLUSH_PREFIX));
        assert_eq!(
            EpisodicLog::strip_flush_prefix(&result.content),
            "Camera is healthy. Job patrol running."
        );
    }

    #[test]
    fn latest_flush_returns_most_recent() {
        let (log, _dir) = test_episodic();
        // First flush
        let mut flush1 = EpisodicLog::make_flush_entry("first flush state", None);
        flush1.timestamp = "2026-03-03T10:00:00Z".to_string();
        log.append(&flush1).unwrap();
        // Second (newer) flush
        let mut flush2 = EpisodicLog::make_flush_entry("second flush state", Some("job-42"));
        flush2.timestamp = "2026-03-03T11:00:00Z".to_string();
        log.append(&flush2).unwrap();

        let result = log.latest_flush().unwrap().expect("should find flush");
        assert!(
            EpisodicLog::strip_flush_prefix(&result.content).contains("second flush state"),
            "should return the most recent flush"
        );
    }

    #[test]
    fn latest_flush_ignores_non_flush_system_entries() {
        let (log, _dir) = test_episodic();
        // Regular system message (no CONTEXT_FLUSH prefix)
        log.append(&LogEntry {
            timestamp: "2026-03-03T10:00:00Z".to_string(),
            role: "system".to_string(),
            content: "System initialized".to_string(),
            job_id: None,
            flush: None,
        })
        .unwrap();

        // No flush entries exist
        assert!(log.latest_flush().unwrap().is_none());
    }

    #[test]
    fn strip_flush_prefix_on_non_flush_content() {
        assert_eq!(
            EpisodicLog::strip_flush_prefix("regular content"),
            "regular content"
        );
    }
}
