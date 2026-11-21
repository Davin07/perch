//! Webcam frames through Media Foundation's Source Reader, for the live preview
//! and `perch camera grab`. RGB32 out, converted to straight RGBA.

use std::ptr;

use windows::core::{Interface, PWSTR};
use windows::Win32::Media::MediaFoundation::{
    IMFActivate, IMFMediaSource, IMFMediaType, IMFSourceReader, MFEnumDeviceSources, MFMediaType_Video, MFStartup,
    MFCreateAttributes, MFCreateMediaType, MFCreateSourceReaderFromMediaSource,
    MF_DEVSOURCE_ATTRIBUTE_FRIENDLY_NAME, MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE,
    MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID, MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK,
    MF_MT_FRAME_RATE, MF_MT_FRAME_SIZE,
    MF_MT_MAJOR_TYPE, MF_MT_SUBTYPE, MF_SOURCE_READER_ENABLE_ADVANCED_VIDEO_PROCESSING,
    MFVideoFormat_MJPG,
    MF_SOURCE_READER_FIRST_VIDEO_STREAM, MF_VERSION, MFSTARTUP_FULL, MFVideoFormat_RGB32,
};
use windows::Win32::System::Com::{CoInitializeEx, CoTaskMemFree, COINIT_MULTITHREADED};

use crate::error::{Error, Result};

const STREAM: u32 = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;

fn mf(e: windows::core::Error, what: &str) -> Error {
    Error::System(format!("{what}: {e}"))
}

fn string_attr(act: &IMFActivate, key: &windows::core::GUID) -> String {
    unsafe {
        let mut p = PWSTR::null();
        let mut len = 0u32;
        if act.GetAllocatedString(key, &mut p, &mut len).is_err() {
            return String::new();
        }
        let s = p.to_string().unwrap_or_default();
        CoTaskMemFree(Some(p.0 as *const _));
        s
    }
}

fn friendly(act: &IMFActivate) -> String {
    string_attr(act, &MF_DEVSOURCE_ATTRIBUTE_FRIENDLY_NAME)
}

pub struct Capture {
    reader: IMFSourceReader,
    width: u32,
    height: u32,
    pub name: String,
}

