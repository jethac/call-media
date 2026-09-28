//! Optional native H.264 decoder. Inputs are complete decrypted Annex-B access units.
const INVALID: &str = "The video could not be decoded safely.";
const UNSUPPORTED: &str = "Native H.264 decoding is unavailable.";
const MAX_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_ACCESS_UNIT: usize = 2 * 1024 * 1024 + 64 * 1024;
pub struct LiveFrame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}
pub type LiveSink = Box<dyn Fn(LiveFrame) + Send + Sync>;
#[cfg(target_os = "linux")]
#[path = "decode/live_gst.rs"]
pub mod live;

fn check_dimensions(width: u32, height: u32) -> Result<(), &'static str> {
    if width == 0
        || height == 0
        || width > 1920
        || height > 1920
        || u64::from(width) * u64::from(height) > 1920 * 1080
    {
        return Err(INVALID);
    }
    Ok(())
}
