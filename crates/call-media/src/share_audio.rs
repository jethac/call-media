//! Explicit Linux audio input for HDMI/video sharing.
use crate::screen::{AudioChunk, MAX_AUDIO_SAMPLES, Video};
use gst::prelude::*;
use gstreamer as gst;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc,
};

/// Native device name, never a GStreamer pipeline description.
/// Select the capture card's audio input explicitly; no device pairing is inferred.
#[derive(Clone, Debug)]
pub enum Source {
    /// PulseAudio/PipeWire-Pulse source name, including an explicitly selected monitor.
    Pulse(String),
    /// ALSA capture PCM name, such as `hw:2,0`.
    Alsa(String),
}

/// Owns a separate capture worker. Drop requests cancellation; `shutdown` also
/// returns completion so applications can await physical device release off the UI thread.
pub struct Capture {
    stop: Arc<AtomicBool>,
    done: Option<mpsc::Receiver<Result<(), &'static str>>>,
}
impl Capture {
    /// Attach before handing `video` to the service transport. Start video capture
    /// with `Settings.audio = false` to avoid opening desktop loopback as well.
    /// Native startup is asynchronous; poll `result` to report device errors.
    pub fn attach(video: &mut Video, source: Source) -> Result<Self, &'static str> {
        if video.audio.is_some() {
            return Err("Share already has an audio input");
        }
        let (Source::Pulse(name) | Source::Alsa(name)) = &source;
        if name.is_empty() || name.len() > 4096 || name.contains('\0') {
            return Err("Invalid share audio device");
        }
        let (send, receive) = tokio::sync::mpsc::channel(4);
        let (done_send, done) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        let ready = video.ready.clone();
        let epoch = video.audio_epoch.clone();
        std::thread::Builder::new()
            .name("share-audio-input".into())
            .spawn(move || {
                let result = run(source, send, worker_stop, ready, epoch);
                let _ = done_send.send(result);
            })
            .map_err(|_| "Could not start share audio worker")?;
        video.audio = Some(receive);
        Ok(Self {
            stop,
            done: Some(done),
        })
    }
    /// Returns a terminal result once. `None` means still running or already read.
    pub fn result(&mut self) -> Option<Result<(), &'static str>> {
        match self.done.as_ref()?.try_recv() {
            Ok(result) => {
                self.done = None;
                Some(result)
            }
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.done = None;
                Some(Err("Share audio worker stopped unexpectedly"))
            }
        }
    }
    pub fn shutdown(mut self) -> Option<mpsc::Receiver<Result<(), &'static str>>> {
        self.stop.store(true, Ordering::Release);
        self.done.take()
    }
}
impl Drop for Capture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
    }
}
struct Pipeline(gst::Pipeline);
impl Drop for Pipeline {
    fn drop(&mut self) {
        let _ = self.0.set_state(gst::State::Null);
    }
}
fn run(
    source: Source,
    send: tokio::sync::mpsc::Sender<AudioChunk>,
    stop: Arc<AtomicBool>,
    ready: Arc<AtomicBool>,
    epoch: Arc<AtomicU64>,
) -> Result<(), &'static str> {
    if stop.load(Ordering::Acquire) || send.is_closed() {
        return Ok(());
    }
    gst::init().map_err(|_| "GStreamer unavailable")?;
    let (factory, device) = match source {
        Source::Pulse(name) => ("pulsesrc", name),
        Source::Alsa(name) => ("alsasrc", name),
    };
    let input = gst::ElementFactory::make(factory)
        .property("device", device)
        .property("buffer-time", 40_000i64)
        .property("latency-time", 10_000i64)
        .build()
        .map_err(|_| "Share audio source plugin unavailable")?;
    run_input(input, send, stop, ready, epoch)
}

