use kornia_yolo::BoundingBox;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Clone)]
pub struct InferenceResult {
    pub timestamp_nanos: u64,
    pub detections: Vec<BoundingBox>,
}

/// The query for the inference request
#[derive(Debug, Deserialize, Serialize)]
pub struct InferenceQuery;

/// The response of the inference request
#[derive(Debug, Serialize)]
pub enum InferenceResponse {
    Success(InferenceResult),
    Error { error: String },
}
