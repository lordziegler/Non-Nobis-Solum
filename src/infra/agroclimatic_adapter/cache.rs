//! In-memory memoization for any `AgroclimaticRepository`.
//!
//! A 30-year climatology does not change while the process runs, so this
//! needs no TTL and no invalidation — unlike the sibling `vigil`
//! project's `TtlCache`, which caches *current* weather and therefore has
//! to expire. Nothing is written to disk.
//!
//! A poisoned mutex is treated as a cache miss rather than a panic:
//! caching is an optimization and must never be able to fail a plan.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::core::domain::{AnnualClimatology, DomainError};
use crate::core::ports::AgroclimaticRepository;

/// Coordinates rounded to 2 decimal places (~1.1 km) as an integer key,
/// because f64 is not `Eq + Hash`. Well inside POWER's ~0.5° grid, so two
/// lots that round together would have resolved to the same cell anyway.
type CacheKey = (i64, i64);

fn cache_key(latitude: f64, longitude: f64) -> CacheKey {
    ((latitude * 100.0).round() as i64, (longitude * 100.0).round() as i64)
}

/// Wraps any `AgroclimaticRepository` and serves repeat lookups of the
/// same coordinates from memory. Useful for a long-lived front-end
/// planning several crops on one lot; a single CLI run never hits it
/// twice.
///
/// `Send + Sync` on the inner repository is what lets the same cache be
/// filled by a background thread and read by the render loop — see
/// [`PrewarmedAgroclimaticRepo`].
pub struct CachedAgroclimaticRepo {
    inner: Box<dyn AgroclimaticRepository + Send + Sync>,
    entries: Mutex<HashMap<CacheKey, AnnualClimatology>>,
}

impl CachedAgroclimaticRepo {
    pub fn new(inner: Box<dyn AgroclimaticRepository + Send + Sync>) -> Self {
        Self { inner, entries: Mutex::new(HashMap::new()) }
    }

    /// What is already in memory, without ever reaching the network.
    pub fn cached(&self, latitude: f64, longitude: f64) -> Option<AnnualClimatology> {
        self.entries.lock().ok()?.get(&cache_key(latitude, longitude)).cloned()
    }
}

/// A non-blocking view over a shared [`CachedAgroclimaticRepo`]: a hit is
/// returned, a miss is an `ExternalServiceUnavailable` error.
///
/// This exists for the TUI, whose render loop is single-threaded and can
/// afford exactly zero seconds of a 10 s HTTP timeout. Something else
/// (the front-end's prefetch thread) fills the cache; a plan asked for
/// before that lands degrades to baseline constants, which is the same
/// path an outage already takes.
pub struct PrewarmedAgroclimaticRepo {
    cache: Arc<CachedAgroclimaticRepo>,
}

impl PrewarmedAgroclimaticRepo {
    pub fn new(cache: Arc<CachedAgroclimaticRepo>) -> Self {
        Self { cache }
    }
}

impl AgroclimaticRepository for PrewarmedAgroclimaticRepo {
    fn fetch_climatology(&self, latitude: f64, longitude: f64) -> Result<AnnualClimatology, DomainError> {
        self.cache.cached(latitude, longitude).ok_or_else(|| {
            DomainError::ExternalServiceUnavailable(format!("climatology for {latitude},{longitude} not fetched yet"))
        })
    }
}

