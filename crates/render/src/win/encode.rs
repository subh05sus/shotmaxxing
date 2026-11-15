//! Writes the finished frames to an MP4 with Media Foundation's sink writer, which uses the graphics card's
//! encoder where there is one and Windows' own otherwise.

use std::path::Path;

use tiny_skia::Pixmap;
use windows::Win32::Media::MediaFoundation::{
    IMFSinkWriter, MF_MT_AVG_BITRATE, MF_MT_DEFAULT_STRIDE, MF_MT_FRAME_RATE, MF_MT_FRAME_SIZE,
    MF_MT_INTERLACE_MODE, MF_MT_MAJOR_TYPE, MF_MT_MPEG2_PROFILE, MF_MT_PIXEL_ASPECT_RATIO, MF_MT_SUBTYPE,
    MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, MF_SINK_WRITER_DISABLE_THROTTLING, MF_TRANSCODE_CONTAINERTYPE,
    MFCreateAttributes, MFCreateMediaType, MFCreateMemoryBuffer, MFCreateSample, MFCreateSinkWriterFromURL,
    MFMediaType_Video, MFTranscodeContainerType_MPEG4, MFVideoFormat_H264, MFVideoFormat_HEVC,
    MFVideoFormat_RGB32, MFVideoInterlace_Progressive, eAVEncH264VProfile_High,
};
use windows::core::HSTRING;

use super::system;
use crate::{Format, RenderError};

fn pack(high: u32, low: u32) -> u64 {
    (u64::from(high) << 32) | u64::from(low)
}

pub struct VideoWriter {
    writer: IMFSinkWriter,
    stream: u32,
    width: u32,
    height: u32,
    fps: u32,
    written: u64,
}

impl VideoWriter {
    pub fn new(
        path: &Path,
        width: u32,
        height: u32,
        fps: u32,
        format: Format,
        bitrate: f64,
    ) -> Result<Self, RenderError> {
        let hevc = format == Format::Hevc;
        let refused = |_| {
            RenderError::Project(if hevc {
                "this computer can't encode HEVC (Windows needs the HEVC Video Extensions); export MP4 instead".into()
            } else {
                "this computer can't encode H.264 video".into()
            })
        };
        // SAFETY: Media Foundation calls with valid objects; the path outlives the call that reads it.
        unsafe {
            let mut attributes = None;
            MFCreateAttributes(&mut attributes, 3).map_err(system)?;
            let attributes =
                attributes.ok_or_else(|| RenderError::System("Media Foundation is missing".into()))?;
            attributes.SetUINT32(&MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, 1).map_err(system)?;
            attributes
                .SetGUID(&MF_TRANSCODE_CONTAINERTYPE, &MFTranscodeContainerType_MPEG4)
                .map_err(system)?;
            // Frames come as fast as they're drawn; nothing is live.
            attributes.SetUINT32(&MF_SINK_WRITER_DISABLE_THROTTLING, 1).map_err(system)?;
            let writer = MFCreateSinkWriterFromURL(&HSTRING::from(path), None, &attributes).map_err(|e| {
                RenderError::System(format!("couldn't start the video file: {}", e.message()))
            })?;

            let output = MFCreateMediaType().map_err(system)?;
            output.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video).map_err(system)?;
            output
                .SetGUID(&MF_MT_SUBTYPE, if hevc { &MFVideoFormat_HEVC } else { &MFVideoFormat_H264 })
                .map_err(system)?;
            output.SetUINT32(&MF_MT_AVG_BITRATE, bitrate.clamp(1e5, 4e9) as u32).map_err(system)?;
            if !hevc {
                output.SetUINT32(&MF_MT_MPEG2_PROFILE, eAVEncH264VProfile_High.0 as u32).map_err(system)?;
            }
            output.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32).map_err(system)?;
            output.SetUINT64(&MF_MT_FRAME_SIZE, pack(width, height)).map_err(system)?;
            output.SetUINT64(&MF_MT_FRAME_RATE, pack(fps, 1)).map_err(system)?;
            output.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, pack(1, 1)).map_err(system)?;
            let stream = writer.AddStream(&output).map_err(refused)?;

            let input = MFCreateMediaType().map_err(system)?;
            input.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video).map_err(system)?;
            input.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32).map_err(system)?;
            input.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32).map_err(system)?;
            input.SetUINT64(&MF_MT_FRAME_SIZE, pack(width, height)).map_err(system)?;
            input.SetUINT64(&MF_MT_FRAME_RATE, pack(fps, 1)).map_err(system)?;
            input.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, pack(1, 1)).map_err(system)?;
            // The rows run top to bottom. RGB is taken as bottom to top unless the stride says otherwise.
            input.SetUINT32(&MF_MT_DEFAULT_STRIDE, width * 4).map_err(system)?;
            writer.SetInputMediaType(stream, &input, None).map_err(refused)?;
            writer.BeginWriting().map_err(refused)?;
            Ok(Self { writer, stream, width, height, fps, written: 0 })
        }
    }

    /// When frame `i` starts, in 100 ns units.
    fn time(&self, i: u64) -> i64 {
        (i128::from(i) * 10_000_000 / i128::from(self.fps)) as i64
    }

    /// Adds the next frame: opaque RGBA, top row first, at the writer's size.
    pub fn write(&mut self, frame: &Pixmap) -> Result<(), RenderError> {
        if frame.width() != self.width || frame.height() != self.height {
            return Err(RenderError::System("a frame came out the wrong size".into()));
        }
        let length = self.width * self.height * 4;
        let (at, next) = (self.time(self.written), self.time(self.written + 1));
        // SAFETY: the buffer is locked while it is filled, and filled with exactly its length.
        unsafe {
            let buffer = MFCreateMemoryBuffer(length).map_err(system)?;
            let mut data = std::ptr::null_mut();
            buffer.Lock(&mut data, None, None).map_err(system)?;
            let out = std::slice::from_raw_parts_mut(data, length as usize);
            for (o, i) in out.as_chunks_mut::<4>().0.iter_mut().zip(frame.data().as_chunks::<4>().0) {
                *o = [i[2], i[1], i[0], 255];
            }
            buffer.Unlock().map_err(system)?;
            buffer.SetCurrentLength(length).map_err(system)?;
            let sample = MFCreateSample().map_err(system)?;
            sample.AddBuffer(&buffer).map_err(system)?;
            sample.SetSampleTime(at).map_err(system)?;
            sample.SetSampleDuration(next - at).map_err(system)?;
            self.writer
                .WriteSample(self.stream, &sample)
                .map_err(|e| RenderError::System(format!("a frame couldn't be encoded: {}", e.message())))?;
        }
        self.written += 1;
        Ok(())
    }

    /// Finishes the file. Dropping the writer without this leaves an unplayable file to delete.
    pub fn finish(self) -> Result<(), RenderError> {
        // SAFETY: finishes the file; the writer isn't used again.
        unsafe { self.writer.Finalize() }
            .map_err(|e| RenderError::System(format!("the video couldn't be finished: {}", e.message())))
    }
}
