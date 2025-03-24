use axum::response::{IntoResponse, Json};
use indicatif::ParallelProgressIterator;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::api::models::MeanStdResult;

// Query parameters for the mean and std computation
#[derive(Debug, Deserialize)]
pub struct MeanStdQuery {
    // The directory containing the images
    images_dir: PathBuf,
    // The number of threads to use
    num_threads: Option<usize>,
}

/// The response of the compute mean and std request
#[derive(Debug, Serialize)]
pub enum ComputeResponse {
    Success(MeanStdResult),
    Error { error: String },
}

/// Compute the mean and std of the images in the given directory
pub async fn compute_mean_std(query: Json<MeanStdQuery>) -> impl IntoResponse {
    // Check if the directory exists
    if !query.images_dir.exists() {
        return Json(ComputeResponse::Error {
            error: "The directory does not exist".to_string(),
        });
    }

    // Set the number of threads to use, default to 4
    let num_threads = query.num_threads.unwrap_or(4);

    // Create a local thread pool
    rayon::ThreadPoolBuilder::new()
        .num_threads(num_threads)
        .build()
        .expect("Failed to build thread pool");

    log::debug!("🚀 Walking through the images directory ...");

    // Walk through the images directory and collect the paths of the images
    let images_paths = walkdir::WalkDir::new(&query.images_dir)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry.file_type().is_file()
                && entry
                    .path()
                    .extension()
                    .map(|ext| {
                        ext.eq_ignore_ascii_case("jpg")
                            || ext.eq_ignore_ascii_case("jpeg")
                            || ext.eq_ignore_ascii_case("png")
                    })
                    .unwrap_or(false)
        })
        .map(|entry| entry.path().to_path_buf())
        .collect::<Vec<_>>();

    if images_paths.is_empty() {
        log::debug!("No images found in the directory");
        return Json(ComputeResponse::Error {
            error: "No images found in the directory".to_string(),
        });
    }

    log::debug!(
        "🚀 Found {} images. Starting to compute the std and mean !!!",
        images_paths.len()
    );

    // Create a progress bar
    let pb = indicatif::ProgressBar::new(images_paths.len() as u64);
    pb.set_style(indicatif::ProgressStyle::default_bar().template(
        "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos:>7}/{len:7} ({eta}) {msg} {per_sec}",
    )
    .expect("Failed to set progress bar style")
    .progress_chars("##>-"));

    // compute the std and mean of the images

    let total_std = Arc::new(Mutex::new(vec![0.0; 3]));
    let total_mean = Arc::new(Mutex::new(vec![0.0; 3]));

    let num_samples = images_paths.len() as f64;

    images_paths
        .into_par_iter()
        .progress_with(pb.clone())
        .for_each(|image_path| {
            // read the image
            let image = match kornia::io::functional::read_image_any(&image_path) {
                Ok(image) => image,
                Err(_e) => {
                    log::trace!("Failed to read image: {}", image_path.display());
                    pb.inc(1);
                    return;
                }
            };

            // compute the std and mean
            let (std, mean) = kornia::imgproc::core::std_mean(&image);

            // update the total std and mean

            total_std
                .lock()
                .expect("Failed to lock total std")
                .iter_mut()
                .zip(std.iter())
                .for_each(|(t, s)| *t += s);

            total_mean
                .lock()
                .expect("Failed to lock total mean")
                .iter_mut()
                .zip(mean.iter())
                .for_each(|(t, m)| *t += m);
        });

    // average the measurements
    let total_std = total_std
        .lock()
        .expect("Failed to lock total std")
        .iter()
        .map(|&s| s / num_samples)
        .collect::<Vec<_>>();
    let total_mean = total_mean
        .lock()
        .expect("Failed to lock total mean")
        .iter()
        .map(|&m| m / num_samples)
        .collect::<Vec<_>>();

    log::debug!("🔥Total std: {:?}", total_std);
    log::debug!("🔥Total mean: {:?}", total_mean);

    Json(ComputeResponse::Success(MeanStdResult {
        mean: [
            total_mean[0] as f64,
            total_mean[1] as f64,
            total_mean[2] as f64,
        ],
        std: [
            total_std[0] as f64,
            total_std[1] as f64,
            total_std[2] as f64,
        ],
    }))
}
