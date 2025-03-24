use axum::{
    routing::{get, post},
    Router,
};

use crate::{
    api::handles,
    compute, inference,
    pipeline::{PipelineStore, ResultStore},
    stats,
};

#[derive(Default)]
pub struct ApiServer;

impl ApiServer {
    pub async fn start(
        &self,
        addr: String,
        pipeline_store: PipelineStore,
        result_store: ResultStore,
    ) -> Result<(), Box<dyn std::error::Error>> {
        log::info!("🚀 Starting the server");
        log::info!("🔥 Listening on: {}", addr);
        log::info!("🔧 Press Ctrl+C to stop the server");

        let app = Router::new()
            .route("/", get(|| async { "Welcome to Bubbaloop!" }))
            .route("/api/v0/compute/mean_std", post(compute::compute_mean_std))
            .nest(
                "/api/v0/inference",
                Router::new()
                    .route("/result", get(inference::get_inference_result))
                    .with_state(result_store),
            )
            .route("/api/v0/stats/whoami", get(stats::whoami))
            .nest(
                "/api/v0/pipeline",
                Router::new()
                    .route("/start", post(handles::start_pipeline))
                    .route("/stop", post(handles::stop_pipeline))
                    .route("/list", get(handles::list_pipelines))
                    .route("/config", get(handles::get_config))
                    .with_state(pipeline_store),
            );

        let listener = tokio::net::TcpListener::bind(addr).await?;
        axum::serve(listener, app).await?;
        Ok(())
    }
}
