//! Belief update engine — connects world state observations to belief tracking.
//!
//! When a context provider writes `dog.eats_at = "08:00"` to world state,
//! the belief updater checks if there's a belief (subject="dog", predicate="eats_at")
//! and calls confirm_belief or contradict_belief based on value match.
//!
//! This runs synchronously after each world state write (no background task needed).

use crate::agent::memory::semantic::SemanticStore;

/// Outcome of comparing an observation against an existing belief.
#[derive(Debug, PartialEq)]
pub enum ObservationOutcome {
    /// Observed value matches or is contained in the belief value.
    Confirms,
    /// Partial token overlap — neither confirms nor contradicts.
    Neutral,
    /// No overlap at all — the observation contradicts the belief.
    Contradicts,
}

/// Determine if a new observation confirms or contradicts an existing belief.
///
/// Confirmation: observed value is a substring of (or equals) belief value,
///   or belief value is a substring of observed value.
/// Contradiction: values share no common tokens.
pub fn evaluate_observation_vs_belief(
    belief: &crate::agent::memory::semantic::Belief,
    observed_value: &str,
) -> ObservationOutcome {
    let belief_val = belief.value.to_lowercase();
    let obs_val = observed_value.to_lowercase();

    if belief_val == obs_val || belief_val.contains(&obs_val) || obs_val.contains(&belief_val) {
        return ObservationOutcome::Confirms;
    }

    // Simple heuristic: check if ANY token from belief appears in observation
    let belief_tokens: std::collections::HashSet<&str> = belief_val.split_whitespace().collect();
    let obs_tokens: std::collections::HashSet<&str> = obs_val.split_whitespace().collect();
    if !belief_tokens.is_disjoint(&obs_tokens) {
        ObservationOutcome::Neutral // partial overlap, neither confirm nor contradict
    } else {
        ObservationOutcome::Contradicts
    }
}

/// Update beliefs based on a new world state observation.
///
/// Splits `world_state_key` on the first `.` to extract (subject, predicate).
/// If a matching belief exists, confirms or contradicts it based on value comparison.
pub fn update_beliefs_from_observation(
    store: &SemanticStore,
    world_state_key: &str,
    observed_value: &str,
) -> anyhow::Result<()> {
    // Split "dog.eats_at" → subject="dog", predicate="eats_at"
    let parts: Vec<&str> = world_state_key.splitn(2, '.').collect();
    if parts.len() != 2 {
        return Ok(()); // key not in subject.predicate format
    }

    let subject = parts[0];
    let predicate = parts[1];

    let belief = store.get_belief(subject, predicate)?;
    let Some(belief) = belief else {
        return Ok(());
    }; // no belief for this key

    match evaluate_observation_vs_belief(&belief, observed_value) {
        ObservationOutcome::Confirms => {
            store.confirm_belief(subject, predicate)?;
            log::debug!(
                "[BeliefUpdater] confirmed ({},{}) value={}",
                subject,
                predicate,
                observed_value
            );
        }
        ObservationOutcome::Contradicts => {
            store.contradict_belief(subject, predicate)?;
            log::debug!(
                "[BeliefUpdater] contradicted ({},{}) value={}",
                subject,
                predicate,
                observed_value
            );
        }
        ObservationOutcome::Neutral => {
            log::debug!("[BeliefUpdater] neutral for ({},{})", subject, predicate);
        }
    }
    Ok(())
}

