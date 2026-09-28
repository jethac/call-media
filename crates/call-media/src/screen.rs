//! Screen media types. Device capture is enabled with `video`.
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU32, AtomicU64},
};

pub const MAX_SOURCES: usize = 64;
pub const MAX_SOURCE_NAME_BYTES: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceId {
    /// Explicit Linux capture node, e.g. VideoDevice(3) selects /dev/video3.
    /// HDMI capture cards and webcams are both V4L2 inputs.
    #[cfg(target_os = "linux")]
    VideoDevice(u32),
    /// The Linux desktop chooses the source after an explicit Share action.
    Portal,
    /// Explicit whole-desktop capture on a native X11 session, without a portal.
    X11Desktop,
    Display(u64),
    Window(u64),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Source {
    pub id: SourceId,
    pub name: String,
}

#[derive(Clone, Copy, Debug)]
pub struct Settings {
    pub source: SourceId,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub cursor: bool,
    /// Share system audio with the screen; call microphone settings are independent.
    pub audio: bool,
}
impl Settings {
    pub fn valid(self) -> bool {
        !matches!(self.source, SourceId::Display(0) | SourceId::Window(0))
            && matches!(
                (self.width, self.height),
                (854, 480) | (1280, 720) | (1920, 1080)
            )
            && matches!(self.fps, 15 | 30 | 60)
    }

    pub fn bit_rate(self) -> u32 {
        let base = match (self.width, self.height) {
            (854, 480) => 2_000_000,
            (1920, 1080) => 8_000_000,
            _ => 4_000_000,
        };
        (base * if self.fps == 60 { 2 } else { 1 }).min(16_000_000)
    }
}

pub const MAX_RAW_BYTES: usize = 3840 * 2160 * 4;
pub const MAX_ENCODED_BYTES: usize = 2 * 1024 * 1024;

pub struct RawFrame {
    pub width: u32,
    pub height: u32,
    pub stride: usize,
    pub data: Vec<u8>,
}

pub struct EncodedFrame {
    pub data: Vec<u8>,
    pub timestamp: u32,
    pub keyframe: bool,
}

/// Longest system-audio chunk accepted from the OS: 100 ms of 48 kHz stereo.
pub const MAX_AUDIO_SAMPLES: usize = 4800 * 2;

#[derive(Debug)]
pub struct AudioChunk {
    pub samples: Vec<f32>,
    /// Capture generation; old buffers must not cross an encryption transition.
    pub epoch: u64,
}

pub struct Video {
    pub settings: Settings,
    pub frames: tokio::sync::mpsc::Receiver<EncodedFrame>,
    pub ready: Arc<AtomicBool>,
    pub keyframe: Arc<AtomicBool>,
    /// Transport feedback target in bits per second, bounded by the selected quality.
    pub bitrate: Arc<AtomicU32>,
    /// Interleaved 48 kHz stereo system audio, present only when the share requested it.
    pub audio: Option<tokio::sync::mpsc::Receiver<AudioChunk>>,
    pub audio_epoch: Arc<AtomicU64>,
}

#[cfg(feature = "video")]
#[path = "screen_backend.rs"]
mod backend;
#[cfg(feature = "video")]
pub(crate) use backend::software_rate_change;
#[cfg(feature = "video")]
pub use backend::*;
