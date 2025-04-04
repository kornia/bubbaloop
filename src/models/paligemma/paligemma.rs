use super::{TextGeneration, TextGenerationConfig};
use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::paligemma::{Config, Model};
use hf_hub::{Repo, RepoType};
use kornia::image::Image;
use tokenizers::Tokenizer;

#[derive(thiserror::Error, Debug)]
pub enum PaligemmaError {
    #[error(transparent)]
    FailedToLoadModel(#[from] hf_hub::api::sync::ApiError),

    #[error(transparent)]
    CandleError(#[from] candle_core::Error),

    #[error(transparent)]
    ImageError(#[from] kornia::image::ImageError),

    #[error(transparent)]
    TokenizerError(#[from] tokenizers::Error),

    #[error(transparent)]
    IoError(#[from] std::io::Error),

    #[error("Cannot find the <eos> token")]
    EosTokenNotFound,
}

pub struct PaligemmaConfig {
    pub seed: u64,
    pub temp: Option<f64>,
    pub top_p: Option<f64>,
    pub repeat_penalty: f32,
    pub repeat_last_n: usize,
    pub use_cuda: bool,
}

impl From<PaligemmaConfig> for TextGenerationConfig {
    fn from(config: PaligemmaConfig) -> Self {
        TextGenerationConfig {
            seed: config.seed,
            temp: config.temp,
            top_p: config.top_p,
            repeat_penalty: config.repeat_penalty,
            repeat_last_n: config.repeat_last_n,
        }
    }
}

impl Default for PaligemmaConfig {
    fn default() -> Self {
        Self {
            seed: 299792458,
            temp: Some(0.7),
            top_p: Some(0.9),
            repeat_penalty: 1.1,
            repeat_last_n: 64,
            use_cuda: false,
        }
    }
}

pub struct Paligemma {
    pipeline: TextGeneration,
    img_buf: Image<u8, 3>,
    dtype: DType,
}

impl Paligemma {
    pub fn new(config: PaligemmaConfig) -> Result<Self, PaligemmaError> {
        let device = if config.use_cuda {
            Device::cuda_if_available(0).unwrap_or(Device::Cpu)
        } else {
            Device::Cpu
        };
        // TODO: experiment more with the dtype
        //let dtype = DType::BF16;
        let dtype = DType::F32;

        let (model, tokenizer) = Self::load_model(dtype, &device)?;
        let img_buf = Image::from_size_val([224, 224].into(), 0)?;
        let pipeline = TextGeneration::new(model, tokenizer, device, config.into());

        Ok(Self {
            pipeline,
            img_buf,
            dtype,
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
        kornia::imgproc::resize::resize_fast(
            image,
            &mut self.img_buf,
            kornia::imgproc::interpolation::InterpolationMode::Bilinear,
        )?;

        // convert to tensor with shape [1, 3, 224, 224]
        let image_t = Tensor::from_raw_buffer(
            self.img_buf.as_slice(),
            DType::U8,
            &[self.img_buf.rows(), self.img_buf.cols(), 3],
            self.pipeline.device(),
        )?
        .to_dtype(self.dtype)?
        .permute((2, 0, 1))?
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
        let vb = unsafe { VarBuilder::from_mmaped_safetensors(&filenames, dtype, device)? };

        let model = Model::new(&config, vb)?;

        Ok((model, tokenizer))
    }
}
