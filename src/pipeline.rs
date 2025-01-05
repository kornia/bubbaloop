use ::cu29::read_configuration;
use axum::{
    extract::State,
    response::{IntoResponse, Json},
};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    collections::HashMap,
    sync::atomic::AtomicBool,
    sync::{Arc, Mutex},
};

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
        map.remove(name)
            .map(|pipeline| {
                pipeline
                    .stop_signal
                    .store(true, std::sync::atomic::Ordering::Relaxed);

                pipeline
                    .handle
                    .join()
                    .map_err(|_| log::error!("Failed to join pipeline {}", name))
                    .is_ok()
            })
            .unwrap_or(false)
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
    // the id of the pipeline
    id: String,
    // the status of the pipeline
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
    // TODO: create a pipeline factory so that from the REST API we can register
    //       a new pipeline and start it
    // NOTE: for now we only support one pipeline ["bubbaloop"]
    const SUPPORTED_PIPELINES: [&str; 2] = ["bubbaloop", "recording"];
    if !SUPPORTED_PIPELINES.contains(&request.pipeline_id.as_str()) {
        log::error!(
            "Pipeline {} not supported. Try 'bubbaloop' or 'recording' instead",
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

    // the stop signal to kill the pipeline thread
    let stop_signal = Arc::new(AtomicBool::new(false));

    let handle = match pipeline_id.as_str() {
        "bubbaloop" => dummy_bubbaloop_thread(&pipeline_id, stop_signal.clone()),
        "recording" => cu29::app::spawn_cu29_thread(&pipeline_id, stop_signal.clone()),
        _ => {
            log::error!("Pipeline {} not supported", pipeline_id);
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "Pipeline not supported" })),
            );
        }
    };

    // add the pipeline handle to the store
    pipeline_store.insert(
        pipeline_id.clone(),
        PipelineHandle {
            id: pipeline_id.clone(),
            handle,
            status: PipelineStatus::Running,
            stop_signal,
        },
    );

    (
        StatusCode::OK,
        Json(json!({
            "message": format!("Pipeline {} started", pipeline_id),
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
            status: pipeline.status.clone(),
        })
        .collect::<Vec<_>>();
    Json(pipelines)
}

/// Get the current configuration pipeline
pub async fn get_config(State(_store): State<PipelineStore>) -> impl IntoResponse {
    let copper_config = match read_configuration("bubbaloop.ron") {
        Ok(config) => config,
        Err(e) => {
            log::error!("Failed to read configuration: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "Failed to read configuration" })),
            );
        }
    };

    let all_nodes = copper_config.get_all_nodes();

    (StatusCode::OK, Json(json!(all_nodes)))
}

/// A dummy pipeline that runs indefinitely and prints a message every second
fn dummy_bubbaloop_thread(
    pipeline_id: &str,
    stop_signal: Arc<AtomicBool>,
) -> std::thread::JoinHandle<PipelineResult> {
    let pipeline_id = pipeline_id.to_string();
    let signs = ["|", "/", "-", "\\", "|", "/", "-", "\\"];
    let emojis = ["😊", "🚀", "🦀", "🎉", "✨", "🎸", "🌟", "🍕", "🎮", "🌈"];
    std::thread::spawn(move || {
        let mut counter = 0;
        while !stop_signal.load(std::sync::atomic::Ordering::Relaxed) {
            log::debug!(
                "{} Hello !! This is a Bubbaloop !!! {}",
                signs[counter % signs.len()],
                emojis[counter % emojis.len()]
            );
            std::thread::sleep(std::time::Duration::from_secs(1));
            counter += 1;
        }
        log::debug!(
            "Pipeline {} stopped after {} iterations",
            pipeline_id,
            counter
        );
        Ok(())
    })
}
