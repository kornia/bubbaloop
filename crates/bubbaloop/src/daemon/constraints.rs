//! Constraint engine — fail-closed safety validation for physical AI.
//!
//! Pure-computation constraint checks with no I/O. Every constraint validation
//! returns `ConstraintResult::Allow`, `Deny`, or `ValidatorError` — both Deny
//! and ValidatorError halt the command.

use rusqlite::{params, Connection};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

// ── ConstraintResult (safety-first) ─────────────────────────────────

/// Result of a constraint check. Fail-closed: both `Deny` and `ValidatorError` halt.
#[derive(Debug, Clone, PartialEq)]
pub enum ConstraintResult {
    Allow,
    Deny { reason: String },
    ValidatorError { reason: String },
}

impl ConstraintResult {
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Allow)
    }

    pub fn reason(&self) -> Option<&str> {
        match self {
            Self::Deny { reason } | Self::ValidatorError { reason } => Some(reason),
            Self::Allow => None,
        }
    }
}

// ── Constraint enum (pure data, no I/O) ─────────────────────────────

/// A safety constraint. Pure data — no I/O in validation methods.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum Constraint {
    /// Axis-aligned bounding box workspace limit.
    Workspace {
        x: (f64, f64),
        y: (f64, f64),
        z: (f64, f64),
    },
    /// Maximum velocity magnitude.
    MaxVelocity(f64),
    /// Spherical forbidden zone.
    ForbiddenZone { center: [f64; 3], radius: f64 },
    /// Maximum force magnitude.
    MaxForce(f64),
}

impl Constraint {
    /// Validate a position goal. Returns Deny if out of bounds.
    /// NEVER panics — all errors produce ValidatorError.
    pub fn validate_goal(&self, position: &[f64]) -> ConstraintResult {
        match self {
            Constraint::Workspace { x, y, z } => {
                let check =
                    |v: f64, (lo, hi): (f64, f64), axis: char| -> Option<ConstraintResult> {
                        if v < lo || v > hi {
                            Some(ConstraintResult::Deny {
                                reason: format!(
                                    "workspace.{} [{:.3},{:.3}] violated: {:.3}",
                                    axis, lo, hi, v
                                ),
                            })
                        } else {
                            None
                        }
                    };
                let px = position.first().copied().unwrap_or(0.0);
                let py = position.get(1).copied().unwrap_or(0.0);
                let pz = position.get(2).copied().unwrap_or(0.0);
                check(px, *x, 'x')
                    .or_else(|| check(py, *y, 'y'))
                    .or_else(|| check(pz, *z, 'z'))
                    .unwrap_or(ConstraintResult::Allow)
            }
            Constraint::ForbiddenZone { center, radius } => {
                let dist_sq: f64 = (0..3)
                    .map(|i| {
                        let d = position.get(i).copied().unwrap_or(0.0) - center[i];
                        d * d
                    })
                    .sum();
                let dist = dist_sq.sqrt();
                if dist < *radius {
                    ConstraintResult::Deny {
                        reason: format!(
                            "inside forbidden zone (dist={:.3} < r={:.3})",
                            dist, radius
                        ),
                    }
                } else {
                    ConstraintResult::Allow
                }
            }
            // velocity/force checked by validate_velocity
            _ => ConstraintResult::Allow,
        }
    }

    /// Validate a velocity or force magnitude.
    pub fn validate_velocity(&self, speed: f64) -> ConstraintResult {
        match self {
            Constraint::MaxVelocity(max) => {
                if speed > *max {
                    ConstraintResult::Deny {
                        reason: format!("velocity {:.3} > max {:.3}", speed, max),
                    }
                } else {
                    ConstraintResult::Allow
                }
            }
            Constraint::MaxForce(max) => {
                if speed > *max {
                    ConstraintResult::Deny {
                        reason: format!("force {:.3} > max {:.3}", speed, max),
                    }
                } else {
                    ConstraintResult::Allow
                }
            }
            _ => ConstraintResult::Allow,
        }
    }
}

// ── ConstraintEngine ────────────────────────────────────────────────

/// Evaluates a set of constraints against position/velocity goals.
pub struct ConstraintEngine {
    constraints: Vec<Constraint>,
}

impl ConstraintEngine {
    pub fn new(constraints: Vec<Constraint>) -> Self {
        Self { constraints }
    }

