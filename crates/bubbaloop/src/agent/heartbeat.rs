//! Adaptive heartbeat — autonomic nervous system inspired loop.
//!
//! The heartbeat adjusts its interval based on system arousal:
//! - Resting: ~60s (nothing happening)
//! - Aroused: ~5s (events detected, user active)
//! - Recovery: exponential decay back to resting (0.7x per calm beat)

use crate::agent::soul::Capabilities;

/// Upper bound on arousal. Any spike, boost, or source push is clamped to this.
///
/// The heartbeat interval is already clamped to `min_interval` via
/// `interval_secs()`, so unbounded arousal growth has no effect on tick rate.
/// But a persistently-true reactive rule could otherwise push arousal up by
/// its `arousal_boost` every heartbeat forever — leaving `is_at_rest()` stuck
/// at `false` long after the triggering condition clears, because decay has
/// to work through an arbitrarily large value. Capping prevents that drift.
pub const MAX_AROUSAL: f64 = 20.0;

/// Arousal state for the adaptive heartbeat.
#[derive(Debug, Clone)]
pub struct ArousalState {
    /// Current arousal level (0.0 = rest, higher = more alert).
    arousal: f64,
    /// Base interval in seconds (from capabilities).
    base_interval: u64,
    /// Minimum interval in seconds (from capabilities).
    min_interval: u64,
    /// Decay factor per calm beat (from capabilities).
    decay_factor: f64,
}

/// Sources that can spike arousal.
#[derive(Debug, Clone, Copy)]
pub enum ArousalSource {
    /// Node health status changed (e.g., healthy → unhealthy).
    NodeHealthChange,
    /// Node crashed or was restarted.
    NodeCrashRestart,
    /// User sent REPL input.
    UserInput,
    /// A pending job fired.
    PendingJobFired,
}

impl ArousalSource {
    /// Arousal boost value for this source.
    pub fn boost(&self) -> f64 {
        match self {
            ArousalSource::NodeHealthChange => 2.0,
            ArousalSource::NodeCrashRestart => 3.0,
            ArousalSource::UserInput => 2.0,
            ArousalSource::PendingJobFired => 1.0,
        }
    }
}

/// Collected system state from a heartbeat tick.
#[derive(Debug, Clone, Default)]
pub struct HeartbeatState {
    /// Number of events detected this tick.
    pub event_count: usize,
    /// Whether any state changed since last tick.
    pub has_changes: bool,
    /// Arousal sources detected.
    pub sources: Vec<ArousalSource>,
}

impl ArousalState {
    /// Create a new arousal state from capabilities.
    pub fn new(caps: &Capabilities) -> Self {
        Self {
            arousal: 0.0,
            base_interval: caps.heartbeat_base_interval,
            min_interval: caps.heartbeat_min_interval,
            decay_factor: caps.heartbeat_decay_factor,
        }
    }

    /// Calculate the current heartbeat interval in seconds.
    ///
    /// Formula: `max(base / (1.0 + arousal), min)`
    pub fn interval_secs(&self) -> u64 {
        let interval = self.base_interval as f64 / (1.0 + self.arousal);
        let interval = interval.max(self.min_interval as f64);
        interval as u64
    }

    /// Update arousal based on collected state.
    ///
    /// If events occurred, spike arousal proportional to their boosts.
    /// If no events, decay toward rest.
    pub fn update(&mut self, state: &HeartbeatState) {
        if state.sources.is_empty() {
            // Decay toward rest
            self.arousal *= self.decay_factor;
            // Snap to zero to avoid floating point drift
            if self.arousal < 0.01 {
                self.arousal = 0.0;
            }
        } else {
            // Spike proportional to event sources
            for source in &state.sources {
                self.arousal += source.boost();
            }
            self.arousal = self.arousal.min(MAX_AROUSAL);
        }
    }

    /// Spike arousal from a specific source (e.g., user input).
    pub fn spike(&mut self, source: ArousalSource) {
        self.arousal = (self.arousal + source.boost()).min(MAX_AROUSAL);
    }

    /// Add an external arousal boost (e.g. from reactive rules).
    ///
    /// Phase 3: ReactiveRule arousal integration -- called by the daemon
    /// when reactive rules fire against the current world state. Clamped
    /// to [`MAX_AROUSAL`] so a persistently-true predicate cannot push
    /// arousal unboundedly upward.
    pub fn add_external_boost(&mut self, boost: f64) {
        log::debug!("[Heartbeat] external arousal boost: {:.2}", boost);
        self.arousal = (self.arousal + boost).min(MAX_AROUSAL);
    }

    /// Get the current arousal level.
    pub fn arousal(&self) -> f64 {
        self.arousal
    }

