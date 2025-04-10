use crate::cu29::msgs::{EncodedImage, ImageRgb8Msg};
use cu29::prelude::*;
use kornia::io::jpeg::ImageEncoder as KorniaImageEncoder;

pub struct ImageEncoder {
    encoder: KorniaImageEncoder,
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
            encoder: KorniaImageEncoder::new()
                .map_err(|e| CuError::new_with_cause("Failed to create jpeg encoder", e))?,
        })
    }

    fn process(
        &mut self,
        _clock: &RobotClock,
        input: Self::Input,
        output: Self::Output,
    ) -> Result<(), CuError> {
        let Some(img) = input.payload() else {
            return Ok(());
        };

        let encoded_image = self
            .encoder
            .encode(img)
            .map_err(|e| CuError::new_with_cause("Failed to encode image", e))?;

        output.metadata.tov = input.metadata.tov;
        output.set_payload(EncodedImage {
            data: encoded_image,
            encoding: "jpeg".to_string(),
        });

        Ok(())
    }
}
