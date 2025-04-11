use crate::cu29::msgs::ImageRgb8Msg;
use cu29::prelude::*;
use kornia::io::stream::video::{ImageFormat, VideoCodec, VideoWriter as KorniaVideoWriter};

// default values for the video writer
const DEFAULT_RES_ROWS: u32 = 480;
const DEFAULT_RES_COLS: u32 = 640;
const DEFAULT_FPS: u32 = 30;

pub struct VideoWriter {
    writer: Option<KorniaVideoWriter>,
}

impl Freezable for VideoWriter {}

impl<'cl> CuSinkTask<'cl> for VideoWriter {
    type Input = input_msg!('cl, ImageRgb8Msg);

    fn new(config: Option<&ComponentConfig>) -> CuResult<Self>
    where
        Self: Sized,
    {
        // generate path file based on the current timestamp
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let path = format!("video_{}.mp4", timestamp);

        let (res_rows, res_cols, fps) = if let Some(config) = config {
            let res_cols = config.get::<u32>("res_cols").unwrap_or(DEFAULT_RES_COLS);
            let res_rows = config.get::<u32>("res_rows").unwrap_or(DEFAULT_RES_ROWS);
            let fps = config.get::<u32>("fps").unwrap_or(DEFAULT_FPS);
            (res_rows, res_cols, fps)
        } else {
            (DEFAULT_RES_ROWS, DEFAULT_RES_COLS, DEFAULT_FPS)
        };

        let writer = KorniaVideoWriter::new(
            path,
            VideoCodec::H264,
            ImageFormat::Rgb8,
            fps as i32,
            [res_cols as usize, res_rows as usize].into(),
        )
        .map_err(|e| CuError::new_with_cause("Failed to create video writer", e))?;

        Ok(Self {
            writer: Some(writer),
        })
    }

    fn start(&mut self, _clock: &RobotClock) -> CuResult<()> {
        let Some(writer) = self.writer.as_mut() else {
            return Ok(());
        };

        writer
            .start()
            .map_err(|e| CuError::new_with_cause("Failed to start video writer", e))?;

        Ok(())
    }

    fn stop(&mut self, _clock: &RobotClock) -> CuResult<()> {
        let Some(writer) = self.writer.as_mut() else {
            return Ok(());
        };

        writer
            .close()
            .map_err(|e| CuError::new_with_cause("Failed to close video writer", e))?;

        self.writer = None; // drop the writer

        Ok(())
    }

    fn process(&mut self, _clock: &RobotClock, input: Self::Input) -> CuResult<()> {
        let Some(msg) = input.payload() else {
            return Ok(());
        };

        let Some(writer) = self.writer.as_mut() else {
            return Ok(());
        };

        writer
            .write(&msg.image)
            .map_err(|e| CuError::new_with_cause("Failed to write image", e))?;

        Ok(())
    }
}
