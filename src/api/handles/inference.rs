use axum::extract::State;
use axum::response::IntoResponse;
use axum::Json;

use crate::api::models::inference::{InferenceResponse, InferenceResult};
use crate::pipeline::ResultStore;

pub async fn get_inference_result(State(store): State<ResultStore>) -> impl IntoResponse {
    let Ok(result) = store.inference.tx.subscribe().recv().await else {
        return Json(InferenceResponse::Error {
            error: "Failed to get inference result: `just start-pipeline inference`".to_string(),
        });
    };

    Json(InferenceResponse::Success(InferenceResult {
        timestamp_nanos: result.timestamp_nanos,
        detections: result.detections,
    }))
}
