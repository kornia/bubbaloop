mod inference;
mod recording;
mod streaming;

pub use inference::spawn_inference_pipeline;
pub use recording::spawn_recording_pipeline;
pub use streaming::spawn_streaming_pipeline;
