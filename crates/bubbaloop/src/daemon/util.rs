//! Shared daemon utilities.

/// Get machine ID from environment or hostname.
///
/// Resolution order:
/// 1. `BUBBALOOP_MACHINE_ID` env var (used as-is)
/// 2. System hostname with hyphens replaced by underscores
/// 3. `"unknown"` fallback
///
/// Hyphens are sanitized to underscores for Zenoh topic compatibility,
/// matching the convention used by external nodes.
pub fn get_machine_id() -> String {
    std::env::var("BUBBALOOP_MACHINE_ID")
        .unwrap_or_else(|_| {
            hostname::get()
                .map(|h| h.to_string_lossy().to_string())
                .unwrap_or_else(|_| "unknown".to_string())
        })
        .replace('-', "_")
}

/// Sanitize a message for safe logging by stripping control characters.
///
/// Removes characters below 0x20 (except `\n` and `\t`) and DEL (0x7F)
/// to prevent log injection from external error messages (D-Bus, Zenoh, serde, etc.).
pub fn sanitize_log_msg(msg: &str) -> String {
    msg.chars()
        .map(|c| {
            if c == '\n' || c == '\t' {
                c
            } else if c < '\x20' || c == '\x7F' {
                ' '
            } else {
                c
            }
        })
        .collect()
}

/// Open a SQLite connection with WAL mode and busy timeout configured.
///
/// All daemon SQLite stores should use this instead of duplicating the pragma setup.
pub fn open_sqlite(path: &std::path::Path) -> anyhow::Result<rusqlite::Connection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let conn = rusqlite::Connection::open(path)?;
    conn.query_row("PRAGMA journal_mode=WAL", [], |_| Ok(()))?;
    conn.query_row("PRAGMA busy_timeout=5000", [], |_| Ok(()))?;
    Ok(conn)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Env var tests must run serially since they mutate shared process state
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn test_get_machine_id_from_env() {
        let _lock = ENV_LOCK.lock().unwrap();
        let prev = std::env::var("BUBBALOOP_MACHINE_ID").ok();
        std::env::set_var("BUBBALOOP_MACHINE_ID", "my_jetson_01");

        let id = get_machine_id();
        assert_eq!(id, "my_jetson_01");

        if let Some(v) = prev {
            std::env::set_var("BUBBALOOP_MACHINE_ID", v);
        } else {
            std::env::remove_var("BUBBALOOP_MACHINE_ID");
        }
    }

    #[test]
    fn test_get_machine_id_sanitizes_hyphens() {
        let _lock = ENV_LOCK.lock().unwrap();
        let prev = std::env::var("BUBBALOOP_MACHINE_ID").ok();
        std::env::set_var("BUBBALOOP_MACHINE_ID", "jetson-orin-nano");

        let id = get_machine_id();
        assert_eq!(id, "jetson_orin_nano");

        if let Some(v) = prev {
            std::env::set_var("BUBBALOOP_MACHINE_ID", v);
        } else {
            std::env::remove_var("BUBBALOOP_MACHINE_ID");
        }
    }

    #[test]
    fn test_get_machine_id_falls_back_to_hostname() {
        let _lock = ENV_LOCK.lock().unwrap();
        let prev = std::env::var("BUBBALOOP_MACHINE_ID").ok();
        std::env::remove_var("BUBBALOOP_MACHINE_ID");

        let id = get_machine_id();
        // Should return a non-empty string (hostname or "unknown")
        assert!(!id.is_empty());
        // Should not contain hyphens (they get sanitized)
        assert!(
            !id.contains('-'),
            "machine_id should not contain hyphens: {id}"
        );

        if let Some(v) = prev {
            std::env::set_var("BUBBALOOP_MACHINE_ID", v);
        }
    }

    #[test]
    fn test_sanitize_log_msg_strips_control_chars() {
        assert_eq!(sanitize_log_msg("hello\x00world"), "hello world");
        assert_eq!(sanitize_log_msg("a\x01b\x7Fc"), "a b c");
        // Preserve newlines and tabs
        assert_eq!(sanitize_log_msg("line1\nline2\tok"), "line1\nline2\tok");
        // Normal text is unchanged
        assert_eq!(sanitize_log_msg("normal message"), "normal message");
    }
}
