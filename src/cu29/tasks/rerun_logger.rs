use crate::cu29::msgs::{EncodedImage, ImageRgb8Msg};
use cu29::prelude::*;
use std::path::PathBuf;

pub struct RerunLogger {
    rec: rerun::RecordingStream,
}

impl Freezable for RerunLogger {}

impl<'cl> CuSinkTask<'cl> for RerunLogger {
    type Input = input_msg!('cl, EncodedImage);

    fn new(config: Option<&ComponentConfig>) -> Result<Self, CuError>
    where
        Self: Sized,
    {
        let config = config.expect("config is required");

        // create the path to the rerun file
        let path = config.get::<String>("path").expect("path is required");
        let rec_path = {
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            PathBuf::from(path).join(format!("{}.rrd", timestamp))
        };

        let rec = rerun::RecordingStreamBuilder::new("rerun_logger")
            .save(rec_path)
            .map_err(|e| CuError::new_with_cause("Failed to spawn rerun stream", e))?;

        Ok(Self { rec })
    }

    fn process(&mut self, _clock: &RobotClock, input: Self::Input) -> Result<(), CuError> {
        if let Some(image) = input.payload() {
            // log the image to the local rerun server
            log_image_encoded(&self.rec, "image", image)?;
        }
        Ok(())
    }
}

fn _log_image_rgb8(
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

fn log_image_encoded(
    rec: &rerun::RecordingStream,
    name: &str,
    img: &EncodedImage,
) -> Result<(), CuError> {
    rec.log(
        name,
        &rerun::EncodedImage::from_file_contents(img.data.clone()),
    )
    .map_err(|e| CuError::new_with_cause("Failed to log image", e))?;
    Ok(())
}
