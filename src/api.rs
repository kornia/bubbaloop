use axum::{
    routing::{get, post},
    Router,
};

use crate::stats;
use crate::{
    compute,
    pipeline::{self, PipelineStore},
};

#[derive(Default)]
pub struct ApiServer;

impl ApiServer {
    pub async fn start(
        &self,
        addr: String,
        store: PipelineStore,
    ) -> Result<(), Box<dyn std::error::Error>> {
        log::info!("🚀 Starting the server");
        log::info!("🔥 Listening on: {}", addr);
        log::info!("🔧 Press Ctrl+C to stop the server");

        let app = Router::new()
            .route("/", get(|| async { "Welcome to Bubbaloop!" }))
            .route("/api/v0/compute/mean_std", post(compute::compute_mean_std))
            .route("/api/v0/stats/whoami", get(stats::whoami))
            .route("/api/v0/pipeline/start", post(pipeline::start_pipeline))
            .route("/api/v0/pipeline/stop", post(pipeline::stop_pipeline))
            .route("/api/v0/pipeline/list", get(pipeline::list_pipelines))
            .route("/api/v0/pipeline/config", get(pipeline::get_config))
            .with_state(store);

        let listener = tokio::net::TcpListener::bind(addr).await?;
        axum::serve(listener, app).await?;
        Ok(())
    }
}
