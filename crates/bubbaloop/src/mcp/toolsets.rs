//! Dynamic toolset disclosure — gap #12 from `bubbaloop-mcp/docs/alignment-notes.md`.
//!
//! Pattern lifted from the GitHub MCP server's `X-MCP-Toolsets` header and the
//! Postman MCP `toolset_*` meta-tools (Jan 2026 changelog). Three meta-tools
//! gate the full surface:
//!
//! - `toolset_list` — show all toolset names + which are enabled
//! - `toolset_enable` — turn a toolset on for the current session
//! - `toolset_get_tools` — list the tools in a specific toolset (without enabling)
//!
//! Per-session state lives on `BubbaLoopMcpServer::enabled_toolsets`. Defaults
//! come from `$BUBBALOOP_MCP_TOOLSETS` (comma-separated, or `*` / unset = all
//! toolsets enabled — backwards compatible). Sysadmins can pre-narrow at deploy
//! time; agents can demand-load more at runtime via `toolset_enable`.
//!
//! Why this matters: Klavis benchmark shows 60–85% context-window reduction +
//! 13–15% tool-selection accuracy gain on small models. GitHub Copilot cut its
//! tool surface 40 → 13 with measurable benchmark improvement. Bubbaloop sits
//! at 44 — without dynamic toolsets, this is the next size class where small
//! local models (Llama 3.1 8B, Qwen 2.5 7B) start mis-selecting.

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

/// Authoritative toolset → tool-names map. The names here MUST match the
/// `name = "..."` argument on each `#[tool(...)]` attribute in `tools.rs`.
/// Adding a tool: add it here AND in the existing places (RBAC, mock test
/// arrays, instructions block, dispatch.rs if internal-agent visible).
pub fn toolset_definitions() -> &'static HashMap<&'static str, Vec<&'static str>> {
    static MAP: OnceLock<HashMap<&'static str, Vec<&'static str>>> = OnceLock::new();
    MAP.get_or_init(|| {
        let mut m: HashMap<&'static str, Vec<&'static str>> = HashMap::new();

        // Node lifecycle, introspection, and command — by far the largest
        // toolset; the operator's bread-and-butter.
        m.insert(
            "node",
            vec![
                // Discovery (Viewer)
                "node_list",
                "node_health",
                "node_config",
                "node_manifest",
                "node_commands",
                "node_schema",
                "node_logs",
                "node_stream_info",
                "node_discover",
                "capability_discover",
                "dataflow_graph",
                // Lifecycle (Operator)
                "node_command_send",
                "node_start",
                "node_stop",
                "node_restart",
                "node_autostart_enable",
                "node_autostart_disable",
                // Lifecycle (Admin) — install/build/remove
                "node_install",
                "node_remove",
                "node_build",
                "node_uninstall",
                "node_clean",
            ],
        );

        // System-level introspection + the broad-keyspace zenoh query.
        m.insert(
            "system",
            vec!["system_status", "machine_info", "zenoh_query"],
        );

        // Mission YAML lifecycle.
        m.insert(
            "mission",
            vec![
                "mission_list",
                "mission_pause",
                "mission_resume",
                "mission_cancel",
            ],
        );

        // Reactive alert rules.
        m.insert(
            "alert",
            vec!["alert_list", "alert_register", "alert_unregister"],
        );

        // Per-mission safety constraints.
        m.insert("constraint", vec!["constraint_list", "constraint_register"]);

        // Durable agent beliefs (subject+predicate).
        m.insert("belief", vec!["belief_get", "belief_update"]);

        // Live world-state snapshot + context provider configuration.
        m.insert("world_state", vec!["world_state_list", "context_configure"]);

        // Human-in-the-loop proposals.
        m.insert(
            "proposal",
            vec!["proposal_list", "proposal_approve", "proposal_reject"],
        );

        // Scheduled jobs (cron-style).
        m.insert("job", vec!["job_list", "job_delete"]);

        // Episodic memory admin.
        m.insert("memory", vec!["memory_episodic_clear"]);

        // System telemetry (CPU/mem/disk/temp).
        m.insert(
            "telemetry",
            vec![
                "telemetry_get",
                "telemetry_history",
                "telemetry_config_update",
            ],
        );

        m
    })
}

/// Names of the meta-tools that are ALWAYS visible regardless of which
/// toolsets are enabled. Without these the dynamic-disclosure mechanism is
/// inaccessible to the agent.
pub fn meta_tools() -> &'static [&'static str] {
    &["toolset_list", "toolset_enable", "toolset_get_tools"]
}

/// Initial enabled-toolsets set, derived from `$BUBBALOOP_MCP_TOOLSETS`.
///
/// Format: comma-separated toolset names (e.g. `node,system,mission`), or `*`
/// (or unset) for all toolsets enabled. Unknown names are dropped with a
/// warning log so a typo doesn't silently disable a whole domain.
///
/// **Default = all enabled** — backwards compatible. Sysadmins can narrow by
/// setting the env var before launching the daemon. Agents can broaden at
/// runtime via the `toolset_enable` meta-tool.
pub fn default_enabled() -> HashSet<String> {
    let raw = std::env::var("BUBBALOOP_MCP_TOOLSETS").ok();
    match raw.as_deref() {
        None | Some("") | Some("*") => toolset_definitions()
            .keys()
            .map(|s| (*s).to_string())
            .collect(),
        Some(list) => {
            let known: HashSet<&'static str> = toolset_definitions().keys().copied().collect();
            let mut out = HashSet::new();
            for raw_name in list.split(',') {
                let name = raw_name.trim();
                if name.is_empty() {
                    continue;
                }
                if known.contains(name) {
                    out.insert(name.to_string());
                } else {
                    log::warn!(
                        "BUBBALOOP_MCP_TOOLSETS: unknown toolset '{}' (known: {:?}) — dropping",
                        name,
                        known
                    );
                }
            }
            if out.is_empty() {
                log::warn!(
                    "BUBBALOOP_MCP_TOOLSETS='{}' yielded zero valid toolsets — meta-tools only \
                     (use `toolset_enable <name>` from the agent to broaden)",
                    list
                );
            }
            out
        }
    }
}

