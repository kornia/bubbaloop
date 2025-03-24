use axum::extract::State;
use axum::response::{IntoResponse, Json};
use serde::{Deserialize, Serialize};

use crate::api::models::InferenceResult;
use crate::pipeline::ResultStore;

/// The query for the inference request
#[derive(Debug, Deserialize, Serialize)]
pub struct InferenceQuery;

/// The response of the inference request
#[derive(Debug, Serialize)]
pub enum InferenceResponse {
    Success(InferenceResult),
    Error { error: String },
}

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