fn run_input(
    input: gst::Element,
    send: tokio::sync::mpsc::Sender<AudioChunk>,
    stop: Arc<AtomicBool>,
    ready: Arc<AtomicBool>,
    epoch: Arc<AtomicU64>,
) -> Result<(), &'static str> {
    let convert = gst::ElementFactory::make("audioconvert")
        .build()
        .map_err(|_| "Audio conversion unavailable")?;
    let resample = gst::ElementFactory::make("audioresample")
        .build()
        .map_err(|_| "Audio resampling unavailable")?;
    let caps = gst::Caps::builder("audio/x-raw")
        .field("format", "F32LE")
        .field("layout", "interleaved")
        .field("rate", 48_000i32)
        .field("channels", 2i32)
        .build();
    let sink = gstreamer_app::AppSink::builder()
        .caps(&caps)
        .max_buffers(2)
        .drop(true)
        .sync(false)
        .build();
    let pipeline = Pipeline(gst::Pipeline::new());
    pipeline
        .0
        .add_many([&input, &convert, &resample, sink.upcast_ref()])
        .map_err(|_| "Could not create audio pipeline")?;
    gst::Element::link_many([&input, &convert, &resample, sink.upcast_ref()])
        .map_err(|_| "Could not link audio pipeline")?;
    let bus = pipeline.0.bus().ok_or("Audio pipeline has no bus")?;
    pipeline
        .0
        .set_state(gst::State::Playing)
        .map_err(|_| "Could not open selected audio input")?;
    let mut generation = None;
    let mut cutoff = None;
    let mut last_sample = std::time::Instant::now();
    while !stop.load(Ordering::Acquire) && !send.is_closed() {
        if let Some(message) = bus.pop_filtered(&[gst::MessageType::Error, gst::MessageType::Eos]) {
            return Err(match message.type_() {
                gst::MessageType::Eos => "Selected audio input ended",
                _ => "Selected audio input failed",
            });
        }
        let current = epoch.load(Ordering::Acquire);
        if !ready.load(Ordering::Acquire) {
            generation = None;
            cutoff = None;
        } else if generation != Some(current) || cutoff.is_none() {
            // Reject native buffers captured before readiness/encryption changes,
            // including buffers still queued upstream of appsink.
            cutoff = pipeline.0.current_running_time();
            generation = Some(current);
        }
        let Some(sample) = sink.try_pull_sample(gst::ClockTime::from_mseconds(20)) else {
            if last_sample.elapsed() > std::time::Duration::from_secs(5) {
                return Err("Selected audio input stopped producing samples");
            }
            continue;
        };
        last_sample = std::time::Instant::now();
        let Some(cutoff) = cutoff else {
            continue;
        };
        if !ready.load(Ordering::Acquire) || epoch.load(Ordering::Acquire) != current {
            continue;
        }
        let buffer = sample.buffer().ok_or("Audio sample has no buffer")?;
        let time = sample
            .segment()
            .and_then(|s| s.downcast_ref::<gst::ClockTime>())
            .and_then(|s| s.to_running_time(buffer.pts()));
        if time.is_none_or(|time| time < cutoff) {
            continue;
        }
        if buffer.size() > MAX_AUDIO_SAMPLES * 4 || !buffer.size().is_multiple_of(8) {
            return Err("Audio input returned an oversized or unaligned buffer");
        }
        let map = buffer
            .map_readable()
            .map_err(|_| "Could not read audio input")?;
        let samples = map
            .as_slice()
            .as_chunks::<4>()
            .0
            .iter()
            .map(|bytes| {
                let value = f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
                if value.is_finite() {
                    value.clamp(-1.0, 1.0)
                } else {
                    0.0
                }
            })
            .collect();
        let _ = send.try_send(AudioChunk {
            samples,
            epoch: current,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn native_pipeline_gates_audio_tracks_epochs_and_releases_source() {
        gst::init().unwrap();
        // Required offline plugin, never a physical input or server connection.
        let input = gst::ElementFactory::make("audiotestsrc")
            .property("is-live", true)
            .property("samplesperbuffer", 480i32)
            .build()
            .expect("install gstreamer1.0-plugins-base for the offline audio check");
        let inspect = input.clone();
        let (send, mut receive) = tokio::sync::mpsc::channel(4);
        let stop = Arc::new(AtomicBool::new(false));
        let ready = Arc::new(AtomicBool::new(false));
        let epoch = Arc::new(AtomicU64::new(1));
        let (done, result) = mpsc::channel();
        let (worker_stop, worker_ready, worker_epoch) =
            (stop.clone(), ready.clone(), epoch.clone());
        let worker = std::thread::spawn(move || {
            let _ = done.send(run_input(
                input,
                send,
                worker_stop,
                worker_ready,
                worker_epoch,
            ));
        });
        std::thread::sleep(Duration::from_millis(100));
        assert!(receive.try_recv().is_err());
        ready.store(true, Ordering::Release);
        let deadline = Instant::now() + Duration::from_secs(3);
        let first = loop {
            if let Ok(frame) = receive.try_recv() {
                break frame;
            }
            assert!(Instant::now() < deadline, "no ready audio received");
            std::thread::sleep(Duration::from_millis(10));
        };
        assert_eq!(first.epoch, 1);
        assert!(!first.samples.is_empty() && first.samples.len() <= MAX_AUDIO_SAMPLES);
        assert!(first.samples.len().is_multiple_of(2));
        assert!(
            first
                .samples
                .iter()
                .all(|s| s.is_finite() && s.abs() <= 1.0)
        );
        assert!(first.samples.iter().any(|s| s.abs() > 0.01));
        epoch.store(2, Ordering::Release);
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if let Ok(frame) = receive.try_recv()
                && frame.epoch == 2
            {
                break;
            }
            assert!(Instant::now() < deadline, "new epoch never reached capture");
            std::thread::sleep(Duration::from_millis(10));
        }
        // Let backpressure fill the output without blocking native capture.
        std::thread::sleep(Duration::from_millis(100));
        assert!(receive.len() <= 4);
        stop.store(true, Ordering::Release);
        assert_eq!(result.recv_timeout(Duration::from_secs(3)).unwrap(), Ok(()));
        worker.join().unwrap();
        assert_eq!(inspect.current_state(), gst::State::Null);
    }

    #[test]
    fn native_source_eos_is_an_observable_error() {
        gst::init().unwrap();
        let input = gst::ElementFactory::make("audiotestsrc")
            .property("is-live", true)
            .property("num-buffers", 1i32)
            .build()
            .expect("install gstreamer1.0-plugins-base");
        let (send, _receive) = tokio::sync::mpsc::channel(4);
        let (done, result) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        let worker = std::thread::spawn(move || {
            let _ = done.send(run_input(
                input,
                send,
                worker_stop,
                Arc::new(AtomicBool::new(false)),
                Arc::new(AtomicU64::new(1)),
            ));
        });
        let result = result.recv_timeout(Duration::from_secs(3));
        stop.store(true, Ordering::Release);
        worker.join().unwrap();
        assert_eq!(result.unwrap(), Err("Selected audio input ended"));
    }
    #[test]
    fn delayed_native_buffers_cannot_cross_an_epoch_cutoff() {
        gst::init().unwrap();
        let caps = gst::Caps::builder("audio/x-raw")
            .field("format", "F32LE")
            .field("layout", "interleaved")
            .field("rate", 48_000i32)
            .field("channels", 2i32)
            .build();
        let input = gstreamer_app::AppSrc::builder()
            .caps(&caps)
            .is_live(true)
            .format(gst::Format::Time)
            .build();
        let source = input.clone();
        let (send, mut receive) = tokio::sync::mpsc::channel(4);
        let ready = Arc::new(AtomicBool::new(true));
        let epoch = Arc::new(AtomicU64::new(1));
        let stop = Arc::new(AtomicBool::new(false));
        let (worker_stop, worker_epoch) = (stop.clone(), epoch.clone());
        let (done, result) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let _ = done.send(run_input(
                source.upcast(),
                send,
                worker_stop,
                ready,
                worker_epoch,
            ));
        });
        let deadline = Instant::now() + Duration::from_secs(3);
        while input.current_running_time().is_none() {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        std::thread::sleep(Duration::from_millis(50));
        let push = |timestamp: gst::ClockTime, value: f32| {
            let bytes = value.to_le_bytes().repeat(960);
            let mut buffer = gst::Buffer::from_mut_slice(bytes);
            let metadata = buffer.get_mut().unwrap();
            metadata.set_pts(timestamp);
            metadata.set_duration(gst::ClockTime::from_mseconds(10));
            input.push_buffer(buffer).unwrap();
        };
        let old = input.current_running_time().unwrap();
        epoch.store(2, Ordering::Release);
        std::thread::sleep(Duration::from_millis(50));
        push(old, 0.25);
        std::thread::sleep(Duration::from_millis(50));
        assert!(
            receive.try_recv().is_err(),
            "old native buffer was relabeled with the new epoch"
        );
        push(input.current_running_time().unwrap(), 0.75);
        let deadline = Instant::now() + Duration::from_secs(3);
        let frame = loop {
            if let Ok(frame) = receive.try_recv() {
                break frame;
            }
            assert!(Instant::now() < deadline, "fresh audio did not pass cutoff");
            std::thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(frame.epoch, 2);
        assert!(
            frame
                .samples
                .iter()
                .all(|sample| (*sample - 0.75).abs() < 0.001)
        );
        stop.store(true, Ordering::Release);
        assert_eq!(result.recv_timeout(Duration::from_secs(3)).unwrap(), Ok(()));
        worker.join().unwrap();
    }
}
