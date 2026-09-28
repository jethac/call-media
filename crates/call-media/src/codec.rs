//! Software H.264 codecs for caller-supplied frames. No device or window is opened.
use openh264::{
    OpenH264API,
    encoder::{BitRate, Encoder as NativeEncoder, EncoderConfig, FrameRate, FrameType},
    formats::{RgbSliceU8, YUVBuffer, YUVSource},
};

const MAX_PIXELS: usize = 1920 * 1080;
const MAX_ACCESS_UNIT: usize = 2 * 1024 * 1024;

#[derive(Clone, Copy, Debug)]
pub struct Config {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub bitrate: u32,
}
impl Config {
    pub fn validate(self) -> Result<(), &'static str> {
        let pixels = u64::from(self.width) * u64::from(self.height);
        if self.width == 0
            || self.height == 0
            || !self.width.is_multiple_of(2)
            || !self.height.is_multiple_of(2)
            || self.width > 1920
            || self.height > 1920
            || pixels > MAX_PIXELS as u64
            || !(1..=60).contains(&self.fps)
            || !(64_000..=16_000_000).contains(&self.bitrate)
        {
            return Err("Invalid H.264 encoder configuration");
        }
        Ok(())
    }
}
/// Packed RGBA from one decoded access unit.
#[derive(Debug)]
pub struct Picture {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}
#[derive(Debug)]
pub struct AccessUnit {
    pub data: Vec<u8>,
    pub timestamp: u32,
    pub keyframe: bool,
}

pub struct Encoder {
    config: Config,
    encoder: NativeEncoder,
    yuv: YUVBuffer,
}
impl Encoder {
    pub fn new(config: Config) -> Result<Self, &'static str> {
        config.validate()?;
        let settings = EncoderConfig::new()
            .bitrate(BitRate::from_bps(config.bitrate))
            .max_frame_rate(FrameRate::from_hz(config.fps as f32));
        let encoder = NativeEncoder::with_api_config(OpenH264API::from_source(), settings)
            .map_err(|_| "Could not create H.264 encoder")?;
        Ok(Self {
            config,
            encoder,
            yuv: YUVBuffer::new(config.width as usize, config.height as usize),
        })
    }
    /// Encode tightly packed RGB24. Timestamp is supplied by the caller on a 90 kHz clock.
    /// Request an IDR after dropped pictures or when a new viewer needs a refresh.
    pub fn encode_rgb(
        &mut self,
        rgb: &[u8],
        timestamp: u32,
        keyframe: bool,
    ) -> Result<AccessUnit, &'static str> {
        if rgb.len() != self.config.width as usize * self.config.height as usize * 3 {
            return Err("RGB frame has the wrong length");
        }
        self.yuv.read_rgb(RgbSliceU8::new(
            rgb,
            (self.config.width as usize, self.config.height as usize),
        ));
        if keyframe {
            self.encoder.force_intra_frame();
        }
        let encoded = self
            .encoder
            .encode(&self.yuv)
            .map_err(|_| "H.264 encoding failed")?;
        let keyframe = encoded.frame_type() == FrameType::IDR;
        let data = encoded.to_vec();
        if data.len() > MAX_ACCESS_UNIT {
            return Err("Encoded frame exceeds media limit");
        }
        crate::h264::validate_source(&data)?;
        Ok(AccessUnit {
            data,
            timestamp,
            keyframe,
        })
    }
}

pub struct Decoder {
    decoder: openh264::decoder::Decoder,
}
impl Decoder {
    pub fn new() -> Result<Self, &'static str> {
        Ok(Self {
            decoder: openh264::decoder::Decoder::new()
                .map_err(|_| "Could not create H.264 decoder")?,
        })
    }
    pub fn decode(&mut self, data: &[u8]) -> Result<Option<Picture>, &'static str> {
        crate::h264::validate_source(data)?;
        let Some(yuv) = self
            .decoder
            .decode(data)
            .map_err(|_| "H.264 decoding failed")?
        else {
            return Ok(None);
        };
        let (width, height) = yuv.dimensions();
        if width == 0
            || height == 0
            || width > 1920
            || height > 1920
            || width.checked_mul(height).is_none_or(|n| n > MAX_PIXELS)
        {
            return Err("Decoded picture exceeds media limit");
        }
        let mut rgba = vec![0; width * height * 4];
        yuv.write_rgba8(&mut rgba);
        Ok(Some(Picture {
            width: width as u32,
            height: height as u32,
            rgba,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn synthetic_camera_roundtrip_and_keyframe_recovery() {
        let config = Config {
            width: 64,
            height: 48,
            fps: 15,
            bitrate: 600_000,
        };
        let mut encoder = Encoder::new(config).unwrap();
        let mut decoder = Decoder::new().unwrap();
        for index in 0..4 {
            let mut rgb = vec![0; 64 * 48 * 3];
            for pixel in rgb.as_chunks_mut::<3>().0 {
                pixel.copy_from_slice(&[40 + index * 20, 100, 170]);
            }
            let unit = encoder
                .encode_rgb(&rgb, u32::from(index) * 6000, index == 0 || index == 3)
                .unwrap();
            assert_eq!(unit.timestamp, u32::from(index) * 6000);
            if index == 0 || index == 3 {
                assert!(unit.keyframe);
                assert!(crate::h264::has_parameter_sets(&unit.data));
            }
            let frame = decoder.decode(&unit.data).unwrap().unwrap();
            assert_eq!(
                (frame.width, frame.height, frame.rgba.len()),
                (64, 48, 64 * 48 * 4)
            );
            let p = &frame.rgba[0..4];
            assert!((i16::from(p[0]) - i16::from(40 + index * 20)).abs() < 15);
            assert!((i16::from(p[1]) - 100).abs() < 15);
        }
        assert!(encoder.encode_rgb(&[0; 3], 0, true).is_err());
        assert!(decoder.decode(&[1, 2, 3]).is_err());
        assert!(
            Encoder::new(Config {
                width: u32::MAX,
                ..config
            })
            .is_err()
        );
    }
}
