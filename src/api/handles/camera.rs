use crate::{
    api::models::camera::{CameraQuery, CameraResponse},
    pipeline::ResultStore,
};
use axum::{
    extract::{Path, State},
    response::IntoResponse,
    Json,
};

pub async fn get_camera_image(
    Path(query): Path<CameraQuery>,
    State(store): State<ResultStore>,
) -> impl IntoResponse {
    let Ok(result) = store.images[query.channel_id as usize]
        .tx
        .subscribe()
        .recv()
        .await
    else {
        return Json(CameraResponse::Error {
            error: "Failed to get camera image: `just start-pipeline camera`".to_string(),
        });
    };
    Json(CameraResponse::Success(result))
}
