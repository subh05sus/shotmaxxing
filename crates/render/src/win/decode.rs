//! Reads a recording's pictures with Media Foundation's source reader, converted to RGB on the way and cropped to
//! the part that shows (videos are coded in blocks of 16 lines, so a 1800-line recording decodes as 1808).

use std::path::Path;

use tiny_skia::Pixmap;
use windows::Win32::Media::MediaFoundation::{
    IMF2DBuffer, IMFSample, IMFSourceReader, MF_MT_DEFAULT_STRIDE, MF_MT_FRAME_SIZE, MF_MT_MAJOR_TYPE,
    MF_MT_MINIMUM_DISPLAY_APERTURE, MF_MT_SUBTYPE, MF_SOURCE_READER_ALL_STREAMS,
    MF_SOURCE_READER_ENABLE_ADVANCED_VIDEO_PROCESSING, MF_SOURCE_READER_FIRST_VIDEO_STREAM,
    MF_SOURCE_READERF_CURRENTMEDIATYPECHANGED, MF_SOURCE_READERF_ENDOFSTREAM, MFCreateAttributes,
    MFCreateMediaType, MFCreateSourceReaderFromURL, MFMediaType_Video, MFVideoArea, MFVideoFormat_RGB32,
};
use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
use windows::Win32::System::Variant::VT_I8;
use windows::core::{GUID, HSTRING, Interface};

use super::system;
use crate::RenderError;

const VIDEO: u32 = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;

/// A decoded picture and when it shows, in seconds from the recording's first frame.
type Decoded = (f64, Pixmap);

pub struct Decoder {
    reader: IMFSourceReader,
    /// Bytes from one row to the next in the decoded picture; negative when the rows run bottom to top.
    stride: i32,
    height: u32,
    /// The part of the decoded picture that shows: x, y, width, height.
    crop: (u32, u32, u32, u32),
    /// The first frame's timestamp, in 100 ns units: the recording's time zero.
    base: Option<i64>,
    current: Option<Decoded>,
    next: Option<Decoded>,
    /// A picture's memory, kept for the next one.
    spare: Option<Pixmap>,
    done: bool,
}