/// Spawn a periodic belief decay task that runs every `interval_secs` seconds.
///
/// Calls `SemanticStore::decay_beliefs(decay_factor)` to reduce all belief confidences.
///
/// Decay factor: 0.99 = 1% decay per interval (gentle for daily patterns)
///               0.95 = 5% decay per interval (aggressive for fast-changing state)
pub async fn spawn_belief_decay_task(
    db_path: std::path::PathBuf,
    decay_factor: f64,
    interval_secs: u64,
    mut shutdown: tokio::sync::watch::Receiver<()>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
        loop {
            tokio::select! {
                _ = shutdown.changed() => {
                    log::info!("[BeliefDecay] shutting down");
                    break;
                }
                _ = interval.tick() => {
                    let result = tokio::task::block_in_place(|| {
                        let store = SemanticStore::open(&db_path)?;
                        store.decay_beliefs(decay_factor).map_err(|e| anyhow::anyhow!("{}", e))
                    });
                    match result {
                        Ok(n) => log::debug!("[BeliefDecay] decayed {} beliefs", n),
                        Err(e) => log::warn!("[BeliefDecay] error: {}", e),
                    }
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::memory::semantic::Belief;

    /// Create a minimal Belief for unit testing `evaluate_observation_vs_belief`.
    fn make_test_belief(value: &str) -> Belief {
        Belief {
            id: "test-id".to_string(),
            subject: "test".to_string(),
            predicate: "test".to_string(),
            value: value.to_string(),
            confidence: 0.9,
            source: "test".to_string(),
            first_observed: 0,
            last_confirmed: 0,
            confirmation_count: 0,
            contradiction_count: 0,
            notes: None,
        }
    }

    #[test]
    fn observation_confirms_matching_belief() {
        let dir = tempfile::tempdir().unwrap();
        let store = SemanticStore::open(&dir.path().join("t.db")).unwrap();
        store
            .upsert_belief("b1", "dog", "eats_at", "08:00,18:00", 0.8, "prior", None)
            .unwrap();

        let b_before = store.get_belief("dog", "eats_at").unwrap().unwrap();

        update_beliefs_from_observation(&store, "dog.eats_at", "08:00").unwrap();

        let b_after = store.get_belief("dog", "eats_at").unwrap().unwrap();
        assert!(b_after.confirmation_count > b_before.confirmation_count);
    }

    #[test]
    fn observation_contradicts_mismatched_belief() {
        let dir = tempfile::tempdir().unwrap();
        let store = SemanticStore::open(&dir.path().join("t.db")).unwrap();
        store
            .upsert_belief("b1", "dog", "location", "kitchen", 0.9, "prior", None)
            .unwrap();

        update_beliefs_from_observation(&store, "dog.location", "bedroom").unwrap();

        let b = store.get_belief("dog", "location").unwrap().unwrap();
        assert!(b.contradiction_count > 0);
        assert!(b.confidence < 0.9);
    }

    #[test]
    fn observation_neutral_for_partial_overlap() {
        // "kitchen counter" vs "kitchen area" → has "kitchen" token in common → Neutral
        let outcome =
            evaluate_observation_vs_belief(&make_test_belief("kitchen counter"), "kitchen area");
        assert_eq!(outcome, ObservationOutcome::Neutral);
    }

    #[test]
    fn non_dotted_key_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let store = SemanticStore::open(&dir.path().join("t.db")).unwrap();
        // Should not error on keys without '.'
        update_beliefs_from_observation(&store, "nodot", "value").unwrap();
    }

    #[test]
    fn exact_match_confirms() {
        let outcome = evaluate_observation_vs_belief(&make_test_belief("online"), "online");
        assert_eq!(outcome, ObservationOutcome::Confirms);
    }

    #[test]
    fn substring_confirms() {
        let outcome = evaluate_observation_vs_belief(&make_test_belief("08:00,18:00"), "08:00");
        assert_eq!(outcome, ObservationOutcome::Confirms);
    }

    #[test]
    fn completely_different_contradicts() {
        let outcome = evaluate_observation_vs_belief(&make_test_belief("kitchen"), "bedroom");
        assert_eq!(outcome, ObservationOutcome::Contradicts);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn belief_decay_task_reduces_confidence() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("mem.db");
        let store = SemanticStore::open(&db).unwrap();
        store
            .upsert_belief("b1", "x", "y", "z", 1.0, "test", None)
            .unwrap();
        drop(store);

        let (_tx, rx) = tokio::sync::watch::channel(());
        let handle = spawn_belief_decay_task(db.clone(), 0.9, 1, rx).await;

        // Wait for one decay cycle
        tokio::time::sleep(std::time::Duration::from_millis(1200)).await;

        let store2 = SemanticStore::open(&db).unwrap();
        let b = store2.get_belief("x", "y").unwrap().unwrap();
        assert!(b.confidence < 1.0, "confidence should have decayed");

        handle.abort();
    }
}
