//! In-memory fixed-window rate limiter for authentication endpoints.
//!
//! Counters live in the API process. With several API replicas, move them to
//! Redis (same interface) so that limits are shared.

use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

use axum::http::HeaderMap;

use crate::error::ApiError;

struct Bucket {
    count: u32,
    started: Instant,
}

#[derive(Default)]
pub struct RateLimiter {
    buckets: Mutex<HashMap<String, Bucket>>,
}

impl RateLimiter {
    /// Fail with 429 if `key` already reached `limit` events in the current window.
    pub fn check(&self, key: &str, limit: u32, window: Duration) -> Result<(), ApiError> {
        let map = self.buckets.lock().expect("rate limiter poisoned");
        if let Some(b) = map.get(key) {
            let elapsed = b.started.elapsed();
            if elapsed < window && b.count >= limit {
                return Err(ApiError::TooManyRequests((window - elapsed).as_secs().max(1)));
            }
        }
        Ok(())
    }

    /// Record one event for `key`.
    pub fn hit(&self, key: &str, window: Duration) {
        let mut map = self.buckets.lock().expect("rate limiter poisoned");
        if map.len() > 50_000 {
            map.retain(|_, b| b.started.elapsed() < window);
        }
        let b = map.entry(key.to_string()).or_insert(Bucket { count: 0, started: Instant::now() });
        if b.started.elapsed() >= window {
            b.count = 0;
            b.started = Instant::now();
        }
        b.count += 1;
    }

    pub fn reset(&self, key: &str) {
        self.buckets.lock().expect("rate limiter poisoned").remove(key);
    }
}

/// Client IP as seen by the reverse proxy. Our nginx overwrites `X-Real-IP`
/// with the connecting address, so it cannot be spoofed through the proxy.
pub fn client_ip(headers: &HeaderMap) -> String {
    headers
        .get("x-real-ip")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "direct".to_string())
}

pub const WINDOW_15_MIN: Duration = Duration::from_secs(15 * 60);
pub const WINDOW_1_HOUR: Duration = Duration::from_secs(3600);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_after_limit_and_resets() {
        let rl = RateLimiter::default();
        let w = Duration::from_secs(60);
        for _ in 0..3 {
            assert!(rl.check("k", 3, w).is_ok());
            rl.hit("k", w);
        }
        match rl.check("k", 3, w) {
            Err(ApiError::TooManyRequests(s)) => assert!(s > 0 && s <= 60),
            _ => panic!("expected 429"),
        }
        assert!(rl.check("other", 3, w).is_ok());
        rl.reset("k");
        assert!(rl.check("k", 3, w).is_ok());
    }

    #[test]
    fn window_expires() {
        let rl = RateLimiter::default();
        let w = Duration::from_millis(20);
        rl.hit("k", w);
        assert!(rl.check("k", 1, w).is_err());
        std::thread::sleep(Duration::from_millis(30));
        assert!(rl.check("k", 1, w).is_ok());
    }
}
