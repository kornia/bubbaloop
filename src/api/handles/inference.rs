use crate::{
    api::models::inference::{InferenceResponse, InferenceSettingsQuery},
    pipeline::ServerGlobalState,
};
use axum::{extract::State, response::IntoResponse, Json};
use serde_json::json;

pub async fn get_inference_result(State(state): State<ServerGlobalState>) -> impl IntoResponse {
    log::debug!("Request to get inference result");
    let Ok(result) = state.result_store.inference.tx.subscribe().recv().await else {
        return Json(InferenceResponse::Error {
            error: "Failed to get inference result: `just start-pipeline inference`".to_string(),
        });
    };
    Json(InferenceResponse::Success(result))
}

pub async fn post_inference_settings(
    State(state): State<ServerGlobalState>,
    Json(query): Json<InferenceSettingsQuery>,
) -> impl IntoResponse {
    log::debug!("Request to post inference settings: {}", query.prompt);
    let Ok(_) = state.result_store.inference_settings.tx.send(query.prompt) else {
        return Json(json!({
            "error": "Failed to send inference settings"
        }));
    };

    Json(json!({
        "success": true
    }))
}
