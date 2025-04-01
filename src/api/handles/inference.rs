use crate::api::models::inference::InferenceResponse;
use crate::pipeline::ResultStore;
use axum::extract::State;
use axum::response::IntoResponse;
use axum::Json;

pub async fn get_inference_result(State(store): State<ResultStore>) -> impl IntoResponse {
    let Ok(result) = store.inference.tx.subscribe().recv().await else {
        return Json(InferenceResponse::Error {
            error: "Failed to get inference result: `just start-pipeline inference`".to_string(),
        });
    };

    Json(InferenceResponse::Success(result.clone()))
}