    /// Check if the agent is at rest (arousal == 0).
    pub fn is_at_rest(&self) -> bool {
        self.arousal == 0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_caps() -> Capabilities {
        Capabilities {
            model_name: "test".to_string(),
            max_turns: 15,
            allow_internet: true,
            heartbeat_base_interval: 60,
            heartbeat_min_interval: 5,
            heartbeat_decay_factor: 0.7,
            default_approval_mode: "auto".to_string(),
            max_retries: 3,
            compaction_flush_threshold_tokens: 4000,
            episodic_log_retention_days: 30,
            episodic_decay_half_life_days: 30,
        }
    }

    #[test]
    fn resting_interval() {
        let state = ArousalState::new(&test_caps());
        assert_eq!(state.interval_secs(), 60);
        assert!(state.is_at_rest());
    }

    #[test]
    fn aroused_interval() {
        let mut state = ArousalState::new(&test_caps());
        state.spike(ArousalSource::NodeCrashRestart); // +3.0
                                                      // interval = 60 / (1 + 3) = 15
        assert_eq!(state.interval_secs(), 15);
        assert!(!state.is_at_rest());
    }

    #[test]
    fn high_arousal_clamps_to_min() {
        let mut state = ArousalState::new(&test_caps());
        state.arousal = 100.0;
        // 60 / 101 = 0.59... → clamped to min 5
        assert_eq!(state.interval_secs(), 5);
    }

    #[test]
    fn decay_toward_rest() {
        let mut state = ArousalState::new(&test_caps());
        state.arousal = 10.0;

        let empty = HeartbeatState::default();
        state.update(&empty);
        assert!((state.arousal - 7.0).abs() < 0.01); // 10 * 0.7

        state.update(&empty);
        assert!((state.arousal - 4.9).abs() < 0.01); // 7 * 0.7

        state.update(&empty);
        assert!((state.arousal - 3.43).abs() < 0.01); // 4.9 * 0.7
    }

    #[test]
    fn snap_to_zero() {
        let mut state = ArousalState::new(&test_caps());
        state.arousal = 0.005; // below threshold
        state.update(&HeartbeatState::default());
        assert_eq!(state.arousal, 0.0);
        assert!(state.is_at_rest());
    }

    #[test]
    fn spike_from_events() {
        let mut state = ArousalState::new(&test_caps());

        let hb_state = HeartbeatState {
            event_count: 2,
            has_changes: true,
            sources: vec![
                ArousalSource::NodeHealthChange, // +2.0
                ArousalSource::PendingJobFired,  // +1.0
            ],
        };
        state.update(&hb_state);
        assert!((state.arousal - 3.0).abs() < 0.01);
    }

    #[test]
    fn user_input_spike() {
        let mut state = ArousalState::new(&test_caps());
        state.spike(ArousalSource::UserInput);
        assert!((state.arousal - 2.0).abs() < 0.01);
        // interval = 60 / (1 + 2) = 20
        assert_eq!(state.interval_secs(), 20);
    }

    #[test]
    fn arousal_source_boosts() {
        assert_eq!(ArousalSource::NodeHealthChange.boost(), 2.0);
        assert_eq!(ArousalSource::NodeCrashRestart.boost(), 3.0);
        assert_eq!(ArousalSource::UserInput.boost(), 2.0);
        assert_eq!(ArousalSource::PendingJobFired.boost(), 1.0);
    }

    #[test]
    fn external_boost_clamps_to_max() {
        let mut state = ArousalState::new(&test_caps());
        // Simulate a persistently-true reactive rule firing many times.
        for _ in 0..1000 {
            state.add_external_boost(5.0);
        }
        assert!(
            (state.arousal() - MAX_AROUSAL).abs() < f64::EPSILON,
            "arousal should clamp to MAX_AROUSAL, got {}",
            state.arousal()
        );
    }

    #[test]
    fn spike_clamps_to_max() {
        let mut state = ArousalState::new(&test_caps());
        for _ in 0..20 {
            state.spike(ArousalSource::NodeCrashRestart); // +3.0 each
        }
        assert!(state.arousal() <= MAX_AROUSAL);
    }

    #[test]
    fn update_with_sources_clamps_to_max() {
        let mut state = ArousalState::new(&test_caps());
        state.arousal = MAX_AROUSAL - 1.0;
        state.update(&HeartbeatState {
            event_count: 1,
            has_changes: true,
            sources: vec![ArousalSource::NodeCrashRestart], // +3.0
        });
        assert!((state.arousal() - MAX_AROUSAL).abs() < f64::EPSILON);
    }

    #[test]
    fn external_boost_adds_arousal() {
        let mut state = ArousalState::new(&test_caps());
        assert!(state.is_at_rest());
        state.add_external_boost(2.5);
        assert!((state.arousal() - 2.5).abs() < 0.01);
        assert!(!state.is_at_rest());
        // interval = 60 / (1 + 2.5) = ~17
        assert_eq!(state.interval_secs(), 17);
    }

    #[test]
    fn cost_model_resting() {
        // At rest: 1 beat per 60s = 1440 beats/day
        let state = ArousalState::new(&test_caps());
        let beats_per_day = 86400 / state.interval_secs();
        assert_eq!(beats_per_day, 1440);
    }

    #[test]
    fn cost_model_aroused() {
        // At max arousal (arousal=11): interval=5s, 12 beats/min, 720/hour
        let mut state = ArousalState::new(&test_caps());
        state.arousal = 11.0;
        assert_eq!(state.interval_secs(), 5); // clamped to min
    }

    #[test]
    fn full_decay_cycle() {
        let mut state = ArousalState::new(&test_caps());
        state.arousal = 3.0; // Node crash

        let empty = HeartbeatState::default();
        let mut beats = 0;
        while !state.is_at_rest() {
            state.update(&empty);
            beats += 1;
            assert!(beats < 100, "should reach rest in < 100 beats");
        }
        // 3.0 → 2.1 → 1.47 → 1.03 → 0.72 → 0.50 → 0.35 → 0.25 → 0.17 → 0.12 → 0.08 → 0.06 → 0.04 → 0.03 → 0.02 → 0.01 → snap
        assert!(
            beats > 10 && beats < 25,
            "expected ~16 beats, got {}",
            beats
        );
    }
}
