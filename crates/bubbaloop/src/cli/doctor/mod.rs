//! Doctor command for system diagnostics
//!
//! Performs comprehensive health checks on all bubbaloop components:
//! - System services (zenohd, daemon, bridge)
//! - Daemon HTTP connectivity and health (via REST API)
//! - Zenoh data plane availability (port check for node streaming)
//! - Security posture
//!
//! Provides actionable fixes for each issue found.

pub mod checks;
pub mod fixes;

use anyhow::Result;
use serde::Serialize;

use fixes::FixAction;

#[derive(Debug, Serialize)]
pub struct DiagnosticResult {
    check: String,
    passed: bool,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    fix: Option<String>,
    #[serde(skip)]
    fix_action: Option<FixAction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<serde_json::Value>,
}

impl DiagnosticResult {
    pub fn pass(check: &str, message: &str) -> Self {
        Self {
            check: check.to_string(),
            passed: true,
            message: message.to_string(),
            fix: None,
            fix_action: None,
            details: None,
        }
    }

    pub fn pass_with_details(check: &str, message: &str, details: serde_json::Value) -> Self {
        Self {
            check: check.to_string(),
            passed: true,
            message: message.to_string(),
            fix: None,
            fix_action: None,
            details: Some(details),
        }
    }

    pub fn fail(check: &str, message: &str, fix: &str) -> Self {
        Self {
            check: check.to_string(),
            passed: false,
            message: message.to_string(),
            fix: Some(fix.to_string()),
            fix_action: None,
            details: None,
        }
    }

    pub fn fail_with_action(check: &str, message: &str, fix: &str, action: FixAction) -> Self {
        Self {
            check: check.to_string(),
            passed: false,
            message: message.to_string(),
            fix: Some(fix.to_string()),
            fix_action: Some(action),
            details: None,
        }
    }
}

pub async fn run(fix: bool, json: bool, check: &str) -> Result<()> {
    let check_type = check.to_lowercase();

    if !json {
        if fix {
            println!("bubbaloop doctor --fix");
            println!("=====================");
        } else {
            println!("bubbaloop doctor");
            println!("================");
        }
        println!();
    }

    let mut results = Vec::new();
    let mut fixes_applied = 0;

    let run_config = check_type == "all" || check_type == "config";
    let run_services = check_type == "all" || check_type == "zenoh";
    let run_connectivity = check_type == "all" || check_type == "zenoh";
    let run_daemon = check_type == "all" || check_type == "daemon";
    let run_security = check_type == "all" || check_type == "security";

    // 0. Check configuration files
    if run_config {
        if !json {
            println!("[1/6] Checking configuration...");
        }
        results.extend(checks::check_configuration().await);

        if fix && !json {
            fixes_applied += apply_fixes(&mut results).await;
        }
        if !json {
            println!();
        }
    }

    // 1. Check system services
    if run_services {
        if !json {
            println!("[2/6] Checking system services...");
        }
        results.extend(checks::check_system_services().await);

        if fix && !json {
            fixes_applied += apply_fixes(&mut results).await;
        }
        if !json {
            println!();
        }
    }

    // 2. Check daemon connectivity via Zenoh manifest
    if run_connectivity {
        if !json {
            println!("[3/6] Checking daemon connectivity...");
        }
        results.extend(checks::check_daemon_connectivity().await);

        if !json {
            println!();
        }
    }

    // 3. Check daemon health
    if run_daemon {
        if !json {
            println!("[4/6] Checking daemon health...");
        }
        results.extend(checks::check_daemon_health().await);

        if !json {
            println!();
        }
    }

    // 4. Check Zenoh data plane (optional, for streaming)
    if check_type == "all" {
        if !json {
            println!("[5/6] Checking Zenoh data plane...");
        }
        results.extend(checks::check_node_subscriptions().await);

        if !json {
            println!();
        }
    }

    // 5. Security checks
    if run_security {
        if !json {
            println!("[6/6] Checking security posture...");
        }
        results.extend(checks::check_security().await);
        if !json {
            println!();
        }
    }

    if json {
        print_json_results(&results, fixes_applied)?;
    } else {
        print_human_results(&results, fixes_applied, fix)?;
    }

    Ok(())
}

fn print_json_results(results: &[DiagnosticResult], fixes_applied: usize) -> Result<()> {
    let passed = results.iter().filter(|r| r.passed).count();
    let failed = results.iter().filter(|r| !r.passed).count();

    let output = serde_json::json!({
        "summary": {
            "total": results.len(),
            "passed": passed,
            "failed": failed,
            "fixes_applied": fixes_applied,
        },
        "checks": results,
    });

    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

fn print_human_results(
    results: &[DiagnosticResult],
    fixes_applied: usize,
    fix: bool,
) -> Result<()> {
    println!("Summary");
    println!("=======");
    println!();

    let mut issues_found = 0;

    for result in results {
        let symbol = if result.passed { "✓" } else { "✗" };
        println!("[{}] {}: {}", symbol, result.check, result.message);

        if !result.passed {
            issues_found += 1;
            if let Some(fix_hint) = &result.fix {
                if result.fix_action.is_some() {
                    println!("    → Auto-fixable: {}", fix_hint);
                } else {
                    println!("    → Fix: {}", fix_hint);
                }
            }
        }

        // Print details if available
        if let Some(details) = &result.details {
            if let Some(obj) = details.as_object() {
                for (key, value) in obj {
                    println!("    {} = {}", key, value);
                }
            }
        }
    }

    println!();
    if issues_found == 0 {
        println!("All checks passed!");
    } else {
        println!(
            "Found {} issue{}",
            issues_found,
            if issues_found == 1 { "" } else { "s" }
        );
        if fixes_applied > 0 {
            println!(
                "Applied {} fix{}",
                fixes_applied,
                if fixes_applied == 1 { "" } else { "es" }
            );
        } else if !fix {
            // Count auto-fixable issues
            let auto_fixable: usize = results
                .iter()
                .filter(|r| !r.passed && r.fix_action.is_some())
                .count();
            if auto_fixable > 0 {
                println!();
                println!(
                    "Tip: Run 'bubbaloop doctor --fix' to automatically fix {} issue{}",
                    auto_fixable,
                    if auto_fixable == 1 { "" } else { "s" }
                );
            }
        }
    }

    Ok(())
}

/// Apply all available fixes and return count of fixes applied
async fn apply_fixes(results: &mut [DiagnosticResult]) -> usize {
    let mut fixes_applied = 0;

    for result in results.iter_mut() {
        if result.passed || result.fix_action.is_none() {
            continue;
        }

        let action = result.fix_action.clone().unwrap();
        println!("    → Fixing: {}", action.description());

        match action.execute().await {
            Ok(msg) => {
                println!("      ✓ {}", msg);
                result.passed = true;
                result.message = format!("{} (fixed)", result.message);
                fixes_applied += 1;
            }
            Err(e) => {
                println!("      ✗ Failed: {}", e);
            }
        }
    }

    fixes_applied
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diagnostic_result_pass() {
        let result = DiagnosticResult::pass("test", "all good");
        assert!(result.passed);
        assert_eq!(result.check, "test");
        assert_eq!(result.message, "all good");
        assert!(result.fix.is_none());
    }

    #[test]
    fn test_diagnostic_result_fail() {
        let result = DiagnosticResult::fail("test", "something wrong", "do this");
        assert!(!result.passed);
        assert_eq!(result.check, "test");
        assert_eq!(result.message, "something wrong");
        assert_eq!(result.fix, Some("do this".to_string()));
    }
}
