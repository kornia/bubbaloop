use axum::extract::State;
use axum::response::IntoResponse;
use axum::Json;

use crate::api::models::inference::InferenceResponse;
use crate::pipeline::ResultStore;

pub async fn get_inference_result(State(store): State<ResultStore>) -> impl IntoResponse {
    let Ok(guard) = store.inference.read() else {
        return Json(InferenceResponse::Error {
            error: "Failed to get inference result: `just start-pipeline inference`".to_string(),
        });
    };

    let Some(result) = guard.back() else {
        return Json(InferenceResponse::Error {
            error: "No inference result available".to_string(),
        });
    };

    Json(InferenceResponse::Success(result.clone()))
}
