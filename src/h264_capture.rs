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

/// Captures H264 from RTSP
pub struct H264StreamCapture {
    pipeline: gstreamer::Pipeline,
    rx: flume::Receiver<H264Frame>,
}

impl H264StreamCapture {
    pub fn new(url: &str, latency: u32) -> Result<Self, H264CaptureError> {
        if !gstreamer::INITIALIZED.load(std::sync::atomic::Ordering::Relaxed) {
            gstreamer::init()?;
        }

        let pipeline_desc = format!(
            "rtspsrc location={url} latency={latency} ! \
             rtph264depay ! h264parse config-interval=-1 ! \
             video/x-h264,stream-format=byte-stream,alignment=au ! \
             appsink name=sink emit-signals=true sync=false max-buffers=2 drop=true"
        );

        let pipeline = gstreamer::parse::launch(&pipeline_desc)?
            .dynamic_cast::<gstreamer::Pipeline>()
            .map_err(|_| H264CaptureError::DowncastError)?;

        let (tx, rx) = flume::unbounded::<H264Frame>();

        let appsink = pipeline
            .by_name("sink")
            .ok_or(H264CaptureError::ElementNotFound("sink"))?
            .dynamic_cast::<gstreamer_app::AppSink>()
            .map_err(|_| H264CaptureError::DowncastError)?;

        appsink.set_callbacks(
            gstreamer_app::AppSinkCallbacks::builder()
                .new_sample({
                    let mut sequence: u32 = 0;
                    move |sink| {
                        if let Ok(frame) = Self::handle_sample(sink, sequence) {
                            sequence = sequence.wrapping_add(1);
                            let _ = tx.try_send(frame);
                        }
                        Ok(gstreamer::FlowSuccess::Ok)
                    }
                })
                .build(),
        );

        Ok(Self { pipeline, rx })
    }

    fn handle_sample(
        sink: &gstreamer_app::AppSink,
        sequence: u32,
    ) -> Result<H264Frame, H264CaptureError> {
        let sample = sink
            .pull_sample()
            .map_err(|_| H264CaptureError::BufferError)?;
        let buffer = sample.buffer_owned().ok_or(H264CaptureError::BufferError)?;

        let pts = buffer
            .pts()
            .or_else(|| buffer.dts())
            .map(|t| t.nseconds())
            .unwrap_or(0);
        let keyframe = !buffer.flags().contains(gstreamer::BufferFlags::DELTA_UNIT);

        let mapped = buffer
            .into_mapped_buffer_readable()
            .map_err(|_| H264CaptureError::BufferError)?;

        Ok(H264Frame {
            buffer: mapped,
            pts,
            keyframe,
            sequence,
        })
    }

    pub fn start(&self) -> Result<(), H264CaptureError> {
        self.pipeline.set_state(gstreamer::State::Playing)?;
        Ok(())
    }

    pub fn receiver(&self) -> &flume::Receiver<H264Frame> {
        &self.rx
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
