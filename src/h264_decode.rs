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
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DecoderBackend {
    /// Software decoding using avdec_h264 (CPU, always available)
    #[default]
    Software,
    /// Hardware decoding using nvh264dec (NVIDIA GPU, requires drivers)
    Nvidia,
}

impl DecoderBackend {
    /// Get the GStreamer element name for this backend
    fn element_name(&self) -> &'static str {
        match self {
            DecoderBackend::Software => "avdec_h264",
            DecoderBackend::Nvidia => "nvh264dec",
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
}

impl RawFrame {
    /// Get the row stride (bytes per row)
    pub fn step(&self) -> u32 {
        self.width * 3 // RGB24 = 3 bytes per pixel
    }
}

/// Decodes H264 NAL units to raw RGB frames using GStreamer
///
/// Uses pipeline: appsrc -> h264parse -> decoder -> videoconvert -> appsink
pub struct VideoH264Decoder {
    pipeline: gstreamer::Pipeline,
    appsrc: gstreamer_app::AppSrc,
    frame_rx: flume::Receiver<RawFrame>,
    sequence: std::sync::atomic::AtomicU32,
}

impl VideoH264Decoder {
    /// Create a new H264 decoder with the specified backend
    ///
    /// # Arguments
    ///
    /// * `backend` - Decoder backend to use (Software or Nvidia)
    pub fn new(backend: DecoderBackend) -> Result<Self, H264DecodeError> {
        // Initialize GStreamer if not already initialized
        if !gstreamer::INITIALIZED.load(std::sync::atomic::Ordering::Relaxed) {
            gstreamer::init()?;
        }

        let decoder_element = backend.element_name();

        // Build pipeline for H264 decoding to RGB
        let pipeline_desc = format!(
            "appsrc name=src caps=video/x-h264,stream-format=byte-stream,alignment=au ! \
             h264parse ! \
             {decoder_element} ! \
             videoconvert ! \
             video/x-raw,format=RGB ! \
             appsink name=sink emit-signals=true sync=false"
        );

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

        // Configure appsrc
        appsrc.set_format(gstreamer::Format::Time);
        appsrc.set_is_live(true);
        appsrc.set_stream_type(gstreamer_app::AppStreamType::Stream);

        // Get appsink for receiving decoded frames
        let appsink = pipeline
            .by_name("sink")
            .ok_or(H264DecodeError::ElementNotFound)?
            .dynamic_cast::<gstreamer_app::AppSink>()
            .map_err(|_| H264DecodeError::DowncastError)?;

        // Channel for decoded frames
        let (frame_tx, frame_rx) = flume::unbounded::<RawFrame>();

        // Set up callback to receive decoded frames
        appsink.set_callbacks(
            gstreamer_app::AppSinkCallbacks::builder()
                .new_sample(move |sink| match Self::handle_decoded_sample(sink) {
                    Ok(frame) => {
                        let _ = frame_tx.send(frame);
                        Ok(gstreamer::FlowSuccess::Ok)
                    }
                    Err(e) => {
                        log::error!("[H264Decoder] Error handling decoded sample: {}", e);
                        Err(gstreamer::FlowError::Error)
                    }
                })
                .build(),
        );

        // Start the pipeline
        pipeline.set_state(gstreamer::State::Playing)?;

        log::info!(
            "H264 decoder initialized with {} backend",
            match backend {
                DecoderBackend::Software => "software (avdec_h264)",
                DecoderBackend::Nvidia => "NVIDIA (nvh264dec)",
            }
        );

        Ok(Self {
            pipeline,
            appsrc,
            frame_rx,
            sequence: std::sync::atomic::AtomicU32::new(0),
        })
    }

    /// Handle a decoded sample from appsink
    fn handle_decoded_sample(
        appsink: &gstreamer_app::AppSink,
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

        Ok(RawFrame {
            data: map.as_slice().to_vec(),
            width,
            height,
            pts,
            sequence: 0, // Will be set by caller
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

    /// Get and increment the sequence number
    pub fn next_sequence(&self) -> u32 {
        self.sequence
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
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
