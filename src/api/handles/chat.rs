use crate::api::models::chat::{ChatQuery, ChatResponse};
use crate::pipeline::ResultStore;
use axum::{
    extract::State,
    response::{IntoResponse, Json},
};

pub async fn post_chat_request(
    State(result_store): State<ResultStore>,
    Json(query): Json<ChatQuery>,
) -> impl IntoResponse {
    if let Err(e) = result_store.inference.query.tx.send(query.message) {
        log::error!("Failed to send message to inference channel: {}", e);
    }

    let Ok(response) = result_store.inference.result.rx.lock().unwrap().recv() else {
        return Json(ChatResponse::Error {
            error: "Failed to receive message from inference channel".to_string(),
        });
    };

    let response = ChatResponse::Success(response);

    Json(response)
}
