use crate::cu29::msgs::InferenceResultMsg;
use serde::{Deserialize, Serialize};

/// The query for the inference request
#[derive(Debug, Deserialize, Serialize)]
pub struct InferenceQuery;

/// The response of the inference request
#[derive(Debug, Serialize)]
pub enum InferenceResponse {
    Success(InferenceResultMsg),
    Error { error: String },
}
