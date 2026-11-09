//! Sources a permission gate recently refused, for the device page to report.

use std::collections::HashMap;
use std::hash::Hash;
use std::time::{Duration, Instant};

/// How long a refused source counts as asking.
pub(crate) const REQUEST_MEMORY: Duration = Duration::from_secs(30 * 60);
/// Sources remembered at once.
pub(crate) const MAX_SOURCES: usize = 256;

/// Each source's latest refusal, stamped when it last arrived.
pub(crate) struct Recent<K, V, const CAPACITY: usize = MAX_SOURCES>(HashMap<K, (V, Instant)>);

impl<K, V, const CAPACITY: usize> Default for Recent<K, V, CAPACITY> {
    fn default() -> Self {
        Self(HashMap::new())
    }
}

impl<K: Eq + Hash + Clone, V: Default, const CAPACITY: usize> Recent<K, V, CAPACITY> {
    /// The source's entry, stamped `now`. Past the cap, the stalest source is
    /// forgotten.
    pub(crate) fn touch(&mut self, key: K, now: Instant) -> &mut V {
        if !self.0.contains_key(&key) && self.0.len() >= CAPACITY {
            let stalest = self
                .0
                .iter()
                .min_by_key(|(_, (_, at))| *at)
                .map(|(k, _)| k.clone());
            if let Some(stalest) = stalest {
                self.0.remove(&stalest);
            }
        }
        let (value, at) = self.0.entry(key).or_insert_with(|| (V::default(), now));
        *at = now;
        value
    }
}

impl<K: Eq + Hash, V, const CAPACITY: usize> Recent<K, V, CAPACITY> {
    pub(crate) fn forget(&mut self, key: &K) {
        self.0.remove(key);
    }

    /// Entries stamped within the last [`REQUEST_MEMORY`].
    pub(crate) fn recent(&self, now: Instant) -> impl Iterator<Item = (&K, &V)> {
        self.0
            .iter()
            .filter(move |(_, (_, at))| now.duration_since(*at) < REQUEST_MEMORY)
            .map(|(k, (v, _))| (k, v))
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_lapse_after_the_memory() {
        let mut recent = Recent::<u8, u8>::default();
        let now = Instant::now();
        *recent.touch(1, now) = 7;
        assert_eq!(recent.recent(now).collect::<Vec<_>>(), vec![(&1, &7)]);
        assert_eq!(recent.recent(now + REQUEST_MEMORY).count(), 0);
        *recent.touch(1, now + REQUEST_MEMORY) += 1;
        assert_eq!(
            recent.recent(now + REQUEST_MEMORY).collect::<Vec<_>>(),
            vec![(&1, &8)],
            "a new arrival restamps the entry and keeps its value"
        );
        recent.forget(&1);
        assert_eq!(recent.len(), 0);
    }

    #[test]
    fn the_stalest_source_makes_room() {
        let mut recent = Recent::<usize, ()>::default();
        let start = Instant::now();
        for n in 0..=MAX_SOURCES {
            recent.touch(n, start + Duration::from_secs(n as u64));
        }
        assert_eq!(recent.len(), MAX_SOURCES);
        let at = start + Duration::from_secs(MAX_SOURCES as u64);
        assert!(recent.recent(at).all(|(n, _)| *n != 0), "oldest dropped");
    }
}
