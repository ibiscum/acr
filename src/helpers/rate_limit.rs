use std::collections::HashMap;
use parking_lot::Mutex;
use std::time::{Duration, Instant};
use once_cell::sync::Lazy;
use log::debug;

const DEFAULT_RATE_LIMIT_MS: u64 = 500; // Default to 500ms (2 requests per second)

/// Stores the last access time for a specific service
struct ServiceLimit {
    /// Last time this service was accessed
    last_access: Instant,
    /// Minimum delay between requests in milliseconds
    minimum_delay_ms: u64,
}

/// RateLimiter ensures that API calls to external services respect rate limits
pub struct RateLimiter {
    /// Maps service names to their last access time and rate limit
    services: HashMap<String, ServiceLimit>,
}

// Global singleton for the rate limiter
static RATE_LIMITER: Lazy<Mutex<RateLimiter>> = Lazy::new(|| Mutex::new(RateLimiter::new()));

impl RateLimiter {
    /// Create a new rate limiter
    fn new() -> Self {
        RateLimiter {
            services: HashMap::new(),
        }
    }

    /// Register a rate limit for a specific service
    ///
    /// # Arguments
    /// * `service_name` - Name of the service to register
    /// * `minimum_delay_ms` - Minimum delay between requests in milliseconds
    fn register_service(&mut self, service_name: &str, minimum_delay_ms: u64) {
        if let Some(service_limit) = self.services.get_mut(service_name) {
            // Preserve last access to avoid creating an artificial cooldown bypass
            // when updating rate limit configuration.
            service_limit.minimum_delay_ms = minimum_delay_ms;
        } else {
            let service_limit = ServiceLimit {
                last_access: Instant::now() - Duration::from_millis(minimum_delay_ms),
                minimum_delay_ms,
            };

            self.services.insert(service_name.to_string(), service_limit);
        }
        debug!("Registered rate limit for service '{}': {} ms", service_name, minimum_delay_ms);
    }

    /// Apply rate limiting to a service
    ///
    /// This method will block the current thread if necessary to respect the
    /// configured rate limit for the specified service.
    ///
    /// # Arguments
    /// * `service_name` - Name of the service to rate limit
    fn rate_limit(&mut self, service_name: &str) {
        let now = Instant::now();

        // Get or create the service limit
        let service_limit = self.services
            .entry(service_name.to_string())
            .or_insert_with(|| {
                debug!("Using default rate limit for unregistered service '{}': {} ms",
                       service_name, DEFAULT_RATE_LIMIT_MS);

                ServiceLimit {
                    last_access: now - Duration::from_millis(DEFAULT_RATE_LIMIT_MS),
                    minimum_delay_ms: DEFAULT_RATE_LIMIT_MS,
                }
            });

        // Calculate elapsed time since last access
        let elapsed_ms = now.duration_since(service_limit.last_access).as_millis() as u64;

        // If not enough time has passed, sleep for the remaining time
        if elapsed_ms < service_limit.minimum_delay_ms {
            let sleep_time = service_limit.minimum_delay_ms - elapsed_ms;
            debug!("Rate limiting service '{}': sleeping for {} ms", service_name, sleep_time);
            std::thread::sleep(Duration::from_millis(sleep_time));
        }

        // Update the last access time
        service_limit.last_access = Instant::now();
    }
}

/// Get access to the global rate limiter instance
fn get_rate_limiter() -> parking_lot::MutexGuard<'static, RateLimiter> {
    RATE_LIMITER.lock()
}

/// Register a rate limit for a specific service
///
/// # Arguments
/// * `service_name` - Name of the service to register
/// * `minimum_delay_ms` - Minimum delay between requests in milliseconds
pub fn register_service(service_name: &str, minimum_delay_ms: u64) {
    get_rate_limiter().register_service(service_name, minimum_delay_ms);
}

/// Apply rate limiting to a service
///
/// This function will block the current thread if necessary to respect the
/// configured rate limit for the specified service. If the service has not been
/// registered, a default limit of 500ms (2 requests per second) will be applied.
///
/// # Arguments
/// * `service_name` - Name of the service to rate limit
pub fn rate_limit(service_name: &str) {
    get_rate_limiter().rate_limit(service_name);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_service_inserts_with_expected_delay() {
        let mut limiter = RateLimiter::new();
        limiter.register_service("svc", 250);

        let service = limiter.services.get("svc").expect("service should exist");
        assert_eq!(service.minimum_delay_ms, 250);
    }

    #[test]
    fn regression_reregister_service_preserves_last_access() {
        let mut limiter = RateLimiter::new();
        limiter.register_service("svc", 1000);

        {
            let service = limiter.services.get_mut("svc").expect("service should exist");
            service.last_access = Instant::now();
        }

        let previous_last_access = limiter.services.get("svc").expect("service should exist").last_access;

        // Re-registering should update delay configuration but not reset last access.
        limiter.register_service("svc", 1500);

        let updated = limiter.services.get("svc").expect("service should exist");
        assert_eq!(updated.minimum_delay_ms, 1500);
        assert_eq!(updated.last_access, previous_last_access);
    }
}
