//! Taking what an image says about its maker out of it — without touching a
//! single pixel.
//!
//! Photos carry where and when they were taken, with which camera, often the
//! owner's name. None of that belongs in a world others read. Only the
//! containers are taken apart: metadata blocks go, image data stays byte for
//! byte. What the picture needs to look right stays — its colour profile and
//! which way is up; the orientation as a minimal EXIF block that says
//! nothing else.

use super::{Format, Result};

/// The image with its metadata taken out.
pub fn strip(format: Format, raw: &[u8]) -> Result<Vec<u8>> {
    match format {
        Format::Jpeg => jpeg(raw),
        Format::Png => png(raw),
        Format::Webp => webp(raw),
        Format::Gif => gif(raw),
    }
}

fn broken() -> Box<dyn std::error::Error + Send + Sync> {
    "This image could not be read.".into()
}

fn u16_be(b: &[u8], at: usize) -> Result<u16> {
    Ok(u16::from_be_bytes(b.get(at..at + 2).ok_or_else(broken)?.try_into().expect("2 bytes")))
}

fn u32_be(b: &[u8], at: usize) -> Result<u32> {
    Ok(u32::from_be_bytes(b.get(at..at + 4).ok_or_else(broken)?.try_into().expect("4 bytes")))
}

fn u32_le(b: &[u8], at: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(b.get(at..at + 4).ok_or_else(broken)?.try_into().expect("4 bytes")))
}

// --- EXIF: only which way is up -------------------------------------------------

/// The orientation in a TIFF block (EXIF without its `Exif\0\0` head), if it
/// says anything but “as stored”.
fn orientation(tiff: &[u8]) -> Option<u16> {
    let little = match tiff.get(..4)? {
        b"II*\0" => true,
        b"MM\0*" => false,
        _ => return None,
    };
    let u16_at = |at: usize| -> Option<u16> {
        let b: [u8; 2] = tiff.get(at..at + 2)?.try_into().ok()?;
        Some(if little { u16::from_le_bytes(b) } else { u16::from_be_bytes(b) })
    };
    let u32_at = |at: usize| -> Option<u32> {
        let b: [u8; 4] = tiff.get(at..at + 4)?.try_into().ok()?;
        Some(if little { u32::from_le_bytes(b) } else { u32::from_be_bytes(b) })
    };
    let ifd = u32_at(4)? as usize;
    let count = u16_at(ifd)? as usize;
    for i in 0..count.min(512) {
        let entry = ifd + 2 + i * 12;
        if u16_at(entry)? == 0x0112 {
            let value = u16_at(entry + 8)?;
            return (2..=8).contains(&value).then_some(value);
        }
    }
    None
}

/// A TIFF block that holds nothing but the orientation.
fn orientation_tiff(value: u16) -> Vec<u8> {
    let mut out = Vec::with_capacity(26);
    out.extend_from_slice(b"MM\0*");
    out.extend_from_slice(&8u32.to_be_bytes());
    out.extend_from_slice(&1u16.to_be_bytes());
    out.extend_from_slice(&0x0112u16.to_be_bytes());
    out.extend_from_slice(&3u16.to_be_bytes());
    out.extend_from_slice(&1u32.to_be_bytes());
    out.extend_from_slice(&value.to_be_bytes());
    out.extend_from_slice(&[0, 0]);
    out.extend_from_slice(&0u32.to_be_bytes());
    out
}

/// The orientation an image file asks for — 1 when it asks nothing.
pub fn orientation_of(format: Format, raw: &[u8]) -> u16 {
    let found = match format {
        Format::Jpeg => jpeg_orientation(raw),
        Format::Png => png_chunks(raw).ok().and_then(|c| {
            c.into_iter().find(|(kind, _)| kind == b"eXIf").and_then(|(_, data)| orientation(data))
        }),
        Format::Webp => webp_chunks(raw).ok().and_then(|c| {
            c.into_iter().find(|(kind, _)| kind == b"EXIF").and_then(|(_, data)| orientation(exif_body(data)))
        }),
        Format::Gif => None,
    };
    found.unwrap_or(1)
}

/// EXIF data with or without its `Exif\0\0` head — writers differ.
fn exif_body(data: &[u8]) -> &[u8] {
    data.strip_prefix(b"Exif\0\0").unwrap_or(data)
}