impl Decoder {
    pub fn open(path: &Path) -> Result<Self, RenderError> {
        let failed = |e: windows::core::Error| {
            RenderError::System(format!("couldn't read the recording: {}", e.message()))
        };
        // SAFETY: Media Foundation calls with valid objects; the path outlives the call that reads it.
        let reader = unsafe {
            let mut attributes = None;
            MFCreateAttributes(&mut attributes, 1).map_err(failed)?;
            let attributes =
                attributes.ok_or_else(|| RenderError::System("Media Foundation is missing".into()))?;
            // Converts the decoded YUV to RGB, on the graphics card where it can.
            attributes.SetUINT32(&MF_SOURCE_READER_ENABLE_ADVANCED_VIDEO_PROCESSING, 1).map_err(failed)?;
            let reader = MFCreateSourceReaderFromURL(&HSTRING::from(path), &attributes).map_err(failed)?;
            reader.SetStreamSelection(MF_SOURCE_READER_ALL_STREAMS.0 as u32, false).map_err(failed)?;
            reader
                .SetStreamSelection(VIDEO, true)
                .map_err(|_| RenderError::Project("the recording has no video in it".into()))?;
            let rgb = MFCreateMediaType().map_err(failed)?;
            rgb.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video).map_err(failed)?;
            rgb.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32).map_err(failed)?;
            reader.SetCurrentMediaType(VIDEO, None, &rgb).map_err(|_| {
                RenderError::Project("the recording's video can't be decoded on this computer".into())
            })?;
            reader
        };
        let mut decoder = Self {
            reader,
            stride: 0,
            height: 0,
            crop: (0, 0, 0, 0),
            base: None,
            current: None,
            next: None,
            spare: None,
            done: false,
        };
        decoder.read_format()?;
        Ok(decoder)
    }

    /// Like `open`, but starts reading a second before `t`, so the picture at `t` comes without decoding everything
    /// before it. Times still count from the recording's first frame.
    pub fn open_near(path: &Path, t: f64) -> Result<Self, RenderError> {
        let mut decoder = Self::open(path)?;
        if t < 2.0 {
            return Ok(decoder);
        }
        // Time zero is the first frame's timestamp; reading one frame finds it.
        decoder.next = decoder.read()?;
        let Some(base) = decoder.base else { return Ok(decoder) };
        decoder.next = None;
        decoder.done = false;
        let mut position = PROPVARIANT::default();
        // SAFETY: a VT_I8 PROPVARIANT holds its value in `hVal` and owns nothing, so it needs no clearing. Seeking
        // lands on the key frame before the position, and reading goes on from there.
        unsafe {
            let value = &mut *position.Anonymous.Anonymous;
            value.vt = VT_I8;
            value.Anonymous.hVal = base + ((t - 1.0) * 1e7) as i64;
            decoder.reader.SetCurrentPosition(&GUID::zeroed(), &position).map_err(system)?;
        }
        Ok(decoder)
    }

    /// The decoded pictures' layout, read again whenever the reader says it changed.
    fn read_format(&mut self) -> Result<(), RenderError> {
        // SAFETY: reads attributes of the reader's own media type; the blob fits an `MFVideoArea` exactly.
        unsafe {
            let format = self.reader.GetCurrentMediaType(VIDEO).map_err(system)?;
            let size = format.GetUINT64(&MF_MT_FRAME_SIZE).map_err(system)?;
            let (width, height) = ((size >> 32) as u32, size as u32);
            self.height = height;
            self.stride = format.GetUINT32(&MF_MT_DEFAULT_STRIDE).map_or((width * 4) as i32, |s| s as i32);
            let mut area = MFVideoArea::default();
            let blob = std::slice::from_raw_parts_mut(
                (&raw mut area).cast::<u8>(),
                std::mem::size_of::<MFVideoArea>(),
            );
            self.crop = match format.GetBlob(&MF_MT_MINIMUM_DISPLAY_APERTURE, blob, None) {
                Ok(()) if area.Area.cx > 0 && area.Area.cy > 0 => {
                    let x = (area.OffsetX.value.max(0) as u32).min(width.saturating_sub(1));
                    let y = (area.OffsetY.value.max(0) as u32).min(height.saturating_sub(1));
                    (x, y, (area.Area.cx as u32).min(width - x), (area.Area.cy as u32).min(height - y))
                }
                _ => (0, 0, width, height),
            };
        }
        if self.crop.2 == 0 || self.crop.3 == 0 {
            return Err(RenderError::Project("the recording's pictures are empty".into()));
        }
        Ok(())
    }

    /// The next picture, or `None` at the end.
    fn read(&mut self) -> Result<Option<Decoded>, RenderError> {
        while !self.done {
            let (mut flags, mut timestamp, mut sample) = (0u32, 0i64, None::<IMFSample>);
            // SAFETY: the out-pointers are valid for the call.
            unsafe {
                self.reader.ReadSample(
                    VIDEO,
                    0,
                    None,
                    Some(&raw mut flags),
                    Some(&raw mut timestamp),
                    Some(&raw mut sample),
                )
            }
            .map_err(|e| {
                RenderError::System(format!("the recording couldn't be decoded: {}", e.message()))
            })?;
            if flags & MF_SOURCE_READERF_CURRENTMEDIATYPECHANGED.0 as u32 != 0 {
                self.read_format()?;
            }
            if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                self.done = true;
            }
            // A gap in the stream has no picture.
            let Some(sample) = sample else { continue };
            let base = *self.base.get_or_insert(timestamp);
            let picture = self.picture(&sample)?;
            return Ok(Some(((timestamp - base) as f64 / 1e7, picture)));
        }
        Ok(None)
    }

    /// Copies a decoded sample's picture out as opaque RGBA.
    fn picture(&mut self, sample: &IMFSample) -> Result<Pixmap, RenderError> {
        let (cx, cy, cw, ch) = self.crop;
        let mut pixmap = match self.spare.take() {
            Some(p) if p.width() == cw && p.height() == ch => p,
            _ => Pixmap::new(cw, ch)
                .ok_or_else(|| RenderError::System("a picture that size can't be kept".into()))?,
        };
        // SAFETY: the buffer stays locked while its rows are read, and every row read lies inside it: the 2-D lock
        // gives the first row and the true pitch, and the plain lock is checked against its length.
        unsafe {
            let buffer = sample.ConvertToContiguousBuffer().map_err(system)?;
            if let Ok(two_d) = buffer.cast::<IMF2DBuffer>() {
                let (mut first, mut pitch) = (std::ptr::null_mut(), 0i32);
                two_d.Lock2D(&mut first, &mut pitch).map_err(system)?;
                copy_rows(first, pitch as isize, (cx, cy, cw, ch), &mut pixmap);
                two_d.Unlock2D().map_err(system)?;
            } else {
                let (mut data, mut length) = (std::ptr::null_mut(), 0u32);
                buffer.Lock(&mut data, None, Some(&raw mut length)).map_err(system)?;
                let pitch = self.stride as isize;
                let needed = pitch.unsigned_abs() * self.height as usize;
                if (length as usize) < needed {
                    let _ = buffer.Unlock();
                    return Err(RenderError::System("a decoded picture was cut short".into()));
                }
                // Rows that run bottom to top start from the last one in memory.
                let first = if pitch < 0 { data.offset(-pitch * (self.height as isize - 1)) } else { data };
                copy_rows(first, pitch, (cx, cy, cw, ch), &mut pixmap);
                buffer.Unlock().map_err(system)?;
            }
        }
        Ok(pixmap)
    }

    /// The size of the pictures that show.
    #[cfg(test)]
    pub fn size(&self) -> (u32, u32) {
        (self.crop.2, self.crop.3)
    }

    /// Reads every picture left, and says how many there were.
    #[cfg(test)]
    pub fn count(&mut self) -> Result<u64, RenderError> {
        let mut count = 0;
        while self.read()?.is_some() {
            count += 1;
        }
        Ok(count)
    }

    /// The picture on screen `t` seconds into the recording. Calls must not go back in time.
    pub fn frame_at(&mut self, t: f64) -> Result<Option<&Pixmap>, RenderError> {
        loop {
            if self.next.is_none() {
                self.next = self.read()?;
            }
            match &self.next {
                Some((pts, _)) if *pts <= t + 1e-4 => {
                    if let Some((_, old)) = self.current.take() {
                        self.spare = Some(old);
                    }
                    self.current = self.next.take();
                }
                _ => break,
            }
        }
        // Before the first stored frame, show the first frame.
        Ok(self.current.as_ref().or(self.next.as_ref()).map(|(_, picture)| picture))
    }
}

/// Copies the `crop` rectangle of 32-bit BGRX rows, the first at `first` and each `pitch` bytes after the last, into
/// `out` as opaque RGBA.
///
/// # Safety
/// Every row of the rectangle must be readable: `first + (y + crop.y) * pitch + (crop.x + crop.width) * 4` lies
/// inside the locked buffer for each row.
unsafe fn copy_rows(first: *const u8, pitch: isize, crop: (u32, u32, u32, u32), out: &mut Pixmap) {
    let (cx, cy, cw, ch) = (crop.0 as isize, crop.1 as isize, crop.2 as usize, crop.3 as isize);
    let width = out.width() as usize;
    for (y, row) in out.data_mut().chunks_exact_mut(width * 4).enumerate().take(ch as usize) {
        // SAFETY: the caller promises the row is inside the buffer.
        let source =
            unsafe { std::slice::from_raw_parts(first.offset((cy + y as isize) * pitch + cx * 4), cw * 4) };
        for (o, i) in row.as_chunks_mut::<4>().0.iter_mut().zip(source.as_chunks::<4>().0) {
            *o = [i[2], i[1], i[0], 255];
        }
    }
}
