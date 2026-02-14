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
}
