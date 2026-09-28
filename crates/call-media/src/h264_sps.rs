//! Preflight decoder allocation limits. SPS parsing adapted from Serein's
//! bounded H.264 SPS reader; see THIRD_PARTY.md for source attribution.
const INVALID: &str = "Unsupported or oversized H264 sequence parameter set";
struct Bits {
    data: Vec<bool>,
    at: usize,
}
impl Bits {
    fn read(&mut self, count: usize) -> Result<u32, &'static str> {
        if count > 32 || count > self.data.len().saturating_sub(self.at) {
            return Err(INVALID);
        }
        let mut value = 0;
        for _ in 0..count {
            value = (value << 1) | u32::from(self.data[self.at]);
            self.at += 1;
        }
        Ok(value)
    }
    fn flag(&mut self) -> Result<bool, &'static str> {
        Ok(self.read(1)? != 0)
    }
    fn ue(&mut self) -> Result<u32, &'static str> {
        let mut zeros = 0;
        while !self.flag()? {
            zeros += 1;
            if zeros > 31 {
                return Err(INVALID);
            }
        }
        Ok(((1u32 << zeros) - 1) + self.read(zeros)?)
    }
    fn se(&mut self) -> Result<i64, &'static str> {
        let code = i64::from(self.ue()?);
        Ok(if code & 1 == 0 {
            -(code / 2)
        } else {
            (code + 1) / 2
        })
    }
}

pub(super) fn validate(escaped: &[u8]) -> Result<(), &'static str> {
    if escaped.is_empty() || escaped.len() > 4096 {
        return Err(INVALID);
    }
    let mut bytes = Vec::with_capacity(escaped.len());
    let mut zeros = 0;
    for (index, &byte) in escaped.iter().enumerate() {
        if zeros == 2 && byte == 3 {
            if escaped.get(index + 1).is_none_or(|&next| next > 3) {
                return Err(INVALID);
            }
            zeros = 0;
            continue;
        }
        bytes.push(byte);
        zeros = if byte == 0 { zeros + 1 } else { 0 };
    }
    let mut bits = Bits {
        data: bytes
            .iter()
            .flat_map(|byte| (0..8).rev().map(move |bit| byte & (1 << bit) != 0))
            .collect(),
        at: 0,
    };
    // Exclude rbsp_stop_one_bit and padding from the parser's readable input.
    let stop = bits.data.iter().rposition(|&bit| bit).ok_or(INVALID)?;
    bits.data.truncate(stop);
    let profile = bits.read(8)?;
    bits.read(16)?;
    if bits.ue()? > 31 {
        return Err(INVALID);
    }
    if profile == 100 {
        let chroma = bits.ue()?;
        if chroma != 1 {
            return Err(INVALID);
        }
        if bits.ue()? != 0 || bits.ue()? != 0 {
            return Err(INVALID);
        }
        bits.read(1)?;
        if bits.flag()? {
            for index in 0..8 {
                if bits.flag()? {
                    let mut last = 8i64;
                    let mut next = 8i64;
                    for _ in 0..if index < 6 { 16 } else { 64 } {
                        if next != 0 {
                            next = (last + bits.se()?).rem_euclid(256);
                        }
                        if next != 0 {
                            last = next;
                        }
                    }
                }
            }
        }
    } else if !matches!(profile, 66 | 77 | 88) {
        return Err(INVALID);
    }
    if bits.ue()? > 12 {
        return Err(INVALID);
    }
    match bits.ue()? {
        0 => {
            if bits.ue()? > 12 {
                return Err(INVALID);
            }
        }
        1 => {
            bits.read(1)?;
            bits.se()?;
            bits.se()?;
            let cycle = bits.ue()?;
            if cycle > 255 {
                return Err(INVALID);
            }
            for _ in 0..cycle {
                bits.se()?;
            }
        }
        2 => {}
        _ => return Err(INVALID),
    }
    if bits.ue()? > 16 {
        return Err(INVALID);
    } // max_num_ref_frames
    bits.read(1)?; // gaps_in_frame_num_value_allowed_flag
    let width_mbs = u64::from(bits.ue()?) + 1;
    let height_maps = u64::from(bits.ue()?) + 1;
    let progressive = bits.flag()?;
    if !progressive {
        bits.read(1)?;
    }
    let frame_factor = if progressive { 1u64 } else { 2 };
    let width = width_mbs.checked_mul(16).ok_or(INVALID)?;
    let height = height_maps.checked_mul(16 * frame_factor).ok_or(INVALID)?;
    // Decoder surfaces use coded dimensions, before crop. 1080p uses 1088 rows.
    if width > 1920 || height > 1920 || width * height > 1920 * 1088 {
        return Err(INVALID);
    }
    bits.read(1)?; // direct_8x8_inference_flag
    let (mut crop_x, mut crop_y) = (0, 0);
    if bits.flag()? {
        crop_x = (u64::from(bits.ue()?) + u64::from(bits.ue()?)) * 2;
        crop_y = (u64::from(bits.ue()?) + u64::from(bits.ue()?)) * 2 * frame_factor;
    }
    let width = width.checked_sub(crop_x).ok_or(INVALID)?;
    let height = height.checked_sub(crop_y).ok_or(INVALID)?;
    if width == 0 || height == 0 || width * height > 1920 * 1080 {
        return Err(INVALID);
    }
    Ok(())
}
