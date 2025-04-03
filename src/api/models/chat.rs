use serde::{Deserialize, Serialize};

/// The query for the inference request
#[derive(Debug, Deserialize, Serialize)]
pub struct ChatQuery {
    pub message: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ChatResultMsg {
    pub message: String,
}

/// The response of the inference request
#[derive(Debug, Serialize, Deserialize)]
pub enum ChatResponse {
    Success(String),
    Error { error: String },
}
