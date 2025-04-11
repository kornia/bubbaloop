use crate::{
    api::handles,
    pipeline::{PipelineStore, ResultStore},
};
use axum::{
    routing::{get, post},
    Router,
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
            .route(
                "/api/v0/compute/mean_std",
                post(handles::compute::compute_mean_std),
            )
            .nest(
                "/api/v0/inference",
                Router::new()
                    .route(
                        "/result/{channel_id}",
                        get(handles::inference::get_inference_result),
                    )
                    .route(
                        "/settings",
                        post(handles::inference::post_inference_settings),
                    )
                    .route(
                        "/image/{channel_id}",
                        get(handles::camera::get_camera_image),
                    )
                    .with_state(result_store),
            )
            .route("/api/v0/stats/whoami", get(handles::stats::whoami))
            .nest(
                "/api/v0/pipeline",
                Router::new()
                    .route("/start", post(handles::pipeline::start_pipeline))
                    .route("/stop", post(handles::pipeline::stop_pipeline))
                    .route("/list", get(handles::pipeline::list_pipelines))
                    .route("/config", get(handles::pipeline::get_config))
                    .with_state(pipeline_store),
            );

        let listener = tokio::net::TcpListener::bind(addr).await?;
        axum::serve(listener, app).await?;
        Ok(())
    }
}
