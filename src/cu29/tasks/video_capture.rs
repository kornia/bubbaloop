use cu29::prelude::*;
use kornia::io::stream::{CameraCapture, RTSPCameraConfig, V4L2CameraConfig};

use crate::cu29::msgs::ImageRGBU8Msg;

pub struct V4L2Camera {
    cap: CameraCapture,
}

impl V4L2Camera {
    const DEFAULT_CAMERA_ID: u32 = 0;
    const DEFAULT_RES_ROWS: u32 = 480;
    const DEFAULT_RES_COLS: u32 = 640;
    const DEFAULT_FPS: u32 = 30;
}

impl Freezable for V4L2Camera {}

impl<'cl> CuSrcTask<'cl> for V4L2Camera {
    type Output = output_msg!('cl, ImageRGBU8Msg);

    fn new(config: Option<&ComponentConfig>) -> CuResult<Self> {
        let (camera_id, res_rows, res_cols, fps) = if let Some(config) = config {
            let camera_id = config
                .get::<u32>("camera_id")
                .unwrap_or(Self::DEFAULT_CAMERA_ID);
            let res_rows = config.get::<u32>("rows").unwrap_or(Self::DEFAULT_RES_ROWS);
            let res_cols = config.get::<u32>("cols").unwrap_or(Self::DEFAULT_RES_COLS);
            let fps = config.get::<u32>("fps").unwrap_or(Self::DEFAULT_FPS);
            (camera_id, res_rows, res_cols, fps)
        } else {
            (
                Self::DEFAULT_CAMERA_ID,
                Self::DEFAULT_RES_ROWS,
                Self::DEFAULT_RES_COLS,
                Self::DEFAULT_FPS,
            )
        };

        let cap = V4L2CameraConfig::new()
            .with_camera_id(camera_id)
            .with_fps(fps)
            .with_size([res_cols as usize, res_rows as usize].into())
            .build()
            .map_err(|e| CuError::new_with_cause("Failed to build camera", e))?;

        Ok(Self { cap })
    }

    fn start(&mut self, _clock: &RobotClock) -> CuResult<()> {
        self.cap
            .start()
            .map_err(|e| CuError::new_with_cause("Failed to start camera", e))
    }

    fn stop(&mut self, _clock: &RobotClock) -> CuResult<()> {
        self.cap
            .close()
            .map_err(|e| CuError::new_with_cause("Failed to stop camera", e))
    }

    fn process(&mut self, _clock: &RobotClock, output: Self::Output) -> CuResult<()> {
        let Some(image) = self
            .cap
            .grab()
            .map_err(|e| CuError::new_with_cause("Failed to grab image", e))?
        else {
            return Ok(());
        };

        output.set_payload(ImageRGBU8Msg { image });

        Ok(())
    }
}

pub struct RTSPCamera {
    cap: CameraCapture,
}

impl RTSPCamera {
    const DEFAULT_URL: &str = "some_mandatory_url";
}

impl Freezable for RTSPCamera {}

impl<'cl> CuSrcTask<'cl> for RTSPCamera {
    type Output = output_msg!('cl, ImageRGBU8Msg);

    fn new(config: Option<&ComponentConfig>) -> CuResult<Self> {
        let url = if let Some(config) = config {
            config
                .get::<String>("url")
                .unwrap_or(Self::DEFAULT_URL.to_string())
        } else {
            Self::DEFAULT_URL.to_string()
        };

        let cap = RTSPCameraConfig::new()
            .with_url(&url)
            .build()
            .map_err(|e| CuError::new_with_cause("Failed to build camera", e))?;

        Ok(Self { cap })
    }

    fn start(&mut self, _clock: &RobotClock) -> CuResult<()> {
        self.cap
            .start()
            .map_err(|e| CuError::new_with_cause("Failed to start camera", e))
    }

    fn stop(&mut self, _clock: &RobotClock) -> CuResult<()> {
        self.cap
            .close()
            .map_err(|e| CuError::new_with_cause("Failed to stop camera", e))
    }

    fn process(&mut self, _clock: &RobotClock, output: Self::Output) -> CuResult<()> {
        let Some(image) = self
            .cap
            .grab()
            .map_err(|e| CuError::new_with_cause("Failed to grab image", e))?
        else {
            return Ok(());
        };

        output.set_payload(ImageRGBU8Msg { image });

        Ok(())
    }
}
