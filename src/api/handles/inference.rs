use crate::api::models::inference::InferenceResponse;
use crate::cu29::msgs::InferenceResultMsg;
use crate::pipeline::ResultStore;
use axum::extract::State;
use axum::response::IntoResponse;
use axum::Json;

pub async fn get_inference_result(State(_store): State<ResultStore>) -> impl IntoResponse {
    Json(InferenceResponse::Success(InferenceResultMsg {
        timestamp_nanos: 0,
        detections: vec![],
    }))
}
