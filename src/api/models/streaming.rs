use crate::cu29::msgs::EncodedImage;
use serde::{Deserialize, Serialize};

/// The query for the inference request
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct StreamingQuery {
    pub channel_id: u8,
}

/// The response of the inference request
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum StreamingResponse {
    Success(EncodedImage),
    Error { error: String },
}