// --- JPEG -------------------------------------------------------------------------

/// A JPEG segment: its marker and payload.
type Segment<'a> = (u8, &'a [u8]);

/// The segments before the image data, and where the image data starts.
fn jpeg_segments(raw: &[u8]) -> Result<(Vec<Segment<'_>>, usize)> {
    if raw.get(..2) != Some(&[0xFF, 0xD8]) {
        return Err(broken());
    }
    let mut at = 2;
    let mut out = Vec::new();
    loop {
        // Fill bytes before a marker are allowed.
        while raw.get(at) == Some(&0xFF) && raw.get(at + 1) == Some(&0xFF) {
            at += 1;
        }
        if raw.get(at) != Some(&0xFF) {
            return Err(broken());
        }
        let marker = *raw.get(at + 1).ok_or_else(broken)?;
        // The image data starts: everything from here stays as it is.
        if marker == 0xDA {
            return Ok((out, at));
        }
        if (0xD0..=0xD7).contains(&marker) || marker == 0x01 {
            out.push((marker, &raw[at + 2..at + 2]));
            at += 2;
            continue;
        }
        let length = u16_be(raw, at + 2)? as usize;
        if length < 2 {
            return Err(broken());
        }
        let payload = raw.get(at + 4..at + 2 + length).ok_or_else(broken)?;
        out.push((marker, payload));
        at += 2 + length;
    }
}

fn jpeg_orientation(raw: &[u8]) -> Option<u16> {
    let (segments, _) = jpeg_segments(raw).ok()?;
    segments
        .iter()
        .filter(|(m, p)| *m == 0xE1 && p.starts_with(b"Exif\0\0"))
        .find_map(|(_, p)| orientation(&p[6..]))
}

fn jpeg(raw: &[u8]) -> Result<Vec<u8>> {
    let (segments, data) = jpeg_segments(raw)?;
    let turned = jpeg_orientation(raw);
    let mut out = Vec::with_capacity(raw.len());
    out.extend_from_slice(&[0xFF, 0xD8]);
    let mut placed = false;
    let place = |out: &mut Vec<u8>| {
        if let Some(value) = turned {
            let mut payload = b"Exif\0\0".to_vec();
            payload.extend(orientation_tiff(value));
            out.extend_from_slice(&[0xFF, 0xE1]);
            out.extend_from_slice(&((payload.len() + 2) as u16).to_be_bytes());
            out.extend(payload);
        }
    };
    for (marker, payload) in segments {
        let keep = match marker {
            // JFIF stays first; the orientation follows right after it.
            0xE0 => true,
            // Only the colour profile of all APP2 blocks.
            0xE2 => payload.starts_with(b"ICC_PROFILE\0"),
            // Adobe's colour transform: needed to decode right.
            0xEE => true,
            // EXIF, XMP, IPTC, the other APPn and comments go.
            0xE1 | 0xE3..=0xED | 0xEF | 0xFE => false,
            _ => true,
        };
        if !placed && marker != 0xE0 {
            place(&mut out);
            placed = true;
        }
        if !keep {
            continue;
        }
        out.extend_from_slice(&[0xFF, marker]);
        if (0xD0..=0xD7).contains(&marker) || marker == 0x01 {
            continue;
        }
        out.extend_from_slice(&((payload.len() + 2) as u16).to_be_bytes());
        out.extend_from_slice(payload);
    }
    if !placed {
        place(&mut out);
    }
    out.extend_from_slice(&raw[data..]);
    Ok(out)
}

// --- PNG --------------------------------------------------------------------------

const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

