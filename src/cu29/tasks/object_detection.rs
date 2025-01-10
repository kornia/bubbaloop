use std::path::PathBuf;

use cu29::prelude::*;
use kornia::{core::CpuAllocator, image::Image, imgproc};

use crate::cu29::msgs::{ImageRGBU8Msg, MeanStdMsg};

const YOLOV8M_URL: &str =
    "https://parcel.pyke.io/v2/cdn/assetdelivery/ortrsv2/ex_models/yolov8m.onnx";

#[derive(Debug, Clone, Copy)]
struct BoundingBox {
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
    score: f32,
    class_id: u32,
}

fn intersection(box1: &BoundingBox, box2: &BoundingBox) -> f32 {
    let x1 = box1.x1.max(box2.x1);
    let y1 = box1.y1.max(box2.y1);
    let x2 = box1.x2.min(box2.x2);
    let y2 = box1.y2.min(box2.y2);
    (x2 - x1).max(0.0) * (y2 - y1).max(0.0)
}

fn union(box1: &BoundingBox, box2: &BoundingBox) -> f32 {
    let area1 = (box1.x2 - box1.x1) * (box1.y2 - box1.y1);
    let area2 = (box2.x2 - box2.x1) * (box2.y2 - box2.y1);
    area1 + area2 - intersection(box1, box2)
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
        let engine = OnnxEngine::new(&PathBuf::from("/home/edgar/Downloads/yolov8m.onnx"))
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

        let img_t = kornia::core::Tensor4::from_shape_vec(
            [1, 640, 640, 3],
            img_resized.as_slice().to_vec(),
            CpuAllocator,
        )
        .expect("Failed to create onnx tensor");

        let mut img_t_f32 =
            kornia::core::Tensor4::<f32, CpuAllocator>::zeros([1, 640, 640, 3], CpuAllocator);
        img_t
            .as_slice()
            .iter()
            .zip(img_t_f32.as_slice_mut())
            .for_each(|(a, b)| *b = *a as f32 / 255.0);

        let img_t_f32 = img_t_f32.permute_axes([0, 3, 1, 2]).as_contiguous();

        let ort_tensor = ort::value::Tensor::from_array((img_t_f32.shape, img_t_f32.into_vec()))
            .expect("Failed to create onnx tensor");

        let outputs = self
            .engine
            .model
            .run(
                ort::inputs! {
                    "images" => ort_tensor
                }
                .unwrap(),
            )
            .unwrap();

        let _output = outputs["output0"].try_extract_raw_tensor::<f32>().unwrap();
        println!("num_boxes: {}", _output.1.len());

        let mut bboxes = Vec::new();

        _output.1.chunks_exact(6).for_each(|data| {
            let score = data[4];
            if score < 0.5 {
                return;
            }
            let xc = data[0] / 640.0 * (img.cols() as f32);
            let yc = data[1] / 640.0 * (img.rows() as f32);
            let w = data[2] / 640.0 * (img.cols() as f32);
            let h = data[3] / 640.0 * (img.rows() as f32);
            let bbox = BoundingBox {
                x1: xc - w / 2.0,
                y1: yc - h / 2.0,
                x2: xc + w / 2.0,
                y2: yc + h / 2.0,
                score,
                class_id: data[5] as u32,
            };
            bboxes.push(bbox);
        });

        bboxes.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());

        let mut bboxes_filtered = Vec::new();
        while !bboxes.is_empty() {
            bboxes_filtered.push(bboxes[0]);
            bboxes = bboxes
                .iter()
                .filter(|box1| intersection(&bboxes[0], &box1) / union(&bboxes[0], &box1) < 0.7)
                .copied()
                .collect();
        }

        println!("bboxes_filtered: {:?}", bboxes_filtered.len());

        let mean_std = MeanStdMsg {
            mean: [0.0, 0.0, 0.0],
            std: [0.0, 0.0, 0.0],
        };

        output.set_payload(mean_std);

        Ok(())
    }
}
