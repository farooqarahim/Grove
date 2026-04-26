use std::collections::HashSet;

use crate::db::repositories::conversations_lru::SweepCandidate;

#[derive(Debug, Clone, Copy)]
pub struct SweepConfig {
    pub max_cached: usize,
    pub max_idle_secs: i64,
}

/// Pure, deterministic eviction selection.
///
/// Rules (in order of precedence):
///   1. `pinned` is never evicted.
///   2. `in_flight` ids are never evicted.
///   3. Age > `max_idle_secs` → evict.
///   4. If remaining (unpinned, not-in-flight, fresh-enough) count exceeds
///      `max_cached`, evict the oldest extras by `(last_access_at, id)`.
///
/// Tie-break: ascending by `last_access_at`, then ascending by `id`.
pub fn select_evictions(
    candidates: &[SweepCandidate],
    cfg: &SweepConfig,
    now: i64,
    in_flight: &HashSet<String>,
) -> Vec<String> {
    let mut keep: Vec<&SweepCandidate> = Vec::new();
    let mut evict: Vec<String> = Vec::new();

    // Age pass.
    for c in candidates {
        if c.pinned || in_flight.contains(&c.id) {
            keep.push(c);
            continue;
        }
        if now.saturating_sub(c.last_access_at) > cfg.max_idle_secs {
            evict.push(c.id.clone());
        } else {
            keep.push(c);
        }
    }

    // Count-cap pass on survivors.
    if keep.len() > cfg.max_cached {
        let mut sortable: Vec<&SweepCandidate> = keep
            .iter()
            .filter(|c| !c.pinned && !in_flight.contains(&c.id))
            .copied()
            .collect();
        sortable.sort_by(|a, b| {
            a.last_access_at
                .cmp(&b.last_access_at)
                .then_with(|| a.id.cmp(&b.id))
        });
        // Number of survivors that are protected by pin or in_flight.
        let protected = keep.len() - sortable.len();
        // Survivors eligible for count-cap eviction.
        let eligible = sortable.len();
        // Target: keep at most cfg.max_cached total (protected count included).
        let allowed_eligible = cfg.max_cached.saturating_sub(protected);
        if eligible > allowed_eligible {
            let overflow = eligible - allowed_eligible;
            for c in sortable.iter().take(overflow) {
                evict.push(c.id.clone());
            }
        }
    }

    evict.sort();
    evict.dedup();
    evict
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(id: &str, last: i64, pinned: bool) -> SweepCandidate {
        SweepCandidate {
            id: id.to_string(),
            last_access_at: last,
            cached_size_bytes: None,
            pinned,
        }
    }

    fn cfg(max_cached: usize, max_idle_secs: i64) -> SweepConfig {
        SweepConfig {
            max_cached,
            max_idle_secs,
        }
    }

    #[test]
    fn empty_candidates_returns_empty() {
        let got = select_evictions(&[], &cfg(50, 3600), 1_000, &HashSet::new());
        assert!(got.is_empty());
    }

    #[test]
    fn all_fresh_within_age_and_count() {
        let cands = vec![c("a", 900, false), c("b", 950, false)];
        let got = select_evictions(&cands, &cfg(50, 3600), 1_000, &HashSet::new());
        assert!(got.is_empty());
    }

    #[test]
    fn idle_age_triggers_eviction() {
        let cands = vec![c("old", 0, false), c("new", 990, false)];
        let got = select_evictions(&cands, &cfg(50, 100), 1_000, &HashSet::new());
        assert_eq!(got, vec!["old".to_string()]);
    }

    #[test]
    fn count_cap_triggers_lru_tail() {
        let cands: Vec<SweepCandidate> = (0..60)
            .map(|i| c(&format!("c{:02}", i), 1_000 + i as i64, false))
            .collect();
        let got = select_evictions(&cands, &cfg(50, 1_000_000), 5_000, &HashSet::new());
        assert_eq!(got.len(), 10);
        for i in 0..10 {
            assert!(got.contains(&format!("c{:02}", i)), "missing c{:02}", i);
        }
    }

    #[test]
    fn pinned_never_evicted() {
        let cands = vec![c("keep", 0, true), c("drop", 0, false)];
        let got = select_evictions(&cands, &cfg(50, 100), 1_000, &HashSet::new());
        assert_eq!(got, vec!["drop".to_string()]);
    }

    #[test]
    fn in_flight_never_evicted() {
        let cands = vec![c("running", 0, false), c("drop", 0, false)];
        let mut in_flight = HashSet::new();
        in_flight.insert("running".to_string());
        let got = select_evictions(&cands, &cfg(50, 100), 1_000, &in_flight);
        assert_eq!(got, vec!["drop".to_string()]);
    }

    #[test]
    fn combined_age_and_count_union() {
        let mut cands = vec![
            c("ancient", 0, false),   // age-evict
            c("pinned_old", 0, true), // protected
        ];
        for i in 0..55 {
            cands.push(c(&format!("c{:02}", i), 10_000 + i as i64, false));
        }
        // max_cached=50, max_idle_secs=100, now=10_100.
        // ancient is age-evicted (age=10_100 > 100). pinned_old is protected.
        // 55 c* survive age pass (age <= 100 for all: c00 age=100, c54 age=46).
        // keep has 56 (55 c* + pinned_old). protected=1, eligible=55, allowed_eligible=49 → overflow=6.
        let got = select_evictions(&cands, &cfg(50, 100), 10_100, &HashSet::new());
        let mut expected = vec!["ancient".to_string()];
        for i in 0..6 {
            expected.push(format!("c{:02}", i));
        }
        expected.sort();
        assert_eq!(got, expected);
    }

    #[test]
    fn pinned_beats_age_beats_count_priority() {
        let cands = vec![
            c("pinned_ancient", 0, true), // pin wins over age
            c("aged", 0, false),          // age-evict
            c("fresh", 1_000, false),
        ];
        let got = select_evictions(&cands, &cfg(1, 100), 2_000, &HashSet::new());
        // aged is age-evicted. pinned_ancient protected. keep.len()==2, protected=1,
        // eligible=1 (fresh), allowed_eligible=0, overflow=1 → evict fresh too.
        let mut expected = vec!["aged".to_string(), "fresh".to_string()];
        expected.sort();
        assert_eq!(got, expected);
    }

    #[test]
    fn stable_tie_breaking_by_id() {
        let cands = vec![c("b", 100, false), c("a", 100, false), c("c", 100, false)];
        // max_cached=1 → overflow=2. Oldest 2 by (last, id) = a, b.
        let got = select_evictions(&cands, &cfg(1, 1_000_000), 200, &HashSet::new());
        assert_eq!(got, vec!["a".to_string(), "b".to_string()]);
    }
}
