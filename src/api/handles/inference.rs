use crate::{
    api::models::inference::{
        InferenceResponse, InferenceSettingsQuery, InferenceSettingsResponse,
    },
    pipeline::ServerGlobalState,
};
use axum::{extract::State, response::IntoResponse, Json};

pub async fn get_inference_result(State(state): State<ServerGlobalState>) -> impl IntoResponse {
    log::debug!("Request to get inference result");
    if !state.pipeline_store.is_inference_pipeline_running() {
        return Json(InferenceResponse::Error {
            error: "Inference pipeline not running. Please start the inference pipeline first."
                .to_string(),
        });
    }

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
    if !state.pipeline_store.is_inference_pipeline_running() {
        return Json(InferenceSettingsResponse::Error {
            error: "Inference pipeline not running. Please start the inference pipeline first."
                .to_string(),
        });
    }

    let Ok(_) = state.result_store.inference_settings.tx.send(query.prompt) else {
        return Json(InferenceSettingsResponse::Error {
            error: "Failed to send inference settings".to_string(),
        });
    };

    Json(InferenceSettingsResponse::Success)
}
