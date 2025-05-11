use crate::{
    api::models::streaming::{StreamingQuery, StreamingResponse},
    pipeline::ServerGlobalState,
};
use axum::{
    extract::{Path, State},
    response::{IntoResponse, Json},
};

pub async fn get_streaming_image(
    Path(query): Path<StreamingQuery>,
    State(state): State<ServerGlobalState>,
) -> impl IntoResponse {
    log::debug!("Request to get streaming image: {}", query.channel_id);
    if !state.pipeline_store.is_cameras_pipeline_running() {
        return Json(StreamingResponse::Error {
            error: "Cameras pipeline not started. Please start the cameras pipeline first."
                .to_string(),
        });
    }

    let Ok(result) = state.result_store.images[query.channel_id as usize]
        .tx
        .subscribe()
        .recv()
        .await
    else {
        return Json(StreamingResponse::Error {
            error: "Failed to get streaming image: `just start-pipeline streaming`".to_string(),
        });
    };
    Json(StreamingResponse::Success(result))
}
