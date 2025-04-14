use serde::{Deserialize, Serialize};

/// The command for the recording request
#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum RecordingCommand {
    Start,
    Stop,
}

/// The query for the recording request
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RecordingQuery {
    pub command: RecordingCommand,
}
