use crate::{
    api::models::streaming::{StreamingQuery, StreamingResponse},
    pipeline::ResultStore,
};
use axum::{
    extract::{Path, State},
    response::IntoResponse,
    Json,
};

pub async fn get_streaming_image(
    Path(query): Path<StreamingQuery>,
    State(store): State<ResultStore>,
) -> impl IntoResponse {
    let Ok(result) = store.images[query.channel_id as usize]
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
