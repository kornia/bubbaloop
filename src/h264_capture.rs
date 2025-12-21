use crate::h264_decode::{DecoderBackend, RawFrame};
use gstreamer::prelude::*;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum H264CaptureError {
    #[error("GStreamer error: {0}")]
    GStreamer(#[from] gstreamer::glib::Error),
    #[error("GStreamer state change error: {0}")]
    StateChange(#[from] gstreamer::StateChangeError),
    #[error("Element not found: {0}")]
    ElementNotFound(&'static str),
    #[error("Failed to downcast")]
    DowncastError,
    #[error("Buffer error")]
    BufferError,
    #[error("Caps error: {0}")]
    CapsError(String),
}

/// H264 frame (zero-copy from GStreamer)
pub struct H264Frame {
    buffer: gstreamer::MappedBuffer<gstreamer::buffer::Readable>,
    pub pts: u64,
    pub keyframe: bool,
    pub sequence: u32,
}

impl H264Frame {
    pub fn as_slice(&self) -> &[u8] {
        self.buffer.as_slice()
    }
    pub fn len(&self) -> usize {
        self.buffer.len()
    }
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }
}

/// Captures H264 from RTSP with optional decode via tee
pub struct H264StreamCapture {
    pipeline: gstreamer::Pipeline,
    h264_rx: flume::Receiver<H264Frame>,
    raw_rx: Option<flume::Receiver<RawFrame>>,
}

impl H264StreamCapture {
    /// Compressed only
    pub fn new(url: &str, latency: u32) -> Result<Self, H264CaptureError> {
        Self::with_decoder(url, latency, None)
    }

    /// With optional decoder via GStreamer tee
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
                let dec = backend.pipeline_segment();
                format!(
                    "rtspsrc location={url} latency={latency} ! \
                     rtph264depay ! h264parse config-interval=-1 ! \
                     video/x-h264,stream-format=byte-stream,alignment=au ! \
                     tee name=t \
                     t. ! queue max-size-buffers=1 leaky=downstream ! \
                         appsink name=h264sink emit-signals=true sync=false max-buffers=1 drop=true \
                     t. ! queue max-size-buffers=1 leaky=downstream ! {dec} ! \
                         video/x-raw,format=RGBA ! \
                         appsink name=rawsink emit-signals=true sync=false max-buffers=1 drop=true"
                )
            }
            None => format!(
                "rtspsrc location={url} latency={latency} ! \
                 rtph264depay ! h264parse config-interval=-1 ! \
                 video/x-h264,stream-format=byte-stream,alignment=au ! \
                 appsink name=h264sink emit-signals=true sync=false max-buffers=1 drop=true"
            ),
        };

        log::debug!("Pipeline: {}", pipeline_desc);

        let pipeline = gstreamer::parse::launch(&pipeline_desc)?
            .dynamic_cast::<gstreamer::Pipeline>()
            .map_err(|_| H264CaptureError::DowncastError)?;

        // H264 sink - bounded channel, drop if full
        let (h264_tx, h264_rx) = flume::bounded::<H264Frame>(2);
        let h264_sink = pipeline
            .by_name("h264sink")
            .ok_or(H264CaptureError::ElementNotFound("h264sink"))?
            .dynamic_cast::<gstreamer_app::AppSink>()
            .map_err(|_| H264CaptureError::DowncastError)?;

        h264_sink.set_callbacks(
            gstreamer_app::AppSinkCallbacks::builder()
                .new_sample({
                    let mut seq: u32 = 0;
                    move |sink| {
                        if let Ok(frame) = Self::extract_h264(sink, seq) {
                            seq = seq.wrapping_add(1);
                            let _ = h264_tx.try_send(frame); // Drop if full
                        }
                        Ok(gstreamer::FlowSuccess::Ok)
                    }
                })
                .build(),
        );

        // Raw sink (optional)
        let raw_rx = if decoder.is_some() {
            if let Some(raw_sink_elem) = pipeline.by_name("rawsink") {
                let (raw_tx, raw_rx) = flume::bounded::<RawFrame>(2);
                let raw_sink = raw_sink_elem
                    .dynamic_cast::<gstreamer_app::AppSink>()
                    .map_err(|_| H264CaptureError::DowncastError)?;

                raw_sink.set_callbacks(
                    gstreamer_app::AppSinkCallbacks::builder()
                        .new_sample({
                            let mut seq: u32 = 0;
                            move |sink| {
                                if let Ok(frame) = Self::extract_raw(sink, seq) {
                                    seq = seq.wrapping_add(1);
                                    let _ = raw_tx.try_send(frame); // Drop if full
                                }
                                Ok(gstreamer::FlowSuccess::Ok)
                            }
                        })
                        .build(),
                );
                Some(raw_rx)
            } else {
                log::warn!("rawsink not found - decoder may have failed");
                None
            }
        } else {
            None
        };

        Ok(Self {
            pipeline,
            h264_rx,
            raw_rx,
        })
    }

    fn extract_h264(
        sink: &gstreamer_app::AppSink,
        seq: u32,
    ) -> Result<H264Frame, H264CaptureError> {
        let sample = sink
            .pull_sample()
            .map_err(|_| H264CaptureError::BufferError)?;
        let buf = sample.buffer_owned().ok_or(H264CaptureError::BufferError)?;

        let pts = buf
            .pts()
            .or_else(|| buf.dts())
            .map(|t| t.nseconds())
            .unwrap_or(0);
        let keyframe = !buf.flags().contains(gstreamer::BufferFlags::DELTA_UNIT);
        let buffer = buf
            .into_mapped_buffer_readable()
            .map_err(|_| H264CaptureError::BufferError)?;

        Ok(H264Frame {
            buffer,
            pts,
            keyframe,
            sequence: seq,
        })
    }

    fn extract_raw(sink: &gstreamer_app::AppSink, seq: u32) -> Result<RawFrame, H264CaptureError> {
        let sample = sink
            .pull_sample()
            .map_err(|_| H264CaptureError::BufferError)?;
        let caps = sample.caps().ok_or(H264CaptureError::BufferError)?;
        let info = gstreamer_video::VideoInfo::from_caps(caps)
            .map_err(|e| H264CaptureError::CapsError(e.to_string()))?;

        let buf = sample.buffer().ok_or(H264CaptureError::BufferError)?;
        let pts = buf.pts().map(|t| t.nseconds()).unwrap_or(0);
        let map = buf
            .map_readable()
            .map_err(|_| H264CaptureError::BufferError)?;

        Ok(RawFrame {
            data: map.as_slice().to_vec(),
            width: info.width(),
            height: info.height(),
            pts,
            sequence: seq,
            format: "RGBA".to_string(),
            step: info.width() * 4,
        })
    }

    pub fn start(&self) -> Result<(), H264CaptureError> {
        self.pipeline.set_state(gstreamer::State::Playing)?;
        Ok(())
    }

    pub fn h264_receiver(&self) -> &flume::Receiver<H264Frame> {
        &self.h264_rx
    }

    pub fn raw_receiver(&self) -> Option<&flume::Receiver<RawFrame>> {
        self.raw_rx.as_ref()
    }

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
