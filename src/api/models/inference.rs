use super::compute::MeanStdResult;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize)]
pub enum InferenceResult {
    MeanStd(MeanStdResult),
}

impl InferenceResult {
    pub fn new_mean_std(mean: [f64; 3], std: [f64; 3]) -> Self {
        Self::MeanStd(MeanStdResult { mean, std })
    }
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
