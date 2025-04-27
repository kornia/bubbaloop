use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct PipelineStartRequest {
    // the name of the pipeline to start
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PipelineStopRequest {
    // the name of the pipeline to stop
    pub name: String,
}
