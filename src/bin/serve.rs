use argh::FromArgs;

// defaults for the server
const DEFAULT_HOST: &str = "0.0.0.0";
const DEFAULT_PORT: u16 = 3000;

#[derive(FromArgs)]
#[argh(description = "Bubbaloop server")]
struct CLIArgs {
    #[argh(option, short = 'h', default = "DEFAULT_HOST.to_string()")]
    /// the host to listen on
    host: String,

    #[argh(option, short = 'p', default = "DEFAULT_PORT")]
    /// the port to listen on
    port: u16,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let args: CLIArgs = argh::from_env();

    // format the host and port
    let addr = format!("{}:{}", args.host, args.port);

    let global_state = bubbaloop::pipeline::SERVER_GLOBAL_STATE.clone();

    // start the api server
    let api = bubbaloop::api::ApiServer;
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async move {
        api.start(addr, global_state.pipeline_store, global_state.result_store)
            .await
            .unwrap();
    });

    Ok(())
}
