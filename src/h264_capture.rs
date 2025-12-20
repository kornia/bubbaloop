use gstreamer::prelude::*;
use thiserror::Error;

/// Errors that can occur during H264 stream capture
#[derive(Debug, Error)]
pub enum H264CaptureError {
    #[error("GStreamer error: {0}")]
    GStreamer(#[from] gstreamer::glib::Error),
    #[error("GStreamer state change error: {0}")]
    StateChange(#[from] gstreamer::StateChangeError),
    #[error("Failed to get element by name")]
    ElementNotFound,
    #[error("Failed to downcast pipeline")]
    DowncastError,
    #[error("Failed to get buffer from sample")]
    BufferError,
    #[error("Failed to get caps from sample: {0}")]
    CapsError(String),
    #[error("Failed to send EOS event")]
    EosError,
    #[error("Flow error: {0}")]
    FlowError(String),
    #[error("Channel disconnected")]
    ChannelDisconnected,
}

/// A single H264 frame captured from the RTSP stream (zero-copy)
pub struct H264Frame {
    /// Raw H264 NAL units (zero-copy, backed by GStreamer buffer)
    buffer: gstreamer::MappedBuffer<gstreamer::buffer::Readable>,
    /// Presentation timestamp in nanoseconds
    pub pts: u64,
    /// Whether this is a keyframe (IDR frame)
    pub keyframe: bool,
    /// Frame sequence number
    pub sequence: u32,
}

impl H264Frame {
    /// Get the frame data as a byte slice (zero-copy)
    #[inline]
    pub fn as_slice(&self) -> &[u8] {
        self.buffer.as_slice()
    }

    /// Get the length of the frame data
    #[inline]
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    /// Check if the frame is empty
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }
}

/// Captures H264 frames directly from an RTSP stream without decoding
///
/// Uses GStreamer pipeline:
/// `rtspsrc location={url} latency={latency} ! rtph264depay ! h264parse ! appsink name=sink`
pub struct H264StreamCapture {
    pipeline: gstreamer::Pipeline,
    frame_rx: flume::Receiver<H264Frame>,
}

impl H264StreamCapture {
    /// Create a new H264 stream capture from an RTSP URL
    ///
    /// # Arguments
    ///
    /// * `url` - RTSP URL (e.g., rtsp://user:pass@192.168.1.10:554/stream)
    /// * `latency` - Latency in milliseconds for the RTSP stream
    pub fn new(url: &str, latency: u32) -> Result<Self, H264CaptureError> {
        // Initialize GStreamer if not already initialized
        if !gstreamer::INITIALIZED.load(std::sync::atomic::Ordering::Relaxed) {
            gstreamer::init()?;
        }

        // Build the pipeline for direct H264 passthrough
        // - h264parse config-interval=-1: insert SPS/PPS before every keyframe
        // - video/x-h264,stream-format=byte-stream: Annex B format required by Foxglove
        let pipeline_desc = format!(
            "rtspsrc location={url} latency={latency} ! \
             rtph264depay ! \
             h264parse config-interval=-1 ! \
             video/x-h264,stream-format=byte-stream,alignment=au ! \
             appsink name=sink emit-signals=true sync=false"
        );

        let pipeline = gstreamer::parse::launch(&pipeline_desc)?
            .dynamic_cast::<gstreamer::Pipeline>()
            .map_err(|_| H264CaptureError::DowncastError)?;

        let appsink = pipeline
            .by_name("sink")
            .ok_or(H264CaptureError::ElementNotFound)?
            .dynamic_cast::<gstreamer_app::AppSink>()
            .map_err(|_| H264CaptureError::DowncastError)?;

        // Use bounded channel to prevent unbounded memory growth
        let (frame_tx, frame_rx) = flume::bounded::<H264Frame>(8);

        // Set up callbacks to capture H264 frames
        appsink.set_callbacks(
            gstreamer_app::AppSinkCallbacks::builder()
                .new_sample({
                    let mut sequence: u32 = 0;
                    move |sink| {
                        match Self::handle_sample(sink, sequence) {
                            Ok(frame) => {
                                sequence = sequence.wrapping_add(1);
                                // Use try_send to avoid blocking GStreamer thread
                                // If channel is full, drop the frame (backpressure)
                                let _ = frame_tx.try_send(frame);
                                Ok(gstreamer::FlowSuccess::Ok)
                            }
                            Err(_) => Err(gstreamer::FlowError::Error),
                        }
                    }
                })
                .build(),
        );

        Ok(Self {
            pipeline,
            frame_rx,
        })
    }

    /// Handle incoming sample from GStreamer (zero-copy)
    fn handle_sample(
        appsink: &gstreamer_app::AppSink,
        sequence: u32,
    ) -> Result<H264Frame, H264CaptureError> {
        let sample = appsink
            .pull_sample()
            .map_err(|e| H264CaptureError::FlowError(e.to_string()))?;

        let gst_buffer = sample
            .buffer_owned()
            .ok_or(H264CaptureError::BufferError)?;

        // Get presentation timestamp
        let pts = gst_buffer
            .pts()
            .map(|p| p.nseconds())
            .unwrap_or(0);

        // Check if this is a keyframe by looking at buffer flags
        let keyframe = !gst_buffer.flags().contains(gstreamer::BufferFlags::DELTA_UNIT);

        // Map buffer for reading (zero-copy)
        let buffer = gst_buffer
            .into_mapped_buffer_readable()
            .map_err(|_| H264CaptureError::BufferError)?;

        Ok(H264Frame {
            buffer,
            pts,
            keyframe,
            sequence,
        })
    }

    /// Start the capture pipeline
    pub fn start(&self) -> Result<(), H264CaptureError> {
        self.pipeline.set_state(gstreamer::State::Playing)?;
        Ok(())
    }

    /// Get the frame receiver for use in async contexts (e.g., tokio::select!)
    ///
    /// # Example
    ///
    /// ```ignore
    /// let capture = H264StreamCapture::new(url, latency)?;
    /// capture.start()?;
    ///
    /// loop {
    ///     tokio::select! {
    ///         Ok(frame) = capture.receiver().recv_async() => {
    ///             // handle frame
    ///         }
    ///         _ = shutdown.changed() => break,
    ///     }
    /// }
    /// ```
    pub fn receiver(&self) -> &flume::Receiver<H264Frame> {
        &self.frame_rx
    }

    /// Close the capture pipeline
    pub fn close(&self) -> Result<(), H264CaptureError> {
        let res = self.pipeline.send_event(gstreamer::event::Eos::new());
        if !res {
            return Err(H264CaptureError::EosError);
        }
        self.pipeline.set_state(gstreamer::State::Null)?;
        Ok(())
    }

    /// Get the current state of the pipeline
    pub fn state(&self) -> gstreamer::State {
        self.pipeline.current_state()
    }
}

impl Drop for H264StreamCapture {
    fn drop(&mut self) {
        if let Err(e) = self.close() {
            log::error!("Error closing H264 capture: {}", e);
        }
    }
}
