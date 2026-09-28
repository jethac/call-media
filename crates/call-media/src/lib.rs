//! Audio/video types and optional device adapters, independent of UI and service protocols.
//! No device is opened until an explicit `start` or `preview` call.
//! Enable `audio` for CPAL capture/playback and `video` for camera/screen capture.

/// One 20 ms frame of mono 48 kHz floating-point PCM.
pub type Frame = [f32; 960];
#[cfg(feature = "audio")]
mod activity;
#[cfg(feature = "audio")]
pub mod audio;
#[cfg(feature = "video")]
pub mod camera;
#[cfg(all(feature = "video", target_os = "linux"))]
pub mod decode;
#[cfg(any(feature = "audio", feature = "video"))]
#[allow(dead_code)] // Shared counters; only the enabled adapters use each category.
mod diagnostics;
pub mod h264;
pub mod processing;
pub mod screen;
#[cfg(feature = "video")]
mod video_encode;

#[cfg(feature = "codecs")]
pub mod codec;

#[cfg(all(feature = "video", target_os = "linux"))]
pub mod share_audio;
