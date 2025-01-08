use std::sync::{Arc, Mutex};

use axum::{
    routing::{get, post},
    Router,
};

use crate::stats;
use crate::{api::handles, compute, pipeline::PipelineStore};

#[derive(Clone)]
pub struct ApiServerState {
    pub store: Arc<Mutex<PipelineStore>>,
}

impl ApiServerState {
    pub fn new() -> Self {
        Self {
            store: Arc::new(Mutex::new(PipelineStore::new())),
        }
    }
}

#[derive(Default)]
pub struct ApiServer;

impl ApiServer {
    pub async fn start(
        &self,
        addr: String,
        state: ApiServerState,
    ) -> Result<(), Box<dyn std::error::Error>> {
        log::info!("🚀 Starting the server");
        log::info!("🔥 Listening on: {}", addr);
        log::info!("🔧 Press Ctrl+C to stop the server");

        let app = Router::new()
            .route("/", get(|| async { "Welcome to Bubbaloop!" }))
            .route("/api/v0/compute/mean_std", post(compute::compute_mean_std))
            .route("/api/v0/stats/whoami", get(stats::whoami))
            .nest(
                "/api/v0/pipeline",
                Router::new()
                    .route("/start", post(handles::start_pipeline))
                    .route("/stop", post(handles::stop_pipeline))
                    .route("/list", get(handles::list_pipelines))
                    .route("/config", get(handles::get_config))
                    .route("/comms", get(handles::get_comms))
                    .with_state(state),
            );

        let listener = tokio::net::TcpListener::bind(addr).await?;
        axum::serve(listener, app).await?;
        Ok(())
    }
}
