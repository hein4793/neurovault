// Cache helpers — kept for future use, not currently invoked.
#![allow(dead_code)]
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

struct CacheEntry<T> {
    value: T,
    expires_at: Instant,
}

pub struct BrainCache {
    stats: Mutex<Option<CacheEntry<String>>>,
    search: Mutex<HashMap<String, CacheEntry<String>>>,
}

impl BrainCache {
    pub fn new() -> Self {
        Self {
            stats: Mutex::new(None),
            search: Mutex::new(HashMap::new()),
        }
    }

    pub fn get_stats(&self) -> Option<String> {
        let guard = self.stats.lock().ok()?;
        guard.as_ref().and_then(|entry| {
            if Instant::now() < entry.expires_at {
                Some(entry.value.clone())
            } else {
                None
            }
        })
    }

    pub fn set_stats(&self, value: String) {
        if let Ok(mut guard) = self.stats.lock() {
            *guard = Some(CacheEntry {
                value,
                expires_at: Instant::now() + Duration::from_secs(30),
            });
        }
    }

    pub fn get_search(&self, query: &str) -> Option<String> {
        let guard = self.search.lock().ok()?;
        guard.get(query).and_then(|entry| {
            if Instant::now() < entry.expires_at {
                Some(entry.value.clone())
            } else {
                None
            }
        })
    }

    pub fn set_search(&self, query: String, value: String) {
        if let Ok(mut guard) = self.search.lock() {
            // Limit cache size
            if guard.len() > 100 {
                guard.clear();
            }
            guard.insert(query, CacheEntry {
                value,
                expires_at: Instant::now() + Duration::from_secs(300),
            });
        }
    }

    pub fn invalidate_all(&self) {
        if let Ok(mut guard) = self.stats.lock() {
            *guard = None;
        }
        if let Ok(mut guard) = self.search.lock() {
            guard.clear();
        }
    }
}