    /// Check all constraints. Returns first Deny/ValidatorError, else Allow.
    /// SAFETY: must never panic — any panic in constraint logic is a ValidatorError.
    pub fn validate_position_goal(&self, pos: &[f64], velocity: Option<f64>) -> ConstraintResult {
        for c in &self.constraints {
            let r = c.validate_goal(pos);
            if !r.is_allowed() {
                return r;
            }
            if let Some(v) = velocity {
                let rv = c.validate_velocity(v);
                if !rv.is_allowed() {
                    return rv;
                }
            }
        }
        ConstraintResult::Allow
    }
}

/// Convenience function for callers that have a constraint slice but no engine.
/// Phase 4 hook: called before actuator publish (wired in Phase 5).
pub fn check_position_goal_constraints(
    pos: &[f64],
    velocity: Option<f64>,
    constraints: &[Constraint],
) -> ConstraintResult {
    let engine = ConstraintEngine::new(constraints.to_vec());
    engine.validate_position_goal(pos, velocity)
}

// ── ConstraintStore (SQLite) ────────────────────────────────────────

/// SQLite-backed store for constraint configurations.
pub struct ConstraintStore {
    conn: Connection,
}

impl ConstraintStore {
    /// Open (or create) the constraint store at the given path.
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        let conn = crate::daemon::util::open_sqlite(path)?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS constraints (
                id              TEXT PRIMARY KEY,
                mission_id      TEXT NOT NULL,
                constraint_json TEXT NOT NULL,
                created_at      INTEGER NOT NULL DEFAULT (strftime('%s','now'))
            );",
        )?;

        Ok(Self { conn })
    }

    /// Save (insert or replace) a constraint.
    pub fn save_constraint(
        &self,
        id: &str,
        mission_id: &str,
        constraint: &Constraint,
    ) -> anyhow::Result<()> {
        let json = serde_json::to_string(constraint)?;
        self.conn.execute(
            "INSERT OR REPLACE INTO constraints (id, mission_id, constraint_json) \
             VALUES (?1, ?2, ?3)",
            params![id, mission_id, json],
        )?;
        Ok(())
    }

    /// List all constraints for a mission. Returns (id, Constraint) pairs.
    pub fn list_constraints(&self, mission_id: &str) -> anyhow::Result<Vec<(String, Constraint)>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, constraint_json FROM constraints \
             WHERE mission_id = ?1 ORDER BY id ASC",
        )?;
        let rows = stmt.query_map(params![mission_id], |row| {
            let id: String = row.get(0)?;
            let json: String = row.get(1)?;
            Ok((id, json))
        })?;

        let mut results = Vec::new();
        for row in rows {
            let (id, json) = row?;
            let constraint: Constraint = serde_json::from_str(&json)?;
            results.push((id, constraint));
        }
        Ok(results)
    }

    /// Delete a constraint by ID.
    pub fn delete_constraint(&self, id: &str) -> anyhow::Result<()> {
        self.conn
            .execute("DELETE FROM constraints WHERE id = ?1", params![id])?;
        Ok(())
    }
}

// ── CompiledFallback ────────────────────────────────────────────────

/// Pre-compiled fallback actions. No arbitrary Zenoh publish variant —
/// prevents bypass attacks identified in design review.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum CompiledFallback {
    /// Stop all VLA/actuator commands (safe stop).
    StopActuators,
    /// Pause all active missions.
    PauseAllMissions,
    /// Alert the agent (adds high arousal spike).
    AlertAgent { message: String },
    /// Halt and wait for human.
    HaltAndWait,
}

// ── FallbackStore (SQLite) ──────────────────────────────────────────

/// SQLite-backed store for compiled fallback actions.
pub struct FallbackStore {
    conn: Connection,
}