impl Capture {
    pub fn open(hint: &str, size: (u32, u32), fps: u32) -> Result<Capture> {
        static START: std::sync::Once = std::sync::Once::new();
        START.call_once(|| unsafe {
            let _ = MFStartup(MF_VERSION, MFSTARTUP_FULL);
        });
        unsafe {
            // Each thread that touches these interfaces needs its own apartment.
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let mut attrs = None;
            MFCreateAttributes(&mut attrs, 2).map_err(|e| mf(e, "MFCreateAttributes"))?;
            let attrs = attrs.ok_or_else(|| Error::System("no MF attributes".into()))?;
            attrs
                .SetGUID(
                    &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE,
                    &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID,
                )
                .map_err(|e| mf(e, "source type"))?;

            let mut devices: *mut Option<IMFActivate> = ptr::null_mut();
            let mut count = 0u32;
            MFEnumDeviceSources(&attrs, &mut devices, &mut count).map_err(|e| mf(e, "enumerate"))?;
            if count == 0 || devices.is_null() {
                return Err(Error::NotFound("camera"));
            }
            let list = std::slice::from_raw_parts(devices, count as usize);

            let want = hint.to_lowercase();
            let mut pick: Option<(String, IMFActivate)> = None;
            for act in list.iter().flatten() {
                let name = friendly(act);
                let link = string_attr(act, &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK);
                let hit = (!want.is_empty() && name.to_lowercase().contains(&want))
                    || (want.contains("brio") && link.to_lowercase().contains("vid_046d"));
                if pick.is_none() || hit {
                    pick = Some((name, act.clone()));
                }
                if hit {
                    break;
                }
            }
            let (name, act) = pick.ok_or(Error::NotFound("camera"))?;

            let source: IMFMediaSource = act
                .ActivateObject()
                .map_err(|e| mf(e, "activate camera"))?;
            // Let the reader insert the video processor that converts the
            // camera's native NV12/MJPEG into RGB32.
            let mut rattrs = None;
            MFCreateAttributes(&mut rattrs, 1).map_err(|e| mf(e, "reader attributes"))?;
            if let Some(rattrs) = &rattrs {
                rattrs
                    .SetUINT32(&MF_SOURCE_READER_ENABLE_ADVANCED_VIDEO_PROCESSING, 1)
                    .map_err(|e| mf(e, "video processing"))?;
            }
            let reader = MFCreateSourceReaderFromMediaSource(&source, rattrs.as_ref())
                .map_err(|e| mf(e, "source reader"))?;

            // Pick a native mode that can actually deliver the asked-for rate;
            // the reader's default is often the first mode, which is 15 fps.
            let mut best: Option<(i64, IMFMediaType)> = None;
            for i in 0..512 {
                let Ok(native) = reader.GetNativeMediaType(STREAM, i) else {
                    break;
                };
                let Ok(sz) = native.GetUINT64(&MF_MT_FRAME_SIZE) else {
                    continue;
                };
                let (nw, nh) = ((sz >> 32) as i64, (sz & 0xffff_ffff) as i64);
                let rate = native.GetUINT64(&MF_MT_FRAME_RATE).unwrap_or(0);
                let num = (rate >> 32) as f64;
                let den = (rate & 0xffff_ffff).max(1) as f64;
                let nfps = num / den;
                let size_penalty = (nw - size.0 as i64).abs() + (nh - size.1 as i64).abs();
                let fps_penalty = if nfps + 0.5 >= fps as f64 { 0 } else { 1000 };
                let mjpg = native
                    .GetGUID(&MF_MT_SUBTYPE)
                    .map(|g| g == MFVideoFormat_MJPG)
                    .unwrap_or(false);
                let score = size_penalty * 10 + fps_penalty + if mjpg { 0 } else { 1 };
                if best.as_ref().map(|(b, _)| score < *b).unwrap_or(true) {
                    best = Some((score, native));
                }
            }
            if let Some((_, native)) = best {
                let _ = reader.SetCurrentMediaType(STREAM, None, &native);
            }
            let mt = MFCreateMediaType().map_err(|e| mf(e, "media type"))?;
            mt.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)
                .map_err(|e| mf(e, "major type"))?;
            mt.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32)
                .map_err(|e| mf(e, "RGB32"))?;
            let _ = mt.SetUINT64(&MF_MT_FRAME_SIZE, ((size.0 as u64) << 32) | size.1 as u64);
            let _ = mt.SetUINT64(&MF_MT_FRAME_RATE, ((fps.max(1) as u64) << 32) | 1);
            if reader.SetCurrentMediaType(STREAM, None, &mt).is_err() {
                // The camera may refuse that size/rate; fall back to its own.
                let plain = MFCreateMediaType().map_err(|e| mf(e, "media type"))?;
                plain.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)
                    .map_err(|e| mf(e, "major type"))?;
                plain.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32)
                    .map_err(|e| mf(e, "RGB32"))?;
                reader
                    .SetCurrentMediaType(STREAM, None, &plain)
                    .map_err(|e| mf(e, "set format"))?;
            }

            let cur = reader
                .GetCurrentMediaType(STREAM)
                .map_err(|e| mf(e, "current format"))?;
            let size = cur
                .GetUINT64(&MF_MT_FRAME_SIZE)
                .map_err(|e| mf(e, "frame size"))?;
            Ok(Capture {
                reader,
                width: (size >> 32) as u32,
                height: (size & 0xffff_ffff) as u32,
                name,
            })
        }
    }

    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Next frame as RGBA, top-down. `Ok(None)` when the reader had nothing ready.
    pub fn frame(&mut self) -> Result<Option<(u32, u32, Vec<u8>)>> {
        unsafe {
            let mut flags = 0u32;
            let mut sample = None;
            self.reader
                .ReadSample(STREAM, 0, None, Some(&mut flags), None, Some(&mut sample))
                .map_err(|e| mf(e, "ReadSample"))?;
            let Some(sample) = sample else {
                return Ok(None);
            };
            let buf = sample
                .ConvertToContiguousBuffer()
                .map_err(|e| mf(e, "buffer"))?;
            let mut base: *mut u8 = ptr::null_mut();
            let mut max = 0u32;
            let mut cur = 0u32;
            buf.Lock(&mut base, Some(&mut max), Some(&mut cur))
                .map_err(|e| mf(e, "lock"))?;
            let w = self.width as usize;
            let h = self.height as usize;
            let stride = w * 4;
            let mut rgba = Vec::new();
            if cur as usize >= stride * h {
                let src = std::slice::from_raw_parts(base, stride * h);
                rgba = vec![0u8; stride * h];
                for (dst, px) in rgba.chunks_exact_mut(4).zip(src.chunks_exact(4)) {
                    dst[0] = px[2];
                    dst[1] = px[1];
                    dst[2] = px[0];
                    dst[3] = 255;
                }
            }
            let _ = buf.Unlock();
            if rgba.is_empty() {
                return Ok(None);
            }
            Ok(Some((self.width, self.height, rgba)))
        }
    }
}
