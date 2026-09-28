//! Bounded Annex-B H.264 access unit validation.
const MAX_FRAGMENTS: usize = 2048;
pub fn validate_source(frame: &[u8]) -> Result<(), &'static str> {
    if frame.len() > 2 * 1024 * 1024 {
        return Err("H264 frame exceeds the sharing limit");
    }
    nalus(frame).map(|_| ())
}
fn nalus(frame: &[u8]) -> Result<Vec<&[u8]>, &'static str> {
    let mut starts = Vec::new();
    let mut at = 0;
    while at + 3 <= frame.len() {
        let size = if frame[at..].starts_with(&[0, 0, 0, 1]) {
            4
        } else if frame[at..].starts_with(&[0, 0, 1]) {
            3
        } else {
            at += 1;
            continue;
        };
        if starts.len() == MAX_FRAGMENTS {
            return Err("H264 frame exceeds fragment limit");
        }
        starts.push((at, size));
        at += size;
    }
    if starts.is_empty() || starts[0].0 != 0 {
        return Err("H264 frame is not Annex-B");
    }
    let mut out = Vec::with_capacity(starts.len());
    for (index, (start, size)) in starts.iter().enumerate() {
        let end = starts.get(index + 1).map_or(frame.len(), |next| next.0);
        if start + size >= end {
            return Err("H264 frame contains an empty NAL unit");
        }
        out.push(&frame[start + size..end]);
    }
    Ok(out)
}
pub fn has_parameter_sets(frame: &[u8]) -> bool {
    let (mut sps, mut pps) = (false, false);
    let mut at = 0;
    while at + 4 <= frame.len() {
        let size = if frame[at..].starts_with(&[0, 0, 0, 1]) {
            4
        } else if frame[at..].starts_with(&[0, 0, 1]) {
            3
        } else {
            at += 1;
            continue;
        };
        match frame.get(at + size).map(|nal| nal & 0x1f) {
            Some(7) => sps = true,
            Some(8) => pps = true,
            _ => {}
        }
        at += size;
    }
    sps && pps
}

/// True when the cleartext Annex-B access unit carries an IDR slice.
pub fn is_keyframe(frame: &[u8]) -> bool {
    let mut at = 0;
    while at + 4 <= frame.len() {
        let size = if frame[at..].starts_with(&[0, 0, 0, 1]) {
            4
        } else if frame[at..].starts_with(&[0, 0, 1]) {
            3
        } else {
            at += 1;
            continue;
        };
        if frame.get(at + size).is_some_and(|nal| nal & 0x1f == 5) {
            return true;
        }
        at += size;
    }
    false
}

/// Preflight all SPS declarations before native decoding. Supports baseline,
/// main, extended and high profiles with 8-bit 4:2:0 samples, through 1080p.
/// This bounds coded surfaces even when a small crop is advertised. It is not
/// a complete H.264 bitstream verifier; native decoder errors still apply.
pub fn validate_decode(frame: &[u8]) -> Result<(), &'static str> {
    validate_source(frame)?;
    for nal in nalus(frame)? {
        let kind = nal[0] & 31;
        if nal[0] & 0x80 != 0 || !(1..=12).contains(&kind) {
            return Err("Unsupported H264 NAL unit");
        }
        if kind == 7 {
            crate::h264_sps::validate(&nal[1..])?;
        }
    }
    Ok(())
}
