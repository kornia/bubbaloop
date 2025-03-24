use axum::extract::State;
use axum::response::{IntoResponse, Json};

use crate::api::models::inference::InferenceResponse;
use crate::pipeline::ResultStore;

pub async fn get_inference_result(State(store): State<ResultStore>) -> impl IntoResponse {
    let guard = store.0.lock().expect("Failed to lock result store");
    let result = match guard.get("inference") {
        Some(result) => result.clone(),
        None => {
            return Json(InferenceResponse::Error {
                error: "Failed to get mean std result: `just start-pipeline inference`".to_string(),
            });
        }
    };
    Json(InferenceResponse::Success(result))
}
