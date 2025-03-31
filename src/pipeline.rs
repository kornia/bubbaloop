use crate::api::models::{camera::CameraResult, inference::InferenceResult};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::atomic::AtomicBool,
    sync::{Arc, Mutex},
};

pub static SERVER_GLOBAL_STATE: Lazy<ServerGlobalState> = Lazy::new(ServerGlobalState::default);

pub type PipelineResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

/// Global store of all pipelines managed by the server
#[derive(Clone, Default)]
pub struct PipelineStore(pub Arc<Mutex<HashMap<String, PipelineHandle>>>);

#[derive(Clone)]
pub struct SenderReceiver<T> {
    pub tx: Arc<tokio::sync::broadcast::Sender<T>>,
}

impl<T: Clone> SenderReceiver<T> {
    pub fn new() -> Self {
        let (tx, _) = tokio::sync::broadcast::channel(5);
        Self { tx: Arc::new(tx) }
    }
}

impl<T: Clone> Default for SenderReceiver<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Global store of all results managed by the server
#[derive(Clone)]
pub struct ResultStore {
    pub inference: SenderReceiver<InferenceResult>,
    pub image: SenderReceiver<CameraResult>,
}

impl Default for ResultStore {
    fn default() -> Self {
        Self {
            inference: SenderReceiver::new(),
            image: SenderReceiver::new(),
        }
    }
}

/// Global state of the server
#[derive(Clone, Default)]
pub struct ServerGlobalState {
    pub pipeline_store: PipelineStore,
    pub result_store: ResultStore,
}

impl PipelineStore {
    /// Register a pipeline in the store and start it
    pub fn register_pipeline(
        &mut self,
        name: &str,
        handle: std::thread::JoinHandle<PipelineResult>,
        stop_signal: Arc<AtomicBool>,
    ) {
        self.0.lock().unwrap().insert(
            name.into(),
            PipelineHandle {
                id: name.into(),
                handle,
                status: PipelineStatus::Running,
                stop_signal,
            },
        );
    }

    /// Unregister a pipeline from the store and stop it
    pub fn unregister_pipeline(&self, name: &str) -> bool {
        let mut map = self.0.lock().unwrap();
        map.remove(name)
            .map(|pipeline| {
                pipeline
                    .stop_signal
                    .store(true, std::sync::atomic::Ordering::Relaxed);

                pipeline
                    .handle
                    .join()
                    .map_err(|_| log::error!("Failed to join pipeline {}", name))
                    .is_ok()
            })
            .unwrap_or(false)
    }
}

/// The current status of a pipeline
#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum PipelineStatus {
    /// The pipeline is running in the background
    Running,
    /// The pipeline is stopped
    Stopped,
    /// The pipeline has encountered an error
    Error(String),
}

/// An object managing a pipeline
#[derive(Debug)]
pub struct PipelineHandle {
    // a unique identifier for the pipeline
    // TODO: explore using a UUID
    pub id: String,
    /// the task that the pipeline is running
    /// TODO: create a custom error type
    pub handle: std::thread::JoinHandle<PipelineResult>,
    // the status of the pipeline
    pub status: PipelineStatus,
    // stop signal
    pub stop_signal: Arc<AtomicBool>,
}

#[derive(Debug, Serialize)]
pub struct PipelineInfo {
    // the id of the pipeline
    pub id: String,
    // the status of the pipeline
    pub status: PipelineStatus,
}

/// A dummy pipeline that runs indefinitely and prints a message every second
pub fn spawn_bubbaloop_thread(
    stop_signal: Arc<AtomicBool>,
) -> std::thread::JoinHandle<PipelineResult> {
    let signs = ["|", "/", "-", "\\", "|", "/", "-", "\\"];
    let emojis = ["😊", "🚀", "🦀", "🎉", "✨", "🎸", "🌟", "🍕", "🎮", "🌈"];
    std::thread::spawn(move || {
        let mut counter = 0;
        while !stop_signal.load(std::sync::atomic::Ordering::Relaxed) {
            log::debug!(
                "{} Hello !! This is a Bubbaloop !!! {}",
                signs[counter % signs.len()],
                emojis[counter % emojis.len()]
            );
            std::thread::sleep(std::time::Duration::from_secs(1));
            counter += 1;
        }
        log::debug!("Bubbaloop pipeline stopped after {} iterations", counter);
        Ok(())
    })
}
