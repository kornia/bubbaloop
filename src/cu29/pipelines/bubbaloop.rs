use crate::pipeline::PipelineResult;
use cu29::prelude::*;
use cu29_helpers::basic_copper_setup;
use std::{
    path::PathBuf,
    sync::{atomic::AtomicBool, Arc},
};

const SLAB_SIZE: Option<usize> = Some(150 * 1024 * 1024);

// NOTE: this will use the default config file in the current directory during compilation
// however, it will be overridden by the ron config string when the pipeline is started
#[copper_runtime(config = "src/cu29/pipelines/bubbaloop_2.ron")]
struct BubbaloopApp {}

pub struct BubbaloopPipeline(pub BubbaloopApp);

impl BubbaloopPipeline {
    pub fn new() -> CuResult<Self> {
        let logger_path = PathBuf::from("/tmp/bubbaloop.copper");
        debug!("Logger path: {}", path = &logger_path);

        let copper_ctx = basic_copper_setup(&logger_path, SLAB_SIZE, true, None)?;
        let application = BubbaloopAppBuilder::new()
            .with_context(&copper_ctx)
            .build()?;

        Ok(Self(application))
    }
}

/// Spawns a new thread for the pipeline
///
/// This function is used to spawn a new thread for the pipeline
/// and to pass the stop signal to the pipeline
///
/// # Arguments
///
/// * `pipeline_id` - The id of the pipeline
/// * `stop_signal` - The stop signal to stop the pipeline
///
/// # Returns
///
/// A handle to the thread that runs the pipeline
pub fn spawn_bubbaloop_pipeline(
    stop_signal: Arc<AtomicBool>,
) -> std::thread::JoinHandle<PipelineResult> {
    std::thread::spawn({
        let stop_signal = stop_signal.clone();
        move || -> PipelineResult {
            // parse the ron config string and create the pipeline
            let mut app = BubbaloopPipeline::new()?;

            // create the pipeline and start the tasks
            app.start_all_tasks()?;

            while !stop_signal.load(std::sync::atomic::Ordering::Relaxed) {
                // we run the pipeline iteration step by step
                app.run_one_iteration()?;
            }

            // stop the pipeline and wait for the tasks to finish
            app.stop_all_tasks()?;

            log::debug!("Bubbaloop pipeline stopped");

            Ok(())
        }
    })
}

impl std::ops::Deref for BubbaloopPipeline {
    type Target = BubbaloopApp;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for BubbaloopPipeline {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
