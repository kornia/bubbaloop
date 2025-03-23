use std::net::SocketAddr;
use std::str::FromStr;

use crate::cu29::msgs::ImageRgb8Msg;
use cu29::prelude::*;

pub struct RerunLogger {
    rec_stats: rerun::RecordingStream,
    rec_viz: Option<rerun::RecordingStream>,
}

impl Freezable for RerunLogger {}

impl<'cl> CuSinkTask<'cl> for RerunLogger {
    type Input = input_msg!('cl, ImageRgb8Msg);

    fn new(config: Option<&ComponentConfig>) -> Result<Self, CuError>
    where
        Self: Sized,
    {
        let config = config.expect("config is required");

        // stream for local recording
        let path = config.get::<String>("path").expect("path is required");
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

        // create the remote rerun server stream
        let rec_viz = match (config.get::<String>("ip"), config.get::<u32>("port")) {
            (Some(ip), Some(port)) => {
                let addr = SocketAddr::from_str(&format!("{}:{}", ip, port))
                    .map_err(|e| CuError::new_with_cause("Failed to parse socket address", e))?;

                // stream for remote visualization
                Some(
                    rerun::RecordingStreamBuilder::new("bubbaloop")
                        .connect_tcp_opts(addr, None)
                        .map_err(|e| CuError::new_with_cause("Failed to spawn rerun stream", e))?,
                )
            }
            _ => None,
        };

        Ok(Self { rec_viz, rec_stats })
    }

    fn process(&mut self, _clock: &RobotClock, input: Self::Input) -> Result<(), CuError> {
        let Some(image) = input.payload() else {
            return Ok(());
        };

        // log the image to the local rerun server
        log_image_rgb(&self.rec_stats, "image", image)?;

        // log the image to the remote rerun server if it is connected
        if let Some(rec_viz) = &self.rec_viz {
            log_image_rgb(rec_viz, "image", image)?;
        }

        Ok(())
    }
}

fn log_image_rgb(
    rec: &rerun::RecordingStream,
    name: &str,
    img: &ImageRgb8Msg,
) -> Result<(), CuError> {
    rec.log(
        name,
        &rerun::Image::from_elements(img.as_slice(), img.size().into(), rerun::ColorModel::RGB),
    )
    .map_err(|e| CuError::new_with_cause("Failed to log image", e))?;
    Ok(())
}