fn png_chunks(raw: &[u8]) -> Result<Vec<([u8; 4], &[u8])>> {
    if !raw.starts_with(PNG_SIGNATURE) {
        return Err(broken());
    }
    let mut at = 8;
    let mut out = Vec::new();
    while at < raw.len() {
        let length = u32_be(raw, at)? as usize;
        let kind: [u8; 4] = raw.get(at + 4..at + 8).ok_or_else(broken)?.try_into().expect("4 bytes");
        let data = raw.get(at + 8..at + 8 + length).ok_or_else(broken)?;
        raw.get(at + 8 + length..at + 12 + length).ok_or_else(broken)?;
        out.push((kind, data));
        at += 12 + length;
        if &kind == b"IEND" {
            break;
        }
    }
    Ok(out)
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in bytes {
        crc ^= b as u32;
        for _ in 0..8 {
            crc = if crc & 1 == 1 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

fn png_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let start = out.len();
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let crc = crc32(&out[start..]);
    out.extend_from_slice(&crc.to_be_bytes());
}

fn png(raw: &[u8]) -> Result<Vec<u8>> {
    let chunks = png_chunks(raw)?;
    let turned = orientation_of(Format::Png, raw);
    let mut out = Vec::with_capacity(raw.len());
    out.extend_from_slice(PNG_SIGNATURE);
    for (kind, data) in chunks {
        match &kind {
            // Text of every kind, the time, EXIF.
            b"tEXt" | b"zTXt" | b"iTXt" | b"tIME" | b"eXIf" => {}
            b"IHDR" => {
                png_chunk(&mut out, &kind, data);
                if turned != 1 {
                    png_chunk(&mut out, b"eXIf", &orientation_tiff(turned));
                }
            }
            _ => png_chunk(&mut out, &kind, data),
        }
    }
    Ok(out)
}

// --- WebP -------------------------------------------------------------------------

fn webp_chunks(raw: &[u8]) -> Result<Vec<([u8; 4], &[u8])>> {
    if raw.get(..4) != Some(b"RIFF") || raw.get(8..12) != Some(b"WEBP") {
        return Err(broken());
    }
    let end = (8 + u32_le(raw, 4)? as usize).min(raw.len());
    let mut at = 12;
    let mut out = Vec::new();
    while at + 8 <= end {
        let kind: [u8; 4] = raw[at..at + 4].try_into().expect("4 bytes");
        let size = u32_le(raw, at + 4)? as usize;
        let data = raw.get(at + 8..at + 8 + size).ok_or_else(broken)?;
        out.push((kind, data));
        at += 8 + size + (size & 1);
    }
    Ok(out)
}

fn webp(raw: &[u8]) -> Result<Vec<u8>> {
    let chunks = webp_chunks(raw)?;
    let turned = orientation_of(Format::Webp, raw);
    let mut body = Vec::with_capacity(raw.len());
    let chunk = |body: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]| {
        body.extend_from_slice(kind);
        body.extend_from_slice(&(data.len() as u32).to_le_bytes());
        body.extend_from_slice(data);
        if data.len() & 1 == 1 {
            body.push(0);
        }
    };
    for (kind, data) in &chunks {
        match kind {
            b"EXIF" | b"XMP " => {}
            b"VP8X" => {
                let mut flags = data.to_vec();
                // No XMP any more; EXIF only if it carries the orientation.
                if let Some(f) = flags.first_mut() {
                    *f &= !0x04;
                    if turned == 1 {
                        *f &= !0x08;
                    } else {
                        *f |= 0x08;
                    }
                }
                chunk(&mut body, kind, &flags);
            }
            _ => chunk(&mut body, kind, data),
        }
    }
    // The orientation goes last, where EXIF belongs — only an extended file
    // (VP8X) can carry it, and only such a file had it before.
    if turned != 1 && chunks.iter().any(|(k, _)| k == b"VP8X") {
        chunk(&mut body, b"EXIF", &orientation_tiff(turned));
    }
    let mut out = Vec::with_capacity(body.len() + 12);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&((body.len() + 4) as u32).to_le_bytes());
    out.extend_from_slice(b"WEBP");
    out.extend(body);
    Ok(out)
}

// --- GIF --------------------------------------------------------------------------

/// The data sub-blocks from `at`, up to and with the closing zero.
fn sub_blocks(raw: &[u8], mut at: usize) -> Result<usize> {
    loop {
        let size = *raw.get(at).ok_or_else(broken)? as usize;
        at += 1;
        if size == 0 {
            return Ok(at);
        }
        at += size;
    }
}