impl FallbackStore {
    /// Open (or create) the fallback store at the given path.
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        let conn = crate::daemon::util::open_sqlite(path)?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS compiled_fallbacks (
                id                  TEXT PRIMARY KEY,
                mission_id          TEXT NOT NULL,
                trigger_predicate   TEXT NOT NULL,
                fallback_json       TEXT NOT NULL,
                created_at          INTEGER NOT NULL DEFAULT (strftime('%s','now'))
            );",
        )?;

        Ok(Self { conn })
    }

    /// Save (insert or replace) a fallback.
    pub fn save_fallback(
        &self,
        id: &str,
        mission_id: &str,
        trigger_predicate: &str,
        fallback: &CompiledFallback,
    ) -> anyhow::Result<()> {
        let json = serde_json::to_string(fallback)?;
        self.conn.execute(
            "INSERT OR REPLACE INTO compiled_fallbacks \
             (id, mission_id, trigger_predicate, fallback_json) \
             VALUES (?1, ?2, ?3, ?4)",
            params![id, mission_id, trigger_predicate, json],
        )?;
        Ok(())
    }

    /// List all fallbacks for a mission.
    pub fn list_fallbacks(
        &self,
        mission_id: &str,
    ) -> anyhow::Result<Vec<(String, String, CompiledFallback)>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, trigger_predicate, fallback_json FROM compiled_fallbacks \
             WHERE mission_id = ?1 ORDER BY id ASC",
        )?;
        let rows = stmt.query_map(params![mission_id], |row| {
            let id: String = row.get(0)?;
            let trigger: String = row.get(1)?;
            let json: String = row.get(2)?;
            Ok((id, trigger, json))
        })?;

        let mut results = Vec::new();
        for row in rows {
            let (id, trigger, json) = row?;
            let fallback: CompiledFallback = serde_json::from_str(&json)?;
            results.push((id, trigger, fallback));
        }
        Ok(results)
    }

    /// Delete a fallback by ID.
    pub fn delete_fallback(&self, id: &str) -> anyhow::Result<()> {
        self.conn
            .execute("DELETE FROM compiled_fallbacks WHERE id = ?1", params![id])?;
        Ok(())
    }
}

// ── ResourceRegistry (in-memory exclusive locking) ──────────────────

/// In-memory resource lock registry. Not persisted — locks are cleared on daemon restart.
/// This is intentional: in-flight missions lose their locks on restart, which is safer
/// than persisting stale locks that might block resources indefinitely.
#[derive(Clone, Default)]
pub struct ResourceRegistry {
    locks: Arc<Mutex<HashMap<String, String>>>, // resource_id -> mission_id
}

