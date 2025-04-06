use crate::api::models::inference::{InferenceResponse, InferenceSettingsQuery};
use crate::pipeline::ResultStore;
use axum::extract::State;
use axum::response::IntoResponse;
use axum::Json;
use serde_json::json;

pub async fn get_inference_result(State(store): State<ResultStore>) -> impl IntoResponse {
    let Ok(result) = store.inference.tx.subscribe().recv().await else {
        return Json(InferenceResponse::Error {
            error: "Failed to get inference result: `just start-pipeline inference`".to_string(),
        });
    };
    Json(InferenceResponse::Success(result))
}

pub async fn post_inference_settings(
    State(store): State<ResultStore>,
    Json(query): Json<InferenceSettingsQuery>,
) -> impl IntoResponse {
    let Ok(_) = store.inference_settings.tx.send(query.prompt) else {
        return Json(json!({
            "error": "Failed to send inference settings"
        }));
    };

    Json(json!({
        "success": true
    }))
}
