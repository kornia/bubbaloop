use crate::api::models::inference::{
    InferenceResponse, InferenceResultQuery, InferenceSettingsQuery,
};
use crate::pipeline::ResultStore;
use axum::{
    extract::{Path, State},
    response::IntoResponse,
    Json,
};
use serde_json::json;

pub async fn get_inference_result(
    Path(query): Path<InferenceResultQuery>,
    State(store): State<ResultStore>,
) -> impl IntoResponse {
    let Ok(result) = store.inference[query.channel_id as usize]
        .tx
        .subscribe()
        .recv()
        .await
    else {
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
