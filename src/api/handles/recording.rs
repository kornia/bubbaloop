use crate::{
    api::models::recording::RecordingQuery,
    pipeline::{PipelineStatus, ServerGlobalState},
};
use axum::{
    extract::State,
    response::{IntoResponse, Json},
};
use serde_json::json;

pub async fn post_recording_command(
    State(state): State<ServerGlobalState>,
    Json(query): Json<RecordingQuery>,
) -> impl IntoResponse {
    log::debug!("Request to post recording command: {:?}", query.command);

    let pipelines = state.pipeline_store.list_pipelines();
    let recording_pipeline = pipelines
        .iter()
        .filter(|p| p.id == "cameras" && p.status == PipelineStatus::Running)
        .collect::<Vec<_>>();

    if recording_pipeline.is_empty() {
        return Json(json!({
            "success": false,
            "error": "Cameras pipeline not started. Please start the cameras pipeline first."
        }));
    }

    log::debug!("Request to post recording command: {:?}", query.command);

    if let Err(e) = state.result_store.recording.request.tx.send(query.command) {
        return Json(json!({
            "success": false,
            "error": format!("Failed to send command to recording: {}", e)
        }));
    }

    match state.result_store.recording.reply.rx.lock().unwrap().recv() {
        Ok(reply) => Json(json!({
            "success": reply.success,
            "error": reply.error
        })),
        Err(e) => Json(json!({
            "success": false,
            "error": format!("Failed to receive reply from recording: {}", e)
        })),
    }
}
