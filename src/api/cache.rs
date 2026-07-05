use rocket::serde::json::Json;
use rocket::get;
use serde::{Deserialize, Serialize};
use log::{debug, error};
use crate::helpers::attribute_cache::{get_cache_stats, CacheStats};
use crate::helpers::image_cache;

/// Response structure for cache statistics
#[derive(Serialize, Deserialize)]
pub struct CacheStatsResponse {
    pub success: bool,
    pub stats: Option<CacheStats>,
    pub image_cache_stats: Option<ImageCacheStats>,
    pub message: Option<String>,
}

/// Image cache statistics for API response
#[derive(Serialize, Deserialize)]
pub struct ImageCacheStats {
    pub total_images: usize,
    pub total_size: u64,
    pub last_updated: u64,
}

/// Response structure for error operations
#[derive(Serialize, Deserialize)]
pub struct ErrorResponse {
    pub success: bool,
    pub message: String,
}

fn resolve_cache_stats_status(
    attribute_stats_present: bool,
    image_stats_present: bool,
) -> (bool, Option<String>) {
    let success = attribute_stats_present || image_stats_present;
    let message = if !success {
        Some("Failed to retrieve any cache statistics".to_string())
    } else if !attribute_stats_present {
        Some("Failed to retrieve attribute cache statistics".to_string())
    } else if !image_stats_present {
        Some("Failed to retrieve image cache statistics".to_string())
    } else {
        None
    };

    (success, message)
}

fn build_cache_stats_response(
    attribute_result: Result<CacheStats, String>,
    image_result: Result<image_cache::ImageCacheStats, String>,
) -> CacheStatsResponse {
    let attribute_stats = attribute_result.ok();
    let image_cache_stats = image_result.ok().map(|stats| ImageCacheStats {
        total_images: stats.total_images,
        total_size: stats.total_size,
        last_updated: stats.last_updated,
    });

    let (success, message) =
        resolve_cache_stats_status(attribute_stats.is_some(), image_cache_stats.is_some());

    CacheStatsResponse {
        success,
        stats: attribute_stats,
        image_cache_stats,
        message,
    }
}

fn is_test_failure_injection_enabled(env_var: &str) -> bool {
    match std::env::var(env_var) {
        Ok(value) => {
            let normalized = value.trim().to_ascii_lowercase();
            normalized == "1" || normalized == "true" || normalized == "yes"
        }
        Err(_) => false,
    }
}

/// Get cache statistics
///
/// This endpoint retrieves current cache statistics including disk entries,
/// memory entries, memory usage in bytes, memory limit, and image cache statistics.
#[get("/stats")]
pub fn get_cache_statistics() -> Json<CacheStatsResponse> {
    debug!("API request: get cache statistics");

    // Get attribute cache stats
    let attribute_result = if is_test_failure_injection_enabled("ACR_FORCE_ATTRIBUTE_CACHE_STATS_ERROR") {
        error!("Forced attribute cache stats failure via ACR_FORCE_ATTRIBUTE_CACHE_STATS_ERROR");
        Err("forced attribute cache stats failure".to_string())
    } else {
        match get_cache_stats() {
            Ok(stats) => {
                debug!("Successfully retrieved attribute cache stats: disk_entries={}, memory_entries={}, memory_bytes={}, memory_limit_bytes={}",
                    stats.disk_entries, stats.memory_entries, stats.memory_bytes, stats.memory_limit_bytes);
                Ok(stats)
            }
            Err(e) => {
                error!("Failed to retrieve attribute cache stats: {}", e);
                Err(e.to_string())
            }
        }
    };

    // Get image cache stats
    let image_result = if is_test_failure_injection_enabled("ACR_FORCE_IMAGE_CACHE_STATS_ERROR") {
        error!("Forced image cache stats failure via ACR_FORCE_IMAGE_CACHE_STATS_ERROR");
        Err("forced image cache stats failure".to_string())
    } else {
        match image_cache::get_cache_statistics() {
            Ok(stats) => {
                debug!("Successfully retrieved image cache stats: total_images={}, total_size={}, last_updated={}",
                    stats.total_images, stats.total_size, stats.last_updated);
                Ok(stats)
            }
            Err(e) => {
                error!("Failed to retrieve image cache stats: {}", e);
                Err(e.to_string())
            }
        }
    };

    Json(build_cache_stats_response(attribute_result, image_result))
}

#[cfg(test)]
mod tests {
    use super::{build_cache_stats_response, resolve_cache_stats_status};
    use crate::helpers::attribute_cache::CacheStats;
    use crate::helpers::image_cache::ImageCacheStats as HelperImageCacheStats;

    fn sample_attribute_stats() -> CacheStats {
        CacheStats {
            disk_entries: 2,
            memory_entries: 1,
            memory_bytes: 64,
            memory_limit_bytes: 1024,
        }
    }

    fn sample_image_stats() -> HelperImageCacheStats {
        HelperImageCacheStats {
            total_images: 3,
            total_size: 4096,
            last_updated: 123,
        }
    }

    #[test]
    fn cache_status_when_both_stats_present() {
        let (success, message) = resolve_cache_stats_status(true, true);
        assert!(success);
        assert_eq!(message, None);
    }

    #[test]
    fn cache_status_when_only_attribute_stats_present() {
        let (success, message) = resolve_cache_stats_status(true, false);
        assert!(success);
        assert_eq!(
            message,
            Some("Failed to retrieve image cache statistics".to_string())
        );
    }

    #[test]
    fn cache_status_when_only_image_stats_present() {
        let (success, message) = resolve_cache_stats_status(false, true);
        assert!(success);
        assert_eq!(
            message,
            Some("Failed to retrieve attribute cache statistics".to_string())
        );
    }

    #[test]
    fn cache_status_when_no_stats_present() {
        let (success, message) = resolve_cache_stats_status(false, false);
        assert!(!success);
        assert_eq!(
            message,
            Some("Failed to retrieve any cache statistics".to_string())
        );
    }

    #[test]
    fn cache_response_when_attribute_fails_image_succeeds() {
        let response = build_cache_stats_response(
            Err("attribute failure".to_string()),
            Ok(sample_image_stats()),
        );

        assert!(response.success);
        assert!(response.stats.is_none());
        assert!(response.image_cache_stats.is_some());
        assert_eq!(
            response.message,
            Some("Failed to retrieve attribute cache statistics".to_string())
        );
    }

    #[test]
    fn cache_response_when_image_fails_attribute_succeeds() {
        let response = build_cache_stats_response(
            Ok(sample_attribute_stats()),
            Err("image failure".to_string()),
        );

        assert!(response.success);
        assert!(response.stats.is_some());
        assert!(response.image_cache_stats.is_none());
        assert_eq!(
            response.message,
            Some("Failed to retrieve image cache statistics".to_string())
        );
    }

    #[test]
    fn cache_response_when_both_fail() {
        let response = build_cache_stats_response(
            Err("attribute failure".to_string()),
            Err("image failure".to_string()),
        );

        assert!(!response.success);
        assert!(response.stats.is_none());
        assert!(response.image_cache_stats.is_none());
        assert_eq!(
            response.message,
            Some("Failed to retrieve any cache statistics".to_string())
        );
    }
}
