use crate::h264_decode::{DecoderBackend, RawFrame};
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
}

/// A single H264 frame captured from the RTSP stream (zero-copy)
pub struct H264Frame {
    buffer: gstreamer::MappedBuffer<gstreamer::buffer::Readable>,
    pub pts: u64,
    pub keyframe: bool,
    pub sequence: u32,
}

impl H264Frame {
    #[inline]
    pub fn as_slice(&self) -> &[u8] {
        self.buffer.as_slice()
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }
}

/// Captures H264 frames from RTSP, optionally with decoded raw frames via tee
pub struct H264StreamCapture {
    pipeline: gstreamer::Pipeline,
    h264_rx: flume::Receiver<H264Frame>,
    raw_rx: Option<flume::Receiver<RawFrame>>,
}

impl H264StreamCapture {
    /// Create capture without decoding (compressed only)
    pub fn new(url: &str, latency: u32) -> Result<Self, H264CaptureError> {
        Self::with_decoder(url, latency, None)
    }

    /// Create capture with optional decoder using GStreamer tee
    ///
    /// Pipeline with decoder:
    /// ```text
    /// rtspsrc ! rtph264depay ! h264parse ! tee name=t
    ///   t. ! queue ! appsink (H264)
    ///   t. ! queue ! decoder ! videoconvert ! appsink (raw)
    /// ```
    pub fn with_decoder(
        url: &str,
        latency: u32,
        decoder: Option<DecoderBackend>,
    ) -> Result<Self, H264CaptureError> {
        if !gstreamer::INITIALIZED.load(std::sync::atomic::Ordering::Relaxed) {
            gstreamer::init()?;
        }

        let pipeline_desc = match decoder {
            Some(backend) => {
                let decoder_segment = backend.pipeline_segment();
                format!(
                    "rtspsrc location={url} latency={latency} ! \
                     rtph264depay ! \
                     h264parse config-interval=-1 ! \
                     video/x-h264,stream-format=byte-stream,alignment=au ! \
                     tee name=t \
                     t. ! queue max-size-buffers=2 leaky=downstream ! appsink name=h264sink emit-signals=true sync=false \
                     t. ! queue max-size-buffers=2 leaky=downstream ! {decoder_segment} ! video/x-raw,format=RGBA ! appsink name=rawsink emit-signals=true sync=false"
                )
            }
            None => format!(
                "rtspsrc location={url} latency={latency} ! \
                 rtph264depay ! \
                 h264parse config-interval=-1 ! \
                 video/x-h264,stream-format=byte-stream,alignment=au ! \
                 appsink name=h264sink emit-signals=true sync=false"
            ),
        };

        log::debug!("Creating pipeline: {}", pipeline_desc);

        let pipeline = gstreamer::parse::launch(&pipeline_desc)?
            .dynamic_cast::<gstreamer::Pipeline>()
            .map_err(|_| H264CaptureError::DowncastError)?;

        // H264 sink (always present)
        let h264_sink = pipeline
            .by_name("h264sink")
            .ok_or(H264CaptureError::ElementNotFound)?
            .dynamic_cast::<gstreamer_app::AppSink>()
            .map_err(|_| H264CaptureError::DowncastError)?;

        let (h264_tx, h264_rx) = flume::unbounded::<H264Frame>();

        h264_sink.set_callbacks(
            gstreamer_app::AppSinkCallbacks::builder()
                .new_sample({
                    let mut seq: u32 = 0;
                    move |sink| {
                        if let Ok(frame) = Self::handle_h264_sample(sink, seq) {
                            seq = seq.wrapping_add(1);
                            let _ = h264_tx.send(frame);
                        }
                        Ok(gstreamer::FlowSuccess::Ok)
                    }
                })
                .build(),
        );

        // Raw sink (optional)
        let raw_rx = if decoder.is_some() {
            let raw_sink = pipeline
                .by_name("rawsink")
                .ok_or(H264CaptureError::ElementNotFound)?
                .dynamic_cast::<gstreamer_app::AppSink>()
                .map_err(|_| H264CaptureError::DowncastError)?;

            let (raw_tx, raw_rx) = flume::unbounded::<RawFrame>();

            raw_sink.set_callbacks(
                gstreamer_app::AppSinkCallbacks::builder()
                    .new_sample({
                        let mut seq: u32 = 0;
                        move |sink| {
                            if let Ok(frame) = Self::handle_raw_sample(sink, seq) {
                                seq = seq.wrapping_add(1);
                                let _ = raw_tx.send(frame);
                            }
                            Ok(gstreamer::FlowSuccess::Ok)
                        }
                    })
                    .build(),
            );

            Some(raw_rx)
        } else {
            None
        };

        Ok(Self {
            pipeline,
            h264_rx,
            raw_rx,
        })
    }

    fn handle_h264_sample(
        sink: &gstreamer_app::AppSink,
        sequence: u32,
    ) -> Result<H264Frame, H264CaptureError> {
        let sample = sink
            .pull_sample()
            .map_err(|e| H264CaptureError::FlowError(e.to_string()))?;

        let gst_buffer = sample.buffer_owned().ok_or(H264CaptureError::BufferError)?;

        let pts = gst_buffer
            .pts()
            .or_else(|| gst_buffer.dts())
            .map(|t| t.nseconds())
            .unwrap_or(0);

        let keyframe = !gst_buffer
            .flags()
            .contains(gstreamer::BufferFlags::DELTA_UNIT);

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

    fn handle_raw_sample(
        sink: &gstreamer_app::AppSink,
        sequence: u32,
    ) -> Result<RawFrame, H264CaptureError> {
        let sample = sink
            .pull_sample()
            .map_err(|e| H264CaptureError::FlowError(e.to_string()))?;

        let caps = sample.caps().ok_or(H264CaptureError::BufferError)?;
        let info = gstreamer_video::VideoInfo::from_caps(caps)
            .map_err(|e| H264CaptureError::CapsError(e.to_string()))?;

        let buffer = sample.buffer().ok_or(H264CaptureError::BufferError)?;
        let pts = buffer.pts().map(|t| t.nseconds()).unwrap_or(0);

        let map = buffer
            .map_readable()
            .map_err(|_| H264CaptureError::BufferError)?;

        Ok(RawFrame {
            data: map.as_slice().to_vec(),
            width: info.width(),
            height: info.height(),
            pts,
            sequence,
            format: "RGBA".to_string(),
            step: info.width() * 4,
        })
    }

    pub fn start(&self) -> Result<(), H264CaptureError> {
        self.pipeline.set_state(gstreamer::State::Playing)?;
        Ok(())
    }

    /// Receiver for H264 compressed frames
    pub fn h264_receiver(&self) -> &flume::Receiver<H264Frame> {
        &self.h264_rx
    }

    /// Receiver for decoded raw frames (None if decoder not enabled)
    pub fn raw_receiver(&self) -> Option<&flume::Receiver<RawFrame>> {
        self.raw_rx.as_ref()
    }

    /// Legacy API - same as h264_receiver
    pub fn receiver(&self) -> &flume::Receiver<H264Frame> {
        &self.h264_rx
    }

    pub fn close(&self) -> Result<(), H264CaptureError> {
        let _ = self.pipeline.send_event(gstreamer::event::Eos::new());
        self.pipeline.set_state(gstreamer::State::Null)?;
        Ok(())
    }
}

impl Drop for H264StreamCapture {
    fn drop(&mut self) {
        let _ = self.close();
    }
}
