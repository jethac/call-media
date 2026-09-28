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

#[cfg(test)]
mod tests {
    use super::*;
    fn sps(width_mbs: u32, height_maps: u32, bottom_crop: u32, refs: u32) -> Vec<u8> {
        fn ue(bits: &mut Vec<bool>, value: u32) {
            let code = u64::from(value) + 1;
            let size = 64 - code.leading_zeros();
            bits.extend(std::iter::repeat_n(false, size as usize - 1));
            bits.extend((0..size).rev().map(|shift| code & (1 << shift) != 0));
        }
        let mut bits: Vec<bool> = [66u8, 0, 40]
            .into_iter()
            .flat_map(|byte| (0..8).rev().map(move |shift| byte & (1 << shift) != 0))
            .collect();
        for value in [0, 0, 0, 0, refs] {
            ue(&mut bits, value);
        }
        bits.push(false);
        ue(&mut bits, width_mbs - 1);
        ue(&mut bits, height_maps - 1);
        bits.extend([true, true, bottom_crop != 0]);
        if bottom_crop != 0 {
            for value in [0, 0, 0, bottom_crop] {
                ue(&mut bits, value);
            }
        }
        bits.extend([false, true]); // No VUI; RBSP stop bit.
        while !bits.len().is_multiple_of(8) {
            bits.push(false);
        }
        let mut escaped = Vec::new();
        let mut zeros = 0;
        for chunk in bits.as_chunks::<8>().0 {
            let byte = chunk
                .iter()
                .fold(0u8, |value, &bit| (value << 1) | u8::from(bit));
            if zeros == 2 && byte <= 3 {
                escaped.push(3);
                zeros = 0;
            }
            escaped.push(byte);
            zeros = if byte == 0 { zeros + 1 } else { 0 };
        }
        escaped
    }
    #[test]
    fn coded_dimensions_and_crop_are_both_bounded() {
        assert!(validate(&sps(40, 30, 0, 1)).is_ok()); // 640x480
        assert!(validate(&sps(120, 68, 4, 16)).is_ok()); // 1920x1080 from 1088 coded rows
        assert!(validate(&sps(120, 68, 0, 1)).is_err()); // visible height too large
        assert!(validate(&sps(121, 68, 4, 1)).is_err()); // coded width too large
        assert!(validate(&sps(120, 256, 1900, 1)).is_err()); // tiny crop cannot hide huge surface
        assert!(validate(&sps(40, 30, 241, 1)).is_err()); // crop underflow
        assert!(validate(&sps(40, 30, 0, 17)).is_err()); // excessive reference surfaces
    }
    #[test]
    fn malformed_sps_and_unbounded_codes_are_rejected() {
        assert!(validate(&[]).is_err());
        assert!(validate(&vec![0; 4097]).is_err());
        assert!(validate(&[66, 0, 40, 0, 0, 0, 0, 0, 0x80]).is_err());
        assert!(validate(&[66, 0, 40, 0, 0, 3, 4]).is_err());
        let good = sps(40, 30, 0, 1);
        for len in 0..good.len().saturating_sub(1) {
            assert!(validate(&good[..len]).is_err());
        }
        let mut frame = vec![0, 0, 0, 1, 0x67];
        frame.extend(sps(121, 68, 4, 1));
        assert!(crate::h264::validate_decode(&frame).is_err());
    }
}
