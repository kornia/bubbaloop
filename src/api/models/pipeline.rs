use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct PipelineStartRequest {
    // the id of the pipeline to start
    pub pipeline_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PipelineStopRequest {
    // the id of the pipeline to stop
    pub pipeline_id: String,
}
