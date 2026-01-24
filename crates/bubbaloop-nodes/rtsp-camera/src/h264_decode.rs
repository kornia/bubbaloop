use gstreamer::prelude::*;
use thiserror::Error;

/// Errors that can occur during H264 decoding
#[derive(Debug, Error)]
pub enum H264DecodeError {
    #[error("GStreamer error: {0}")]
    GStreamer(#[from] gstreamer::glib::Error),

    #[error("GStreamer state change error: {0}")]
    StateChange(#[from] gstreamer::StateChangeError),

    #[error("Failed to get element by name")]
    ElementNotFound,

    #[error("Failed to downcast element")]
    DowncastError,

    #[error("Failed to get buffer from sample")]
    BufferError,

    #[error("Failed to get caps from sample")]
    CapsError,

    #[error("Failed to push buffer to appsrc")]
    PushError,

    #[error("Channel disconnected")]
    ChannelDisconnected,
}

/// Decoder backend selection
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DecoderBackend {
    /// Software decoding using avdec_h264 (CPU, always available)
    #[default]
    Software,
    /// Hardware decoding using nvh264dec (NVIDIA desktop GPU)
    Nvidia,
    /// Hardware decoding using nvv4l2decoder (NVIDIA Jetson)
    Jetson,
}

impl DecoderBackend {
    /// Get the GStreamer pipeline segment for decoder + converter + scaler
    /// Returns (segment, uses_gpu_scale) - if uses_gpu_scale is true, width/height
    /// should be specified in the segment's caps filter, not after videoscale
    pub fn pipeline_segment(&self, width: u32, height: u32) -> (String, bool) {
        match self {
            // Software: CPU decode + CPU scale
            DecoderBackend::Software => (
                "avdec_h264 ! videoconvert ! videoscale".to_string(),
                false, // needs videoscale caps after
            ),
            // NVIDIA desktop: GPU decode + GPU scale via nvvidconv
            DecoderBackend::Nvidia => (
                format!(
                    "nvh264dec ! nvvidconv ! video/x-raw,format=RGBA,width={width},height={height}"
                ),
                true, // scaling already done in GPU
            ),
            // Jetson: GPU decode + GPU scale via nvvidconv
            // nvv4l2decoder outputs to NVMM memory, nvvidconv does GPU scaling
            DecoderBackend::Jetson => (
                format!(
                    "nvv4l2decoder enable-max-performance=1 ! \
                     nvvidconv ! video/x-raw,format=RGBA,width={width},height={height}"
                ),
                true, // scaling already done in GPU
            ),
        }
    }
}

/// A decoded raw video frame (RGB24)
#[derive(Clone)]
pub struct RawFrame {
    /// Raw RGB24 pixel data
    pub data: Vec<u8>,
    /// Frame width in pixels
    pub width: u32,
    /// Frame height in pixels
    pub height: u32,
    /// Presentation timestamp in nanoseconds
    pub pts: u64,
    /// Frame sequence number
    pub sequence: u32,
    /// Format of the frame
    pub format: String,
    /// Step in bytes
    pub step: u32,
}

/// Decodes H264 NAL units to raw RGB frames using GStreamer
///
/// Uses pipeline: appsrc -> h264parse -> decoder -> videoconvert -> appsink
pub struct VideoH264Decoder {
    pipeline: gstreamer::Pipeline,
    appsrc: gstreamer_app::AppSrc,
    frame_rx: flume::Receiver<RawFrame>,
}

impl VideoH264Decoder {
    /// Create a new H264 decoder with the specified backend
    ///
    /// # Arguments
    ///
    /// * `backend` - Decoder backend to use (Software or Nvidia)
    pub fn new(backend: DecoderBackend, height: u32, width: u32) -> Result<Self, H264DecodeError> {
        // Initialize GStreamer if not already initialized
        if !gstreamer::INITIALIZED.load(std::sync::atomic::Ordering::Relaxed) {
            gstreamer::init()?;
        }

        let (decoder_segment, gpu_scaled) = backend.pipeline_segment(width, height);

        // Build pipeline for H264 decoding to RGBA at specified resolution
        // GPU backends (NVIDIA/Jetson) include scaling in their segment
        // Software backend needs videoscale + caps filter after
        let pipeline_desc = if gpu_scaled {
            format!(
                "appsrc name=src is-live=true format=3 ! \
                 h264parse ! \
                 {decoder_segment} ! \
                 appsink name=sink emit-signals=true sync=false"
            )
        } else {
            format!(
                "appsrc name=src is-live=true format=3 ! \
                 h264parse ! \
                 {decoder_segment} ! \
                 video/x-raw,format=RGBA,width={width},height={height} ! \
                 appsink name=sink emit-signals=true sync=false"
            )
        };

        log::debug!("Creating H264 decoder pipeline: {}", pipeline_desc);

        let pipeline = gstreamer::parse::launch(&pipeline_desc)?
            .dynamic_cast::<gstreamer::Pipeline>()
            .map_err(|_| H264DecodeError::DowncastError)?;

        // Get appsrc for pushing H264 data
        let appsrc = pipeline
            .by_name("src")
            .ok_or(H264DecodeError::ElementNotFound)?
            .dynamic_cast::<gstreamer_app::AppSrc>()
            .map_err(|_| H264DecodeError::DowncastError)?;

        // Get appsink for receiving decoded frames
        let appsink = pipeline
            .by_name("sink")
            .ok_or(H264DecodeError::ElementNotFound)?
            .dynamic_cast::<gstreamer_app::AppSink>()
            .map_err(|_| H264DecodeError::DowncastError)?;

        // Channel for decoded frames
        let (frame_tx, frame_rx) = flume::unbounded::<RawFrame>();

        // Sequence counter shared with callback
        let sequence = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let seq_clone = sequence.clone();

        // Set up callback to receive decoded frames
        appsink.set_callbacks(
            gstreamer_app::AppSinkCallbacks::builder()
                .new_sample(
                    move |sink| match Self::handle_decoded_sample(sink, &seq_clone) {
                        Ok(frame) => {
                            let _ = frame_tx.send(frame);
                            Ok(gstreamer::FlowSuccess::Ok)
                        }
                        Err(e) => {
                            log::error!("[H264Decoder] Error handling decoded sample: {}", e);
                            Err(gstreamer::FlowError::Error)
                        }
                    },
                )
                .build(),
        );

        // Start the pipeline
        pipeline.set_state(gstreamer::State::Playing)?;

        log::info!(
            "H264 decoder initialized with {} backend ({}x{}, {})",
            match backend {
                DecoderBackend::Software => "software (avdec_h264)",
                DecoderBackend::Nvidia => "NVIDIA desktop (nvh264dec)",
                DecoderBackend::Jetson => "NVIDIA Jetson (nvv4l2decoder)",
            },
            width,
            height,
            if gpu_scaled {
                "GPU scaling"
            } else {
                "CPU scaling"
            }
        );

        Ok(Self {
            pipeline,
            appsrc,
            frame_rx,
        })
    }

    /// Handle a decoded sample from appsink
    fn handle_decoded_sample(
        appsink: &gstreamer_app::AppSink,
        sequence: &std::sync::atomic::AtomicU32,
    ) -> Result<RawFrame, H264DecodeError> {
        let sample = appsink
            .pull_sample()
            .map_err(|_| H264DecodeError::BufferError)?;

        // Get video info from caps
        let caps = sample.caps().ok_or(H264DecodeError::CapsError)?;
        let video_info =
            gstreamer_video::VideoInfo::from_caps(caps).map_err(|_| H264DecodeError::CapsError)?;

        let width = video_info.width();
        let height = video_info.height();

        let buffer = sample.buffer().ok_or(H264DecodeError::BufferError)?;

        // Get PTS
        let pts = buffer.pts().map(|p| p.nseconds()).unwrap_or(0);

        // Map buffer for reading
        let map = buffer
            .map_readable()
            .map_err(|_| H264DecodeError::BufferError)?;

        let seq = sequence.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(RawFrame {
            data: map.as_slice().to_vec(),
            width,
            height,
            pts,
            sequence: seq,
            format: "RGBA".to_string(),
            step: width * 4,
        })
    }

    /// Push H264 data for decoding
    ///
    /// # Arguments
    ///
    /// * `h264_data` - H264 NAL units in Annex B format
    /// * `pts` - Presentation timestamp in nanoseconds
    /// * `keyframe` - Whether this is a keyframe (IDR frame)
    pub fn push(&self, h264_data: &[u8], pts: u64, keyframe: bool) -> Result<(), H264DecodeError> {
        let mut buffer = gstreamer::Buffer::with_size(h264_data.len())
            .map_err(|_| H264DecodeError::BufferError)?;

        {
            let buffer_ref = buffer.get_mut().ok_or(H264DecodeError::BufferError)?;

            // Set timestamp
            buffer_ref.set_pts(gstreamer::ClockTime::from_nseconds(pts));

            // Set flags
            if !keyframe {
                buffer_ref.set_flags(gstreamer::BufferFlags::DELTA_UNIT);
            }

            // Copy data
            let mut map = buffer_ref
                .map_writable()
                .map_err(|_| H264DecodeError::BufferError)?;
            map.as_mut_slice().copy_from_slice(h264_data);
        }

        // Push to appsrc
        self.appsrc
            .push_buffer(buffer)
            .map_err(|_| H264DecodeError::PushError)?;

        Ok(())
    }

    /// Get the receiver for decoded frames
    ///
    /// Use this with async runtimes:
    /// ```ignore
    /// while let Ok(frame) = decoder.receiver().recv_async().await {
    ///     // Process decoded frame
    /// }
    /// ```
    pub fn receiver(&self) -> &flume::Receiver<RawFrame> {
        &self.frame_rx
    }

    /// Close the decoder pipeline
    pub fn close(&self) -> Result<(), H264DecodeError> {
        // Send EOS
        self.appsrc.end_of_stream().ok();
        self.pipeline.set_state(gstreamer::State::Null)?;
        Ok(())
    }
}

impl Drop for VideoH264Decoder {
    fn drop(&mut self) {
        if let Err(e) = self.close() {
            log::error!("Error closing H264 decoder: {}", e);
        }
    }
}