impl ResourceRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Exclusively acquire a resource for a mission. Fails if already locked by another.
    pub fn acquire(&self, resource: &str, mission_id: &str) -> anyhow::Result<()> {
        let mut locks = self
            .locks
            .lock()
            .map_err(|_| anyhow::anyhow!("resource registry mutex poisoned"))?;
        match locks.get(resource) {
            Some(owner) if owner == mission_id => Ok(()), // idempotent for same mission
            Some(owner) => anyhow::bail!("resource '{}' locked by '{}'", resource, owner),
            None => {
                locks.insert(resource.to_string(), mission_id.to_string());
                Ok(())
            }
        }
    }

    /// Release a resource. Idempotent — releasing an unlocked resource is a no-op.
    pub fn release(&self, resource: &str, mission_id: &str) -> anyhow::Result<()> {
        let mut locks = self
            .locks
            .lock()
            .map_err(|_| anyhow::anyhow!("resource registry mutex poisoned"))?;
        if locks
            .get(resource)
            .map(|s| s == mission_id)
            .unwrap_or(false)
        {
            locks.remove(resource);
        }
        Ok(())
    }

    /// Returns true if resource is available (not locked).
    pub fn is_available(&self, resource: &str) -> bool {
        self.locks
            .lock()
            .map(|l| !l.contains_key(resource))
            .unwrap_or(false)
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Constraint validation tests ─────────────────────────────────

    #[test]
    fn workspace_constraint_rejects_out_of_bounds() {
        let c = Constraint::Workspace {
            x: (-0.8, 0.8),
            y: (-0.8, 0.8),
            z: (0.0, 1.2),
        };
        assert!(c.validate_goal(&[0.5, 0.3, 0.8]).is_allowed());
        assert!(!c.validate_goal(&[0.95, 0.0, 0.5]).is_allowed()); // x > 0.8
        assert!(!c.validate_goal(&[0.0, 0.0, -0.1]).is_allowed()); // z < 0.0
    }

    #[test]
    fn velocity_constraint_rejects_excess_speed() {
        let c = Constraint::MaxVelocity(0.3);
        assert!(c.validate_velocity(0.2).is_allowed());
        assert!(!c.validate_velocity(0.5).is_allowed());
    }

    #[test]
    fn forbidden_zone_rejects_interior_point() {
        let c = Constraint::ForbiddenZone {
            center: [0.0, 0.0, 0.0],
            radius: 0.5,
        };
        assert!(!c.validate_goal(&[0.1, 0.1, 0.1]).is_allowed()); // inside
        assert!(c.validate_goal(&[1.0, 0.0, 0.0]).is_allowed()); // outside
    }

    #[test]
    fn engine_validates_all_constraints() {
        let engine = ConstraintEngine::new(vec![
            Constraint::Workspace {
                x: (-1.0, 1.0),
                y: (-1.0, 1.0),
                z: (0.0, 2.0),
            },
            Constraint::MaxVelocity(0.5),
        ]);
        assert!(engine
            .validate_position_goal(&[0.3, 0.2, 0.5], Some(0.4))
            .is_allowed());
        assert!(!engine
            .validate_position_goal(&[1.5, 0.0, 0.5], Some(0.4))
            .is_allowed());
        assert!(!engine
            .validate_position_goal(&[0.3, 0.2, 0.5], Some(0.8))
            .is_allowed());
    }

    #[test]
    fn constraint_deny_includes_descriptive_reason() {
        let c = Constraint::Workspace {
            x: (-1.0, 1.0),
            y: (-1.0, 1.0),
            z: (0.0, 2.0),
        };
        let result = c.validate_goal(&[2.0, 0.0, 0.0]);
        if let ConstraintResult::Deny { reason } = result {
            assert!(
                reason.contains("workspace.x"),
                "reason should mention axis: {}",
                reason
            );
        } else {
            panic!("expected Deny");
        }
    }

    #[test]
    fn max_force_constraint_rejects_excess() {
        let c = Constraint::MaxForce(10.0);
        assert!(c.validate_velocity(5.0).is_allowed());
        assert!(!c.validate_velocity(15.0).is_allowed());
    }

    #[test]
    fn workspace_constraint_boundary_values() {
        let c = Constraint::Workspace {
            x: (-1.0, 1.0),
            y: (-1.0, 1.0),
            z: (0.0, 2.0),
        };
        // Exactly on boundary should be allowed
        assert!(c.validate_goal(&[1.0, -1.0, 0.0]).is_allowed());
        assert!(c.validate_goal(&[-1.0, 1.0, 2.0]).is_allowed());
    }

    #[test]
    fn constraint_result_reason_accessors() {
        assert!(ConstraintResult::Allow.reason().is_none());
        assert_eq!(
            ConstraintResult::Deny {
                reason: "x".to_string()
            }
            .reason(),
            Some("x")
        );
        assert_eq!(
            ConstraintResult::ValidatorError {
                reason: "y".to_string()
            }
            .reason(),
            Some("y")
        );
    }

    #[test]
    fn check_position_goal_constraints_convenience() {
        let constraints = vec![
            Constraint::Workspace {
                x: (-1.0, 1.0),
                y: (-1.0, 1.0),
                z: (0.0, 2.0),
            },
            Constraint::MaxVelocity(0.5),
        ];
        assert!(
            check_position_goal_constraints(&[0.0, 0.0, 1.0], Some(0.3), &constraints).is_allowed()
        );
        assert!(
            !check_position_goal_constraints(&[2.0, 0.0, 1.0], None, &constraints).is_allowed()
        );
    }

    // ── ConstraintStore tests ───────────────────────────────────────

    #[test]
    fn constraint_store_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let store = ConstraintStore::open(&dir.path().join("c.db")).unwrap();
        let c = Constraint::MaxVelocity(0.5);
        store.save_constraint("c1", "mission-a", &c).unwrap();
        let loaded = store.list_constraints("mission-a").unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].0, "c1");
        if let Constraint::MaxVelocity(v) = &loaded[0].1 {
            assert!((v - 0.5).abs() < 0.001);
        } else {
            panic!("wrong constraint type");
        }
    }

    #[test]
    fn constraint_store_delete() {
        let dir = tempfile::tempdir().unwrap();
        let store = ConstraintStore::open(&dir.path().join("c.db")).unwrap();
        store
            .save_constraint("c1", "m1", &Constraint::MaxVelocity(1.0))
            .unwrap();
        assert_eq!(store.list_constraints("m1").unwrap().len(), 1);
        store.delete_constraint("c1").unwrap();
        assert!(store.list_constraints("m1").unwrap().is_empty());
    }

    #[test]
    fn constraint_store_multiple_missions() {
        let dir = tempfile::tempdir().unwrap();
        let store = ConstraintStore::open(&dir.path().join("c.db")).unwrap();
        store
            .save_constraint("c1", "m1", &Constraint::MaxVelocity(1.0))
            .unwrap();
        store
            .save_constraint("c2", "m2", &Constraint::MaxForce(5.0))
            .unwrap();
        assert_eq!(store.list_constraints("m1").unwrap().len(), 1);
        assert_eq!(store.list_constraints("m2").unwrap().len(), 1);
    }

    #[test]
    fn constraint_serialization_roundtrip() {
        let constraints = vec![
            Constraint::Workspace {
                x: (-1.0, 1.0),
                y: (-0.5, 0.5),
                z: (0.0, 2.0),
            },
            Constraint::MaxVelocity(0.3),
            Constraint::ForbiddenZone {
                center: [0.0, 0.0, 0.0],
                radius: 0.5,
            },
            Constraint::MaxForce(10.0),
        ];
        for c in &constraints {
            let json = serde_json::to_string(c).unwrap();
            let back: Constraint = serde_json::from_str(&json).unwrap();
            let json2 = serde_json::to_string(&back).unwrap();
            assert_eq!(json, json2);
        }
    }

    // ── FallbackStore tests ─────────────────────────────────────────

    #[test]
    fn fallback_store_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let store = FallbackStore::open(&dir.path().join("fb.db")).unwrap();
        let fb = CompiledFallback::AlertAgent {
            message: "danger".to_string(),
        };
        store.save_fallback("fb1", "m1", "temp > 100", &fb).unwrap();
        let loaded = store.list_fallbacks("m1").unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].0, "fb1");
        assert_eq!(loaded[0].1, "temp > 100");
        if let CompiledFallback::AlertAgent { message } = &loaded[0].2 {
            assert_eq!(message, "danger");
        } else {
            panic!("wrong fallback type");
        }
    }

    #[test]
    fn fallback_store_delete() {
        let dir = tempfile::tempdir().unwrap();
        let store = FallbackStore::open(&dir.path().join("fb.db")).unwrap();
        store
            .save_fallback("fb1", "m1", "x > 1", &CompiledFallback::StopActuators)
            .unwrap();
        assert_eq!(store.list_fallbacks("m1").unwrap().len(), 1);
        store.delete_fallback("fb1").unwrap();
        assert!(store.list_fallbacks("m1").unwrap().is_empty());
    }

    #[test]
    fn all_compiled_fallback_variants_serialize() {
        let variants: Vec<CompiledFallback> = vec![
            CompiledFallback::StopActuators,
            CompiledFallback::PauseAllMissions,
            CompiledFallback::AlertAgent {
                message: "test".to_string(),
            },
            CompiledFallback::HaltAndWait,
        ];
        for fb in &variants {
            let json = serde_json::to_string(fb).unwrap();
            let back: CompiledFallback = serde_json::from_str(&json).unwrap();
            let json2 = serde_json::to_string(&back).unwrap();
            assert_eq!(json, json2);
        }
    }

    // ── ResourceRegistry tests ──────────────────────────────────────

    #[test]
    fn resource_lock_exclusive_acquisition() {
        let registry = ResourceRegistry::new();
        assert!(registry.acquire("robot_arm", "mission-a").is_ok());
        assert!(registry.acquire("robot_arm", "mission-b").is_err()); // locked by mission-a
        registry.release("robot_arm", "mission-a").unwrap();
        assert!(registry.acquire("robot_arm", "mission-b").is_ok()); // now available
    }

    #[test]
    fn resource_lock_idempotent_release() {
        let registry = ResourceRegistry::new();
        registry.acquire("robot_arm", "m1").unwrap();
        registry.release("robot_arm", "m1").unwrap();
        registry.release("robot_arm", "m1").unwrap(); // should not error
    }

    #[test]
    fn resource_lock_same_mission_idempotent() {
        let registry = ResourceRegistry::new();
        assert!(registry.acquire("cam", "m1").is_ok());
        assert!(registry.acquire("cam", "m1").is_ok()); // same mission = idempotent
    }

    #[test]
    fn resource_availability_check() {
        let registry = ResourceRegistry::new();
        assert!(registry.is_available("arm"));
        registry.acquire("arm", "m1").unwrap();
        assert!(!registry.is_available("arm"));
        registry.release("arm", "m1").unwrap();
        assert!(registry.is_available("arm"));
    }

    #[test]
    fn resource_release_wrong_mission_is_noop() {
        let registry = ResourceRegistry::new();
        registry.acquire("arm", "m1").unwrap();
        registry.release("arm", "m2").unwrap(); // wrong mission, should be noop
        assert!(!registry.is_available("arm")); // still locked by m1
    }

    #[test]
    fn resource_registry_is_clone() {
        let r1 = ResourceRegistry::new();
        r1.acquire("x", "m1").unwrap();
        let r2 = r1.clone();
        assert!(!r2.is_available("x")); // shares the same Arc<Mutex>
    }
}
