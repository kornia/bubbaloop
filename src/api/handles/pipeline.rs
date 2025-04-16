use crate::{
    api::models::pipeline::{PipelineStartRequest, PipelineStopRequest},
    cu29,
    pipeline::{self, PipelineHandle, PipelineInfo, PipelineStatus, PipelineStore},
};
use axum::{
    extract::State,
    response::{IntoResponse, Json},
};
use reqwest::StatusCode;
use serde_json::json;
use std::{sync::atomic::AtomicBool, sync::Arc};

/// Start a pipeline given its id
pub async fn start_pipeline(
    State(store): State<PipelineStore>,
    Json(request): Json<PipelineStartRequest>,
) -> impl IntoResponse {
    log::debug!("Request to start pipeline: {}", request.pipeline_id);

    const SUPPORTED_PIPELINES: [&str; 3] = ["bubbaloop", "cameras", "inference"];
    if !SUPPORTED_PIPELINES.contains(&request.pipeline_id.as_str()) {
        log::error!(
            "Pipeline {} not supported. Try 'bubbaloop', 'cameras', 'inference', instead",
            request.pipeline_id
        );
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "Pipeline not supported. Try 'bubbaloop', 'cameras', 'inference' instead",
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
        "bubbaloop" => pipeline::spawn_bubbaloop_thread(stop_signal.clone()),
        "cameras" => cu29::pipelines::spawn_cameras_pipeline(stop_signal.clone()),
        "inference" => cu29::pipelines::spawn_inference_pipeline(stop_signal.clone()),
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

    log::debug!("Pipeline {} started", pipeline_id);

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
        log::error!("Pipeline {} not found", request.pipeline_id);
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "Pipeline not found",
            })),
        );
    }

    log::debug!("Pipeline {} stopped", request.pipeline_id);

    (
        StatusCode::OK,
        Json(json!({ "message": format!("Pipeline {} stopped", request.pipeline_id) })),
    )
}

// List all pipelines and return their status
pub async fn list_pipelines(State(store): State<PipelineStore>) -> impl IntoResponse {
    log::debug!("Request to list pipelines");
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
