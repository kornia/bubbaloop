use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::paligemma::{Config, Model};
use hf_hub::{Repo, RepoType};
use kornia::image::Image;
use tokenizers::Tokenizer;

use crate::api::handles::pipeline;

use super::TextGeneration;

#[derive(thiserror::Error, Debug)]
pub enum PaligemmaError {
    #[error(transparent)]
    FailedToLoadModel(#[from] hf_hub::api::sync::ApiError),

    #[error(transparent)]
    CandleError(#[from] candle_core::Error),

    #[error(transparent)]
    TokenizerError(#[from] Box<dyn std::error::Error + Send + Sync>),

    #[error(transparent)]
    ImageError(#[from] kornia::image::ImageError),
}

pub struct PaligemmaConfig {
    seed: u64,
    temp: Option<f64>,
    top_p: Option<f64>,
    repeat_penalty: f32,
    repeat_last_n: usize,
}

impl Default for PaligemmaConfig {
    fn default() -> Self {
        Self {
            seed: 299792458,
            temp: Some(0.7),
            top_p: Some(0.9),
            repeat_penalty: 1.1,
            repeat_last_n: 64,
        }
    }
}

pub struct Paligemma {
    pipeline: TextGeneration,
    device: Device,
    dtype: DType,
    config: PaligemmaConfig,
}

impl Paligemma {
    pub fn new(config: PaligemmaConfig) -> Result<Self, PaligemmaError> {
        let device = Device::Cpu;
        let dtype = DType::F32;
        let (model, tokenizer) = Self::load_model(dtype, &device)?;
        let pipeline = TextGeneration::new(
            model,
            tokenizer,
            config.seed,
            config.temp,
            config.top_p,
            config.repeat_penalty,
            config.repeat_last_n,
            &device,
        );
        Ok(Self {
            device,
            dtype,
            pipeline,
            config,
        })
    }
}

impl Paligemma {
    pub fn inference(
        &mut self,
        image: &Image<u8, 3>,
        prompt: &str,
        sample_len: usize,
    ) -> Result<String, PaligemmaError> {
        // resize image to 224x224
        let mut img_224 = Image::from_size_val([224, 224].into(), 0)?;
        kornia::imgproc::resize::resize_fast(
            image,
            &mut img_224,
            kornia::imgproc::interpolation::InterpolationMode::Bilinear,
        )?;

        // convert to tensor with shape [1, 3, 224, 224]
        let image_t = Tensor::from_raw_buffer(
            img_224.as_slice(),
            DType::U8,
            &[img_224.rows(), img_224.cols(), 3],
            &self.device,
        )?
        .permute((2, 0, 1))?
        .to_dtype(DType::F32)?
        .affine(2. / 255., -1.)?
        .unsqueeze(0)?;

        let response = self
            .pipeline
            .run(&image_t, prompt, sample_len)
            .expect("Failed to generate text");

        Ok(response)
    }

    pub fn load_model(dtype: DType, device: &Device) -> Result<(Model, Tokenizer), PaligemmaError> {
        let api = hf_hub::api::sync::Api::new()?;
        let model_id = "google/paligemma-3b-mix-224".to_string();
        let repo = {
            let revision = "main".to_string();
            api.repo(Repo::with_revision(model_id, RepoType::Model, revision))
        };

        let tokenizer_filename = repo.get("tokenizer.json")?;
        let filenames =
            candle_examples::hub_load_safetensors(&repo, "model.safetensors.index.json")?;

        let tokenizer = Tokenizer::from_file(tokenizer_filename)?;

        let config = Config::paligemma_3b_224();
        let vb = unsafe { VarBuilder::from_mmaped_safetensors(&filenames, dtype, &device)? };

        let model = Model::new(&config, vb)?;

        Ok((model, tokenizer))
    }
}
