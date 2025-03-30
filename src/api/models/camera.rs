use crate::cu29::msgs::ImageRgb8Msg;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize)]
pub struct CameraResult {
    pub timestamp_nanos: u64,
    pub image: ImageRgb8Msg,
}

/// The query for the inference request
#[derive(Debug, Deserialize, Serialize)]
pub struct CameraQuery;

/// The response of the inference request
#[derive(Debug, Serialize)]
pub enum CameraResponse {
    Success(CameraResult),
    Error { error: String },
}
