use std::net::SocketAddr;
use std::str::FromStr;

use crate::cu29::msgs::{ImageRGBU8, ImageRGBU8Msg};
use cu29::prelude::*;

// default values
const DEFAULT_IP: &str = "127.0.0.1";
const DEFAULT_PORT: u32 = 9876;

pub struct RerunLogger {
    rec_viz: rerun::RecordingStream,
    rec_stats: rerun::RecordingStream,
}

impl Freezable for RerunLogger {}

impl<'cl> CuSinkTask<'cl> for RerunLogger {
    type Input = input_msg!('cl, ImageRGBU8Msg);

    fn new(config: Option<&ComponentConfig>) -> Result<Self, CuError>
    where
        Self: Sized,
    {
        let config = config.expect("config is required");

        let (ip, port) = (
            config.get::<String>("ip").unwrap_or(DEFAULT_IP.to_string()),
            config.get::<u32>("port").unwrap_or(DEFAULT_PORT),
        );

        let path = config.get::<String>("path").expect("path is required");

        let addr = SocketAddr::from_str(format!("{}:{}", ip, port).as_str())
            .map_err(|e| CuError::new_with_cause("Failed to parse socket address", e))?;

        // stream for remote visualization
        let rec_viz = rerun::RecordingStreamBuilder::new("bubbaloop")
            .connect_tcp_opts(addr, None)
            .map_err(|e| CuError::new_with_cause("Failed to spawn rerun stream", e))?;

        // stream for local recording
        let rec_path = {
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            format!("{}/{}.rrd", path, timestamp)
        };

        let rec_stats = rerun::RecordingStreamBuilder::new("bubbaloop_stats")
            .save(rec_path)
            .map_err(|e| CuError::new_with_cause("Failed to spawn rerun stream", e))?;

        Ok(Self { rec_viz, rec_stats })
    }

    fn process(&mut self, _clock: &RobotClock, input: Self::Input) -> Result<(), CuError> {
        let Some(ImageRGBU8Msg { image }) = input.payload() else {
            return Ok(());
        };

        log_image_rgb(&self.rec_viz, "image", image)?;
        log_image_rgb(&self.rec_stats, "image", image)?;

        Ok(())
    }
}

fn log_image_rgb(
    rec: &rerun::RecordingStream,
    name: &str,
    img: &ImageRGBU8,
) -> Result<(), CuError> {
    rec.log(
        name,
        &rerun::Image::from_elements(img.as_slice(), img.size().into(), rerun::ColorModel::RGB),
    )
    .map_err(|e| CuError::new_with_cause("Failed to log image", e))?;
    Ok(())
}
