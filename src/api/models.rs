use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub enum InferenceResult {
    MeanStd(MeanStdResult),
}

impl InferenceResult {
    pub fn new_mean_std(mean: [f64; 3], std: [f64; 3]) -> Self {
        Self::MeanStd(MeanStdResult { mean, std })
    }
}

/// The result of the compute mean and std request
#[derive(Clone, Debug, Serialize)]
pub struct MeanStdResult {
    pub mean: [f64; 3],
    pub std: [f64; 3],
}
