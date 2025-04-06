use serde::{Deserialize, Serialize};

/// The query for the inference request
#[derive(Debug, Deserialize, Serialize)]
pub struct InferenceQuery {
    pub prompt: String,
}

/// The result of the inference request
#[derive(Clone, Debug, Serialize)]
pub struct InferenceResult {
    pub timestamp_nanos: u64,
    pub prompt: String,
    pub response: String,
}

/// The response of the inference request
#[derive(Debug, Serialize)]
pub enum InferenceResponse {
    Success(InferenceResult),
    Error { error: String },
}
