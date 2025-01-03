use axum::extract::State;
use axum::response::{IntoResponse, Json};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use crate::cu29;

/// Global store of all pipelines managed by the server
#[derive(Clone)]
pub struct PipelineStore(pub Arc<Mutex<HashMap<String, PipelineHandle>>>);

type PipelineResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

impl PipelineStore {
    /// Register a pipeline in the store and start it
    pub fn register_pipeline(
        &mut self,
        name: String,
        handle: std::thread::JoinHandle<PipelineResult>,
        stop_signal: Arc<AtomicBool>,
    ) {
        self.0.lock().unwrap().insert(
            name.clone(),
            PipelineHandle {
                id: name.clone(),
                handle,
                status: PipelineStatus::Running,
                stop_signal,
            },
        );
    }

    /// Unregister a pipeline from the store and stop it
    pub fn unregister_pipeline(&self, name: &str) -> bool {
        let mut map = self.0.lock().unwrap();
        let pipeline = map.remove(name);
        if let Some(pipeline) = pipeline {
            pipeline
                .stop_signal
                .store(true, std::sync::atomic::Ordering::Relaxed);
            return true;
        }
        false
    }
}

/// The current status of a pipeline
#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum PipelineStatus {
    /// The pipeline is running in the background
    Running,
    /// The pipeline is stopped
    Stopped,
    /// The pipeline has encountered an error
    Error(String),
}

/// An object managing a pipeline
#[derive(Debug)]
pub struct PipelineHandle {
    // a unique identifier for the pipeline
    // TODO: explore using a UUID
    id: String,
    /// the task that the pipeline is running
    /// TODO: create a custom error type
    handle: std::thread::JoinHandle<PipelineResult>,
    // the status of the pipeline
    status: PipelineStatus,
    // stop signal
    stop_signal: Arc<AtomicBool>,
}

#[derive(Debug, Serialize)]
struct PipelineInfo {
    id: String,
    thread_name: String,
    status: PipelineStatus,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PipelineStartRequest {
    // the id of the pipeline to start
    pub pipeline_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PipelineStopRequest {
    // the id of the pipeline to stop
    pub pipeline_id: String,
}

// initialize the pipeline store
pub fn init_pipeline_store() -> PipelineStore {
    PipelineStore(Arc::new(Mutex::new(HashMap::new())))
}

/// Start a pipeline given its id
pub async fn start_pipeline(
    State(store): State<PipelineStore>,
    Json(request): Json<PipelineStartRequest>,
) -> impl IntoResponse {
    // NOTE: for now we only support one pipeline ["bubbaloop"]
    if request.pipeline_id != "bubbaloop" {
        log::error!(
            "Pipeline {} not supported. Try 'bubbaloop' instead",
            request.pipeline_id
        );
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "Pipeline not supported. Try 'bubbaloop' instead",
            })),
        );
    }

    // check if the pipeline id is already in the store
    let pipeline_id = request.pipeline_id;
    let mut pipeline_store = store.0.lock().expect("Failed to lock pipeline store");

    if pipeline_store.contains_key(&pipeline_id) {
        log::error!("Pipeline {} already exists", pipeline_id);
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "Pipeline already exists",
            })),
        );
    }

    // create the pipeline if it does not exist
    let mut app = match cu29::app::CopperPipeline::new() {
        Ok(app) => app,
        Err(e) => {
            log::error!("Failed to create pipeline: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "Failed to create pipeline" })),
            );
        }
    };

    // the stop signal to kill the pipeline thread
    let stop_signal = Arc::new(AtomicBool::new(false));

    let handle = std::thread::spawn({
        let pipeline_id = pipeline_id.clone();
        let stop_signal = stop_signal.clone();
        move || -> PipelineResult {
            // create the pipeline and start the tasks
            app.start_all_tasks()?;

            while !stop_signal.load(std::sync::atomic::Ordering::Relaxed) {
                // we run the pipeline iteration step by step
                app.run_one_iteration()?;

                // NOTE: is this really needed?
                std::thread::sleep(std::time::Duration::from_millis(30));
            }

            // stop the pipeline and wait for the tasks to finish
            app.stop_all_tasks()?;
            log::debug!("Pipeline {} stopped", pipeline_id);

            Ok(())
        }
    });

    // add the pipeline handle to the store
    pipeline_store.insert(
        pipeline_id.clone(),
        PipelineHandle {
            id: pipeline_id,
            handle,
            status: PipelineStatus::Running,
            stop_signal,
        },
    );

    (
        StatusCode::OK,
        Json(json!({
            "message": "Pipeline started",
        })),
    )
}

// Stop a pipeline given its id
pub async fn stop_pipeline(
    State(store): State<PipelineStore>,
    Json(request): Json<PipelineStopRequest>,
) -> impl IntoResponse {
    log::debug!("Request to stop pipeline: {}", request.pipeline_id);
    if !store.unregister_pipeline(&request.pipeline_id) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "Pipeline not found",
            })),
        );
    }

    (
        StatusCode::OK,
        Json(json!({ "message": format!("Pipeline {} stopped", request.pipeline_id) })),
    )
}

// List all pipelines and return their status
pub async fn list_pipelines(State(store): State<PipelineStore>) -> impl IntoResponse {
    let store = store.0.lock().expect("Failed to lock pipeline store");
    let pipelines = store
        .values()
        .map(|pipeline| PipelineInfo {
            id: pipeline.id.clone(),
            thread_name: pipeline.handle.thread().name().unwrap_or("").to_string(),
            status: pipeline.status.clone(),
        })
        .collect::<Vec<_>>();
    Json(pipelines)
}
