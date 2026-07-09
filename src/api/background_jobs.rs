use rocket::serde::json::Json;
use rocket::get;
use serde::{Deserialize, Serialize};
use log::{debug, error};
use crate::helpers::background_jobs::{get_all_jobs, BackgroundJob};

/// Response structure for background jobs listing
#[derive(Serialize, Deserialize)]
pub struct BackgroundJobsResponse {
    pub success: bool,
    pub jobs: Option<Vec<BackgroundJobInfo>>,
    pub message: Option<String>,
}

/// Enhanced background job information for API response
#[derive(Serialize, Deserialize)]
pub struct BackgroundJobInfo {
    pub id: String,
    pub name: String,
    pub start_time: u64,
    pub last_update: u64,
    pub progress: Option<String>,
    pub total_items: Option<usize>,
    pub completed_items: Option<usize>,
    pub duration_seconds: u64,
    pub time_since_last_update: u64,
    pub completion_percentage: Option<f64>,
    pub finished: bool,
    pub finish_time: Option<u64>,
}

impl From<BackgroundJob> for BackgroundJobInfo {
    fn from(job: BackgroundJob) -> Self {
        let completion_percentage = if let (Some(completed), Some(total)) = (job.completed_items, job.total_items) {
            if total > 0 {
                Some((completed as f64 / total as f64) * 100.0)
            } else {
                Some(100.0)
            }
        } else {
            None
        };

        Self {
            id: job.id.clone(),
            name: job.name.clone(),
            start_time: job.start_time,
            last_update: job.last_update,
            progress: job.progress.clone(),
            total_items: job.total_items,
            completed_items: job.completed_items,
            duration_seconds: job.duration_seconds(),
            time_since_last_update: job.time_since_last_update(),
            completion_percentage,
            finished: job.finished,
            finish_time: job.finish_time,
        }
    }
}

/// Response structure for error operations
#[derive(Serialize, Deserialize)]
pub struct ErrorResponse {
    pub success: bool,
    pub message: String,
}

/// Get all background jobs
///
/// This endpoint retrieves information about all background jobs
/// (including finished jobs), including their progress and timing information.
#[get("/jobs")]
pub fn get_background_jobs() -> Json<BackgroundJobsResponse> {
    debug!("API request: get background jobs");

    match get_all_jobs() {
        Ok(jobs) => {
            debug!("Successfully retrieved {} background jobs", jobs.len());

            let job_infos: Vec<BackgroundJobInfo> = jobs
                .into_iter()
                .map(BackgroundJobInfo::from)
                .collect();

            Json(BackgroundJobsResponse {
                success: true,
                jobs: Some(job_infos),
                message: None,
            })
        }
        Err(e) => {
            error!("Failed to retrieve background jobs: {}", e);
            Json(BackgroundJobsResponse {
                success: false,
                jobs: None,
                message: Some(format!("Failed to retrieve background jobs: {}", e)),
            })
        }
    }
}

/// Get information about a specific background job by ID
///
/// This endpoint retrieves detailed information about a specific background job.
#[get("/jobs/<job_id>")]
pub fn get_background_job(job_id: String) -> Json<BackgroundJobsResponse> {
    debug!("API request: get background job with ID: {}", job_id);

    match crate::helpers::background_jobs::get_job(&job_id) {
        Ok(Some(job)) => {
            debug!("Successfully retrieved background job: {}", job_id);

            let job_info = BackgroundJobInfo::from(job);

            Json(BackgroundJobsResponse {
                success: true,
                jobs: Some(vec![job_info]),
                message: None,
            })
        }
        Ok(None) => {
            debug!("Background job not found: {}", job_id);
            Json(BackgroundJobsResponse {
                success: false,
                jobs: None,
                message: Some(format!("Background job '{}' not found", job_id)),
            })
        }
        Err(e) => {
            error!("Failed to retrieve background job '{}': {}", job_id, e);
            Json(BackgroundJobsResponse {
                success: false,
                jobs: None,
                message: Some(format!("Failed to retrieve background job: {}", e)),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::helpers::background_jobs::{complete_job, register_job, update_job};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_job_id(prefix: &str) -> String {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        format!("{}_{}", prefix, nanos)
    }

    #[test]
    fn test_background_job_info_completion_percentage() {
        let mut job = BackgroundJob::new("job_pct".to_string(), "Progress Job".to_string());
        job.completed_items = Some(5);
        job.total_items = Some(10);

        let info = BackgroundJobInfo::from(job);
        assert_eq!(info.completion_percentage, Some(50.0));
    }

    #[test]
    fn test_background_job_info_zero_total_maps_to_100_percent() {
        let mut job = BackgroundJob::new("job_zero".to_string(), "Zero Total".to_string());
        job.completed_items = Some(0);
        job.total_items = Some(0);

        let info = BackgroundJobInfo::from(job);
        assert_eq!(info.completion_percentage, Some(100.0));
    }

    #[test]
    fn test_get_background_job_not_found() {
        let response = get_background_job(unique_job_id("missing")).into_inner();

        assert!(!response.success);
        assert!(response.jobs.is_none());
        assert!(response
            .message
            .as_deref()
            .unwrap_or_default()
            .contains("not found"));
    }

    #[test]
    fn test_get_background_job_found() {
        let job_id = unique_job_id("found");
        register_job(job_id.clone(), "Found Job".to_string()).unwrap();

        let response = get_background_job(job_id.clone()).into_inner();

        assert!(response.success);
        let jobs = response.jobs.expect("expected jobs payload");
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].id, job_id);
        assert_eq!(jobs[0].name, "Found Job");
    }

    #[test]
    fn test_get_background_jobs_excludes_finished_jobs_current_behavior() {
        let job_id = unique_job_id("finished");
        register_job(job_id.clone(), "Finished Job".to_string()).unwrap();
        update_job(&job_id, Some("done".to_string()), Some(1), Some(1)).unwrap();
        complete_job(&job_id).unwrap();

        let response = get_background_jobs().into_inner();

        assert!(response.success);
        let jobs = response.jobs.expect("expected jobs payload");
        assert!(jobs.into_iter().all(|job| job.id != job_id));
    }
}
