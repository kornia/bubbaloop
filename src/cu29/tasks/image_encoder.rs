use crate::cu29::msgs::{EncodedImage, ImageRgb8Msg};
use cu29::prelude::*;
use kornia_io::jpegturbo::JpegTurboEncoder;

pub struct ImageEncoder {
    encoder: JpegTurboEncoder,
}

impl Freezable for ImageEncoder {}

impl<'cl> CuTask<'cl> for ImageEncoder {
    type Input = input_msg!('cl, ImageRgb8Msg);
    type Output = output_msg!('cl, EncodedImage);

    fn new(_config: Option<&ComponentConfig>) -> Result<Self, CuError>
    where
        Self: Sized,
    {
        Ok(Self {
            encoder: JpegTurboEncoder::new()
                .map_err(|e| CuError::new_with_cause("Failed to create jpeg encoder", e))?,
        })
    }

    fn process(
        &mut self,
        _clock: &RobotClock,
        input: Self::Input,
        output: Self::Output,
    ) -> Result<(), CuError> {
        let Some(msg) = input.payload() else {
            return Ok(());
        };

        let encoded_image = self
            .encoder
            .encode_rgb8(&msg.image)
            .map_err(|e| CuError::new_with_cause("Failed to encode image", e))?;

        output.set_payload(EncodedImage {
            stamp_ns: msg.stamp_ns,
            channel_id: msg.channel_id,
            data: encoded_image,
            encoding: "jpeg".to_string(),
        });

        Ok(())
    }
}