fn gif(raw: &[u8]) -> Result<Vec<u8>> {
    if !(raw.starts_with(b"GIF87a") || raw.starts_with(b"GIF89a")) {
        return Err(broken());
    }
    let flags = *raw.get(10).ok_or_else(broken)?;
    let mut at = 13;
    if flags & 0x80 != 0 {
        at += 3 * (1 << ((flags & 0x07) + 1));
    }
    let mut out = raw.get(..at).ok_or_else(broken)?.to_vec();
    loop {
        match *raw.get(at).ok_or_else(broken)? {
            0x3B => {
                out.push(0x3B);
                return Ok(out);
            }
            0x2C => {
                let local = *raw.get(at + 9).ok_or_else(broken)?;
                let mut end = at + 10;
                if local & 0x80 != 0 {
                    end += 3 * (1 << ((local & 0x07) + 1));
                }
                // The LZW code size, then the image data.
                end = sub_blocks(raw, end + 1)?;
                out.extend_from_slice(raw.get(at..end).ok_or_else(broken)?);
                at = end;
            }
            0x21 => {
                let label = *raw.get(at + 1).ok_or_else(broken)?;
                let end = sub_blocks(raw, at + 2)?;
                let keep = match label {
                    // Timing and transparency; plain text drawn into the image.
                    0xF9 | 0x01 => true,
                    // Of the application blocks only the loop count.
                    0xFF => raw.get(at + 3..at + 14) == Some(b"NETSCAPE2.0") || raw.get(at + 3..at + 14) == Some(b"ANIMEXTS1.0"),
                    // Comments and everything unknown go.
                    _ => false,
                };
                if keep {
                    out.extend_from_slice(raw.get(at..end).ok_or_else(broken)?);
                }
                at = end;
            }
            _ => return Err(broken()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exif_with(orientation: u16, extra: &[u8]) -> Vec<u8> {
        // Orientation plus a fake GPS entry, little-endian like phones write.
        let mut tiff = b"II*\0".to_vec();
        tiff.extend_from_slice(&8u32.to_le_bytes());
        tiff.extend_from_slice(&2u16.to_le_bytes());
        tiff.extend_from_slice(&0x0112u16.to_le_bytes());
        tiff.extend_from_slice(&3u16.to_le_bytes());
        tiff.extend_from_slice(&1u32.to_le_bytes());
        tiff.extend_from_slice(&orientation.to_le_bytes());
        tiff.extend_from_slice(&[0, 0]);
        tiff.extend_from_slice(&0x8825u16.to_le_bytes());
        tiff.extend_from_slice(&4u16.to_le_bytes());
        tiff.extend_from_slice(&1u32.to_le_bytes());
        tiff.extend_from_slice(&0u32.to_le_bytes());
        tiff.extend_from_slice(&0u32.to_le_bytes());
        tiff.extend_from_slice(extra);
        tiff
    }

    fn segment(marker: u8, payload: &[u8]) -> Vec<u8> {
        let mut out = vec![0xFF, marker];
        out.extend_from_slice(&((payload.len() + 2) as u16).to_be_bytes());
        out.extend_from_slice(payload);
        out
    }

    #[test]
    fn a_jpeg_loses_its_exif_and_comments_but_keeps_profile_orientation_and_pixels() {
        let mut exif = b"Exif\0\0".to_vec();
        exif.extend(exif_with(6, b"Secret Street 12, 52.52N 13.40E"));
        let mut raw = vec![0xFF, 0xD8];
        raw.extend(segment(0xE0, b"JFIF\0\x01\x02\0\0\x01\0\x01\0\0"));
        raw.extend(segment(0xE1, &exif));
        raw.extend(segment(0xE1, b"http://ns.adobe.com/xap/1.0/\0<x:xmpmeta>Author: Marla</x:xmpmeta>"));
        raw.extend(segment(0xE2, b"ICC_PROFILE\0\x01\x01profile"));
        raw.extend(segment(0xED, b"Photoshop 3.0\0IPTC Marla"));
        raw.extend(segment(0xFE, b"taken by Marla"));
        raw.extend(segment(0xDB, b"\0quantisation"));
        let data = [0xFF, 0xDA, 0x00, 0x04, 0x01, 0x02, 0x12, 0x34, 0xFF, 0x00, 0x56, 0xFF, 0xD9];
        raw.extend_from_slice(&data);

        let out = strip(Format::Jpeg, &raw).unwrap();
        let text = String::from_utf8_lossy(&out);
        for gone in ["Secret Street", "Marla", "xmpmeta", "IPTC"] {
            assert!(!text.contains(gone), "{gone} is still in it");
        }
        assert!(text.contains("ICC_PROFILE"), "the colour profile stays");
        assert!(out.ends_with(&data), "the image data stays byte for byte");
        assert_eq!(jpeg_orientation(&out), Some(6), "which way is up stays");
        assert_eq!(strip(Format::Jpeg, &out).unwrap(), out, "stripping twice changes nothing");
    }

    #[test]
    fn a_png_loses_its_text_and_time() {
        let mut raw = PNG_SIGNATURE.to_vec();
        png_chunk(&mut raw, b"IHDR", &[0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0, 0, 0]);
        png_chunk(&mut raw, b"tEXt", b"Author\0Marla");
        png_chunk(&mut raw, b"iCCP", b"profile\0\0data");
        png_chunk(&mut raw, b"eXIf", &exif_with(1, b"GPS 52.52N"));
        png_chunk(&mut raw, b"tIME", &[7, 234, 10, 7, 12, 0, 0]);
        png_chunk(&mut raw, b"IDAT", b"pixels");
        png_chunk(&mut raw, b"IEND", b"");
        let out = strip(Format::Png, &raw).unwrap();
        let kinds: Vec<String> = png_chunks(&out).unwrap().iter().map(|(k, _)| String::from_utf8_lossy(k).to_string()).collect();
        assert_eq!(kinds, ["IHDR", "iCCP", "IDAT", "IEND"]);
        assert!(png_chunks(&out).unwrap().iter().any(|(k, d)| k == b"IDAT" && *d == b"pixels"));
    }

    #[test]
    fn a_webp_loses_exif_and_xmp_and_its_flags_say_so() {
        let mut body = Vec::new();
        let mut put = |kind: &[u8; 4], data: &[u8]| {
            body.extend_from_slice(kind);
            body.extend_from_slice(&(data.len() as u32).to_le_bytes());
            body.extend_from_slice(data);
            if data.len() & 1 == 1 {
                body.push(0);
            }
        };
        put(b"VP8X", &[0x0C, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        put(b"VP8L", b"pixels!");
        put(b"EXIF", &exif_with(1, b"GPS 52.52N"));
        put(b"XMP ", b"<x:xmpmeta>Marla</x:xmpmeta>");
        let mut raw = b"RIFF".to_vec();
        raw.extend_from_slice(&((body.len() + 4) as u32).to_le_bytes());
        raw.extend_from_slice(b"WEBP");
        raw.extend(body);
        let out = strip(Format::Webp, &raw).unwrap();
        let chunks = webp_chunks(&out).unwrap();
        let kinds: Vec<&[u8; 4]> = chunks.iter().map(|(k, _)| k).collect();
        assert_eq!(kinds, [b"VP8X", b"VP8L"]);
        assert_eq!(chunks[0].1[0] & 0x0C, 0, "no EXIF or XMP announced");
        assert_eq!(chunks[1].1, b"pixels!");
    }

    #[test]
    fn a_gif_loses_comments_and_keeps_its_loop() {
        let mut raw = b"GIF89a".to_vec();
        raw.extend_from_slice(&[1, 0, 1, 0, 0x80, 0, 0]);
        raw.extend_from_slice(&[0, 0, 0, 255, 255, 255]);
        raw.extend_from_slice(&[0x21, 0xFE, 5]);
        raw.extend_from_slice(b"Marla");
        raw.push(0);
        raw.extend_from_slice(&[0x21, 0xFF, 11]);
        raw.extend_from_slice(b"NETSCAPE2.0");
        raw.extend_from_slice(&[3, 1, 0, 0, 0]);
        raw.extend_from_slice(&[0x21, 0xF9, 4, 0, 10, 0, 0, 0]);
        raw.extend_from_slice(&[0x2C, 0, 0, 0, 0, 1, 0, 1, 0, 0, 2, 2, 0x44, 0x01, 0]);
        raw.push(0x3B);
        let out = strip(Format::Gif, &raw).unwrap();
        assert!(!String::from_utf8_lossy(&out).contains("Marla"));
        assert!(String::from_utf8_lossy(&out).contains("NETSCAPE2.0"));
        assert_eq!(out.len(), raw.len() - 9);
    }

    #[test]
    fn what_is_no_image_is_refused() {
        assert!(strip(Format::Jpeg, b"not a jpeg").is_err());
        assert!(strip(Format::Png, b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR").is_err());
    }
}
