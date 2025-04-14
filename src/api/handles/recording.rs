use crate::{api::models::recording::RecordingQuery, pipeline::ResultStore};
use axum::{extract::State, response::IntoResponse, Json};
use serde_json::json;

pub async fn post_recording_command(
    State(store): State<ResultStore>,
    Json(query): Json<RecordingQuery>,
) -> impl IntoResponse {
    log::debug!("Request to post recording command: {:?}", query.command);
    let Ok(_) = store.recording.tx.send(query.command) else {
        return Json(json!({
            "error": "Failed to send recording"
        }));
    };

    Json(json!({
        "success": true
    }))
}
