use serde::{Deserialize, Serialize};

/// The query for the inference request
#[derive(Debug, Deserialize, Serialize)]
pub struct InferenceSettingsQuery {
    pub prompt: String,
}

/// The query for the inference request
#[derive(Debug, Deserialize, Serialize)]
pub struct InferenceResultQuery {
    pub channel_id: u8,
}

/// The result of the inference request
#[derive(Clone, Debug, Serialize)]
pub struct InferenceResult {
    pub stamp_ns: u64,
    pub channel_id: u8,
    pub prompt: String,
    pub response: String,
}

/// The response of the inference request
#[derive(Debug, Serialize)]
pub enum InferenceResponse {
    Success(InferenceResult),
    Error { error: String },
}