/// Returns true if `tool_name` is exposed under any of the `enabled` toolsets,
/// or is one of the always-visible meta-tools.
///
/// Tools in `tools.rs` that are NOT listed in any toolset will be invisible
/// when filtering — that's a forcing function to keep `toolset_definitions`
/// in sync as new tools land.
pub fn is_visible(tool_name: &str, enabled: &HashSet<String>) -> bool {
    if meta_tools().contains(&tool_name) {
        return true;
    }
    for ts_name in enabled {
        if let Some(tools) = toolset_definitions().get(ts_name.as_str()) {
            if tools.contains(&tool_name) {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meta_tools_always_visible_even_with_empty_enabled_set() {
        let enabled = HashSet::new();
        assert!(is_visible("toolset_list", &enabled));
        assert!(is_visible("toolset_enable", &enabled));
        assert!(is_visible("toolset_get_tools", &enabled));
        assert!(!is_visible("node_list", &enabled));
    }

    #[test]
    fn enabling_a_toolset_exposes_all_its_tools() {
        let mut enabled = HashSet::new();
        enabled.insert("node".to_string());
        assert!(is_visible("node_list", &enabled));
        assert!(is_visible("node_start", &enabled));
        assert!(is_visible("node_install", &enabled));
        // not in node toolset
        assert!(!is_visible("mission_list", &enabled));
        assert!(!is_visible("belief_get", &enabled));
    }

    #[test]
    fn unknown_tool_never_visible() {
        let mut enabled = HashSet::new();
        for ts in toolset_definitions().keys() {
            enabled.insert(ts.to_string());
        }
        assert!(!is_visible("totally_made_up_tool", &enabled));
    }

    #[test]
    fn default_enabled_when_env_unset_returns_all_toolsets() {
        // Save+clear+restore so this test doesn't race against others
        // touching the same env var.
        let prev = std::env::var("BUBBALOOP_MCP_TOOLSETS").ok();
        std::env::remove_var("BUBBALOOP_MCP_TOOLSETS");
        let got = default_enabled();
        let want: HashSet<String> = toolset_definitions()
            .keys()
            .map(|s| (*s).to_string())
            .collect();
        assert_eq!(got, want);
        if let Some(v) = prev {
            std::env::set_var("BUBBALOOP_MCP_TOOLSETS", v);
        }
    }

    #[test]
    fn default_enabled_with_explicit_subset_returns_subset() {
        let prev = std::env::var("BUBBALOOP_MCP_TOOLSETS").ok();
        std::env::set_var(
            "BUBBALOOP_MCP_TOOLSETS",
            "node,mission, belief ,unknown_one",
        );
        let got = default_enabled();
        // Should contain the three valid ones, drop the unknown.
        assert!(got.contains("node"));
        assert!(got.contains("mission"));
        assert!(got.contains("belief"));
        assert!(!got.contains("unknown_one"));
        assert_eq!(got.len(), 3);
        match prev {
            Some(v) => std::env::set_var("BUBBALOOP_MCP_TOOLSETS", v),
            None => std::env::remove_var("BUBBALOOP_MCP_TOOLSETS"),
        }
    }

    #[test]
    fn star_means_all() {
        let prev = std::env::var("BUBBALOOP_MCP_TOOLSETS").ok();
        std::env::set_var("BUBBALOOP_MCP_TOOLSETS", "*");
        let got = default_enabled();
        let want: HashSet<String> = toolset_definitions()
            .keys()
            .map(|s| (*s).to_string())
            .collect();
        assert_eq!(got, want);
        match prev {
            Some(v) => std::env::set_var("BUBBALOOP_MCP_TOOLSETS", v),
            None => std::env::remove_var("BUBBALOOP_MCP_TOOLSETS"),
        }
    }

    /// Sanity check: every toolset's tool list contains only valid identifiers
    /// (snake_case, no whitespace, no duplicates within a toolset).
    #[test]
    fn toolset_definitions_are_well_formed() {
        for (name, tools) in toolset_definitions() {
            assert!(
                !tools.is_empty(),
                "toolset '{}' is empty — drop it or populate it",
                name
            );
            let mut seen = HashSet::new();
            for t in tools {
                assert!(
                    !t.contains(char::is_whitespace),
                    "toolset '{}' contains tool with whitespace: {:?}",
                    name,
                    t
                );
                assert!(
                    seen.insert(*t),
                    "toolset '{}' has duplicate tool: {:?}",
                    name,
                    t
                );
            }
        }
    }

    /// Sanity check: meta-tool names don't collide with regular toolset
    /// contents — that would be ambiguous.
    #[test]
    fn meta_tools_disjoint_from_regular_toolsets() {
        let meta: HashSet<&str> = meta_tools().iter().copied().collect();
        for tools in toolset_definitions().values() {
            for t in tools {
                assert!(
                    !meta.contains(t),
                    "tool '{}' is BOTH a regular tool and a meta-tool",
                    t
                );
            }
        }
    }
}
