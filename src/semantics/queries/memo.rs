//! Bounded analysis-local memo storage with explicit lifetime collection.
//!
//! Eviction only forgets recomputable answers. Authored contributions, pending
//! work, diagnostics and exported graph data never live in this cache. The byte
//! budget is an accounted payload estimate, not a process-RSS guarantee.
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Default)]
pub(super) struct Stats {
    pub hits: usize,
    pub misses: usize,
    pub evictions: usize,
    pub collected: usize,
    pub peak_bytes: usize,
    pub bytes: usize,
}

struct Entry<V> {
    value: V,
    bytes: usize,
    stamp: u64,
}

pub(super) struct Memo<V> {
    entries: BTreeMap<String, Entry<V>>,
    age: BTreeSet<(u64, String)>,
    clock: u64,
    budget: usize,
    pub stats: Stats,
}

impl<V> Default for Memo<V> {
    fn default() -> Self {
        Self::new(4 * 1024 * 1024)
    }
}

impl<V> Memo<V> {
    pub fn new(budget: usize) -> Self {
        Self {
            entries: BTreeMap::new(),
            age: BTreeSet::new(),
            clock: 0,
            budget,
            stats: Stats::default(),
        }
    }
    pub fn get(&mut self, key: &str) -> Option<&V> {
        let Some(entry) = self.entries.get_mut(key) else {
            self.stats.misses += 1;
            return None;
        };
        self.stats.hits += 1;
        self.age.remove(&(entry.stamp, key.to_owned()));
        self.clock += 1;
        entry.stamp = self.clock;
        self.age.insert((entry.stamp, key.to_owned()));
        Some(&entry.value)
    }
    /// Oversized results remain usable by the caller, but are never retained.
    /// Replacement drops the previous value before allocating cache ownership.
    pub fn insert(&mut self, key: String, value: V, payload_bytes: usize) {
        self.remove(&key);
        let bytes = payload_bytes
            .saturating_add(key.len().saturating_mul(2))
            .saturating_add(256);
        if bytes > self.budget {
            return;
        }
        while self.stats.bytes.saturating_add(bytes) > self.budget {
            let Some((_, oldest)) = self.age.first().cloned() else {
                break;
            };
            self.remove(&oldest);
            self.stats.evictions += 1;
        }
        self.clock += 1;
        self.age.insert((self.clock, key.clone()));
        self.entries.insert(
            key,
            Entry {
                value,
                bytes,
                stamp: self.clock,
            },
        );
        self.stats.bytes += bytes;
        self.stats.peak_bytes = self.stats.peak_bytes.max(self.stats.bytes);
    }
    pub fn collect(&mut self, dead: impl Fn(&V) -> bool) {
        let keys = self
            .entries
            .iter()
            .filter(|(_, entry)| dead(&entry.value))
            .map(|(key, _)| key.clone())
            .collect::<Vec<_>>();
        self.stats.collected += keys.len();
        for key in keys {
            self.remove(&key);
        }
    }
    fn remove(&mut self, key: &str) {
        if let Some(entry) = self.entries.remove(key) {
            self.age.remove(&(entry.stamp, key.to_owned()));
            self.stats.bytes -= entry.bytes;
        }
    }
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lru_budget_and_collection_release_payloads() {
        let mut cache = Memo::new(600);
        cache.insert("a".into(), "a", 20);
        cache.insert("b".into(), "b", 20);
        assert_eq!(cache.get("a"), Some(&"a"));
        cache.insert("c".into(), "c", 20);
        assert!(cache.get("b").is_none());
        assert_eq!(cache.stats.evictions, 1);
        cache.collect(|value| *value == "a");
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.stats.collected, 1);
        cache.insert("huge".into(), "huge", 1000);
        assert_eq!(cache.len(), 1);
        cache.collect(|_| true);
        assert_eq!(cache.stats.bytes, 0);
    }
}
