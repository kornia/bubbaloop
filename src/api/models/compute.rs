use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// The result of the compute mean and std request
#[derive(Clone, Debug, Serialize)]
pub struct MeanStdResult {
    pub mean: [f64; 3],
    pub std: [f64; 3],
}

// Query parameters for the mean and std computation
#[derive(Debug, Deserialize)]
pub struct MeanStdQuery {
    // The directory containing the images
    pub images_dir: PathBuf,
    // The number of threads to use
    pub num_threads: Option<usize>,
}

/// The response of the compute mean and std request
#[derive(Debug, Serialize)]
pub enum ComputeResponse {
    Success(MeanStdResult),
    Error { error: String },
}
