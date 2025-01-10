use std::path::PathBuf;

use cu29::prelude::*;
use kornia::{core::CpuAllocator, image::Image, imgproc};

use crate::cu29::msgs::{ImageRGBU8Msg, MeanStdMsg};

#[derive(Debug, Clone, Copy)]
struct BoundingBox {
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
    score: f32,
    class_id: u32,
}

struct OnnxEngine {
    model: ort::session::Session,
}

impl OnnxEngine {
    fn new(model_path: &std::path::PathBuf) -> Result<Self, Box<dyn std::error::Error>> {
        let model = ort::session::Session::builder()?
            .with_optimization_level(ort::session::builder::GraphOptimizationLevel::Level3)?
            .with_intra_threads(4)?
            .commit_from_file(model_path)?;

        Ok(Self { model })
    }
}

pub struct ObjectDetection {
    engine: OnnxEngine,
}

impl Freezable for ObjectDetection {}

impl<'cl> CuTask<'cl> for ObjectDetection {
    type Input = input_msg!('cl, ImageRGBU8Msg);
    type Output = output_msg!('cl, MeanStdMsg);

    fn new(_config: Option<&ComponentConfig>) -> CuResult<Self>
    where
        Self: Sized,
    {
        let engine = OnnxEngine::new(&PathBuf::from("/home/edgar/Downloads/model.onnx"))
            .expect("Failed to create onnx engine");
        Ok(Self { engine })
    }

    fn process(
        &mut self,
        _clock: &RobotClock,
        input: Self::Input,
        output: Self::Output,
    ) -> CuResult<()> {
        let Some(msg) = input.payload() else {
            return Ok(());
        };

        let img = &msg.image;

        let mut img_resized = Image::from_size_val([640, 640].into(), 0u8).unwrap();
        kornia::imgproc::resize::resize_fast(
            img,
            &mut img_resized,
            kornia::imgproc::interpolation::InterpolationMode::Bilinear,
        )
        .expect("Failed to resize image");

        let img_resized_f32 = img_resized
            .cast_and_scale(1.0 / 255.0)
            .expect("Failed to cast image");

        let mut normalized_img = Image::from_size_val([640, 640].into(), 0f32).unwrap();
        imgproc::normalize::normalize_mean_std(
            &img_resized_f32,
            &mut normalized_img,
            &[0.485, 0.456, 0.406],
            &[0.229, 0.224, 0.225],
        )
        .expect("Failed to normalize image");

        //let img_t = kornia::core::Tensor4::from_shape_vec(
        //    [1, 640, 640, 3],
        //    normalized_img.into_vec(),
        //    CpuAllocator,
        //)
        //.expect("Failed to create onnx tensor");

        let img_t = unsafe {
            kornia::core::Tensor4::from_raw_parts(
                [1, 640, 640, 3],
                normalized_img.as_ptr(),
                normalized_img.numel(),
                CpuAllocator,
            )
        }
        .expect("Failed to create onnx tensor");
        std::mem::forget(normalized_img);

        // convert from BHWC to BCHW
        let img_t = img_t.permute_axes([0, 3, 1, 2]).as_contiguous();

        let ort_tensor = ort::value::Tensor::from_array((img_t.shape, img_t.into_vec()))
            .expect("Failed to create onnx tensor");

        let outputs = self
            .engine
            .model
            .run(
                ort::inputs! {
                    "pixel_values" => ort_tensor
                }
                .unwrap(),
            )
            .unwrap();

        let (axes, data) = outputs["pred_boxes"]
            .try_extract_raw_tensor::<f32>()
            .unwrap();
        println!("axes: {:?}", axes);
        println!("num_boxes: {}", data.len());

        let bboxes = data.chunks_exact(4).map(|data| BoundingBox {
            x1: data[0],
            y1: data[1],
            x2: data[2],
            y2: data[3],
            score: 0.0,
            class_id: 0,
        });

        let bboxes_filtered = bboxes.filter(|bbox| bbox.score > 0.5).collect::<Vec<_>>();

        println!("bboxes_filtered: {:?}", bboxes_filtered.len());

        let mean_std = MeanStdMsg {
            mean: [0.0, 0.0, 0.0],
            std: [0.0, 0.0, 0.0],
        };

        output.set_payload(mean_std);

        Ok(())
    }
}
