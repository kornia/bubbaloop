use crate::api::models::camera::CameraResponse;
use crate::pipeline::ResultStore;
use axum::extract::State;
use axum::response::{IntoResponse, Json};

pub async fn get_camera_image(State(store): State<ResultStore>) -> impl IntoResponse {
    let Ok(guard) = store.image.read() else {
        return Json(CameraResponse::Error {
            error: "Failed to get camera image: `just start-pipeline camera`".to_string(),
        });
    };

    let Some(result) = guard.back() else {
        return Json(CameraResponse::Error {
            error: "No camera image available".to_string(),
        });
    };

    Json(CameraResponse::Success(result.clone()))
}
