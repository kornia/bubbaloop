use crate::{api::models::streaming::StreamingQuery, pipeline::ServerGlobalState};
use axum::{
    body::Body,
    extract::{ws::WebSocketUpgrade, Path, State},
    http::{header, StatusCode},
    response::IntoResponse,
};

pub async fn get_streaming_image(
    Path(query): Path<StreamingQuery>,
    State(state): State<ServerGlobalState>,
) -> impl IntoResponse {
    log::trace!("Request to get streaming image: {}", query.channel_id);

    // TODO: need to improve later to make sure that the right pipeline is running
    if !state.pipeline_store.is_cameras_pipeline_running()
        && !state.pipeline_store.is_inference_pipeline_running()
    {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            [(header::CONTENT_TYPE, "text/plain")],
            Body::from("Cameras pipeline not started. Please start the cameras pipeline first."),
        );
    }

    match state.result_store.images[query.channel_id as usize]
        .tx
        .subscribe()
        .recv()
        .await
    {
        Ok(result) => {
            // Return the JPEG image data directly with proper headers
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "image/jpeg")],
                Body::from(result.data),
            )
        }
        Err(e) => {
            log::error!("Failed to get streaming image: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                [(header::CONTENT_TYPE, "text/plain")],
                Body::from("Failed to get streaming image: `just start-pipeline streaming`"),
            )
        }
    }
}

/// Handler for WebSocket connections
pub async fn websocket_streaming_image(
    ws: WebSocketUpgrade,
    Path(query): Path<StreamingQuery>,
    State(state): State<ServerGlobalState>,
) -> impl IntoResponse {
    // Accept the WebSocket connection
    ws.on_upgrade(|socket| handle_socket(socket, query, state))
}

async fn handle_socket(
    mut socket: axum::extract::ws::WebSocket,
    query: StreamingQuery,
    state: ServerGlobalState,
) {
    log::info!(
        "WebSocket connection established for channel {}",
        query.channel_id
    );

    if !state.pipeline_store.is_cameras_pipeline_running()
        && !state.pipeline_store.is_inference_pipeline_running()
    {
        log::error!("Cameras pipeline not started");
        // Send an error message and close the connection
        let _ = socket
            .send(axum::extract::ws::Message::Text(
                "Cameras pipeline not started. Please start the cameras pipeline first.".into(),
            ))
            .await;
        return;
    }

    // Subscribe to the broadcast channel for this camera
    let mut rx = state.result_store.images[query.channel_id as usize]
        .tx
        .subscribe();

    // Stream images until the client disconnects
    while let Ok(result) = rx.recv().await {
        if let Err(e) = socket
            .send(axum::extract::ws::Message::Binary(result.data.into()))
            .await
        {
            log::error!("Failed to send WebSocket message: {}", e);
            break;
        }
    }

    log::info!(
        "WebSocket connection closed for channel {}",
        query.channel_id
    );
}