impl AgroclimaticRepository for CachedAgroclimaticRepo {
    fn fetch_climatology(&self, latitude: f64, longitude: f64) -> Result<AnnualClimatology, DomainError> {
        let key = cache_key(latitude, longitude);

        if let Ok(entries) = self.entries.lock() {
            if let Some(hit) = entries.get(&key) {
                return Ok(hit.clone());
            }
        }

        // Only successes are cached. A failed fetch is usually transient
        // (timeout, flaky link) and re-trying it next time is cheap
        // compared to pinning a lot to "no climate" for the whole session.
        let climate = self.inner.fetch_climatology(latitude, longitude)?;

        if let Ok(mut entries) = self.entries.lock() {
            entries.insert(key, climate.clone());
        }

        Ok(climate)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    /// Counts calls so a cache hit is observable, and can be told to fail.
    struct CountingRepo {
        calls: AtomicUsize,
        fails: bool,
    }

    impl CountingRepo {
        fn new(fails: bool) -> Self {
            Self { calls: AtomicUsize::new(0), fails }
        }
    }

    impl AgroclimaticRepository for CountingRepo {
        fn fetch_climatology(&self, latitude: f64, _longitude: f64) -> Result<AnnualClimatology, DomainError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.fails {
                return Err(DomainError::ExternalServiceUnavailable("down".into()));
            }
            // Echo the latitude back so different keys are distinguishable.
            Ok(AnnualClimatology { mean_temp_c: Some(latitude), ..Default::default() })
        }
    }

    /// Shares the counter with the repo the cache takes ownership of.
    struct SharedRepo(std::sync::Arc<CountingRepo>);

    impl AgroclimaticRepository for SharedRepo {
        fn fetch_climatology(&self, latitude: f64, longitude: f64) -> Result<AnnualClimatology, DomainError> {
            self.0.fetch_climatology(latitude, longitude)
        }
    }

    fn cached(inner: &std::sync::Arc<CountingRepo>) -> CachedAgroclimaticRepo {
        CachedAgroclimaticRepo::new(Box::new(SharedRepo(inner.clone())))
    }

    #[test]
    fn a_repeated_lookup_hits_the_cache_once() {
        let counter = std::sync::Arc::new(CountingRepo::new(false));
        let repo = cached(&counter);

        let first = repo.fetch_climatology(1.2136, -77.2811).expect("first fetch");
        let second = repo.fetch_climatology(1.2136, -77.2811).expect("second fetch");

        assert_eq!(first, second);
        assert_eq!(counter.calls.load(Ordering::SeqCst), 1, "second lookup served from memory");
    }

    #[test]
    fn coordinates_are_keyed_at_two_decimals() {
        let counter = std::sync::Arc::new(CountingRepo::new(false));
        let repo = cached(&counter);

        // 1.2136 and 1.2149 both round to 1.21 -> same grid cell, one fetch.
        repo.fetch_climatology(1.2136, -77.2811).expect("first fetch");
        repo.fetch_climatology(1.2149, -77.2794).expect("same cell");
        assert_eq!(counter.calls.load(Ordering::SeqCst), 1);

        // A genuinely different lot must not be served the first one's data.
        let other = repo.fetch_climatology(4.71, -74.07).expect("different cell");
        assert_eq!(counter.calls.load(Ordering::SeqCst), 2);
        assert_eq!(other.mean_temp_c, Some(4.71));
    }

    #[test]
    fn the_prewarmed_view_never_fetches_and_sees_what_the_cache_holds() {
        let counter = std::sync::Arc::new(CountingRepo::new(false));
        let cache = std::sync::Arc::new(cached(&counter));
        let prewarmed = PrewarmedAgroclimaticRepo::new(cache.clone());

        // Nothing fetched yet: a miss, and still no call to the provider.
        assert!(prewarmed.fetch_climatology(1.2136, -77.2811).is_err());
        assert_eq!(counter.calls.load(Ordering::SeqCst), 0, "the prewarmed view must never fetch");

        // Whoever is allowed to block fills the cache; the view then sees it.
        cache.fetch_climatology(1.2136, -77.2811).expect("background fetch");
        assert_eq!(
            prewarmed.fetch_climatology(1.2136, -77.2811).expect("hit").mean_temp_c,
            Some(1.2136)
        );
    }

    #[test]
    fn failures_are_not_cached() {
        let counter = std::sync::Arc::new(CountingRepo::new(true));
        let repo = cached(&counter);

        assert!(repo.fetch_climatology(1.2136, -77.2811).is_err());
        assert!(repo.fetch_climatology(1.2136, -77.2811).is_err());
        // A transient outage must not pin this lot to "no climate".
        assert_eq!(counter.calls.load(Ordering::SeqCst), 2, "retried rather than remembering the failure");
    }
}
