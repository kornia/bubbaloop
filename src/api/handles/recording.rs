use crate::{
    api::models::recording::{RecordingQuery, RecordingResponse},
    pipeline::ServerGlobalState,
};
use axum::{
    extract::State,
    response::{IntoResponse, Json},
};

pub async fn post_recording_command(
    State(state): State<ServerGlobalState>,
    Json(query): Json<RecordingQuery>,
) -> impl IntoResponse {
    log::debug!("Request to post recording command: {:?}", query.command);

    if !state.pipeline_store.is_cameras_pipeline_running() {
        return Json(RecordingResponse::Error {
            error: "Cameras pipeline not started. Please start the cameras pipeline first."
                .to_string(),
        });
    }

    log::debug!("Request to post recording command: {:?}", query.command);

    if let Err(e) = state.result_store.recording.request.tx.send(query.command) {
        return Json(RecordingResponse::Error {
            error: format!("Failed to send command to recording: {}", e),
        });
    }

    match state.result_store.recording.reply.rx.lock().unwrap().recv() {
        Ok(reply) => Json(reply),
        Err(e) => Json(RecordingResponse::Error {
            error: format!("Failed to receive reply from recording: {}", e),
        }),
    }
}
