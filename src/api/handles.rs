use ::cu29::read_configuration;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use axum::{
    extract::State,
    response::{IntoResponse, Json},
};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    cu29,
    pipeline::{self, PipelineHandle, PipelineInfo, PipelineStatus, PipelineStore},
};

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
        "bubbaloop" => pipeline::dummy_bubbaloop_thread(&pipeline_id, stop_signal.clone()),
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
