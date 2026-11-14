//! The exported video's sound. Every recorded track is cut and sped up like the picture, then mixed. AVFoundation
//! does the work: a composition lays each track out on the export's timeline (keeping the pitch through speed
//! changes), and an audio mix sets the levels and fades each cut edge so it never clicks.

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_av_foundation::{
    AVAssetReader, AVAssetReaderAudioMixOutput, AVAssetReaderStatus, AVAssetTrack, AVAudioMixInputParameters,
    AVAudioTimePitchAlgorithmSpectral, AVMediaTypeAudio, AVMutableAudioMix, AVMutableAudioMixInputParameters,
    AVMutableComposition, AVURLAsset,
};
use objc2_core_media::{CMSampleBuffer, CMTime, CMTimeRange};
use objc2_foundation::{NSArray, NSDictionary, NSNumber, NSString, NSURL, ns_string};

use super::decode::system;
use crate::RenderError;
use crate::model::RenderProject;

/// How long each cut edge fades, in seconds.
const FADE: f64 = 0.008;
/// The sample rate of the mix and of the exported sound.
pub const RATE: u32 = 48_000;

fn time(seconds: f64) -> CMTime {
    // SAFETY: a pure conversion.
    unsafe { CMTime::with_seconds(seconds, RATE as i32) }
}

fn span(start: f64, duration: f64) -> CMTimeRange {
    CMTimeRange { start: time(start), duration: time(duration) }
}

/// Keeps peaks under full scale without the harshness of clipping: untouched below the knee, rounded off above.
fn soft_limit(x: f32) -> f32 {
    const KNEE: f32 = 0.9;
    let level = x.abs();
    if level <= KNEE { x } else { (KNEE + (1.0 - KNEE) * ((level - KNEE) / (1.0 - KNEE)).tanh()).copysign(x) }
}

pub struct Sound {
    reader: Retained<AVAssetReader>,
    output: Retained<AVAssetReaderAudioMixOutput>,
    _composition: Retained<AVMutableComposition>,
    /// Loudness applied after the mix. An audio mix stops at 100%, so tracks set louder are mixed relative to the
    /// loudest and the whole mix is raised by this much.
    gain: f32,
    pending: Option<Retained<CMSampleBuffer>>,
    done: bool,
}

impl Sound {
    /// The project's sound, ready to read in order; `None` when there's nothing to hear.
    pub fn open(project: &RenderProject) -> Result<Option<Self>, RenderError> {
        if project.sounds.is_empty() {
            return Ok(None);
        }
        let loudest = project.sounds.iter().map(|s| s.volume).fold(1.0, f64::max);
        // SAFETY: AVFoundation objects built and used on this thread only, with valid arguments.
        unsafe {
            let audio = AVMediaTypeAudio.ok_or_else(|| system("AVFoundation is missing AVMediaTypeAudio"))?;
            let composition = AVMutableComposition::composition();
            let mut tracks: Vec<Retained<AVAssetTrack>> = Vec::new();
            let mut inputs: Vec<Retained<AVAudioMixInputParameters>> = Vec::new();
            for sound in &project.sounds {
                let Some(url) = NSURL::from_file_path(&sound.path) else { continue };
                let asset = AVURLAsset::URLAssetWithURL_options(&url, None);
                #[allow(deprecated)]
                let Some(source) = asset.tracksWithMediaType(audio).firstObject() else { continue };
                let range = source.timeRange();
                let from = range.start.seconds();
                let to = from + range.duration.seconds();
                if !from.is_finite() || !to.is_finite() || to <= from {
                    continue;
                }
                let Some(track) = composition.addMutableTrackWithMediaType_preferredTrackID(audio, 0) else {
                    continue;
                };
                let level = (sound.volume / loudest) as f32;
                let params = AVMutableAudioMixInputParameters::audioMixInputParametersWithTrack(Some(&track));
                params.setVolume_atTime(level, time(0.0));
                // `out` is where the clip starts in the export; `end` is where this track's sound ends so far.
                let (mut out, mut end) = (0.0, 0.0);
                for clip in &project.clips {
                    // The clip in the sound file's own time, and the part of it the file covers.
                    let (s0, s1) = (clip.start - sound.offset, clip.end - sound.offset);
                    let (f0, f1) = (s0.max(from), s1.min(to));
                    if f1 - f0 > 0.002 {
                        let at = out + (f0 - s0) / clip.speed;
                        if at > end + 1e-6 {
                            track.insertEmptyTimeRange(span(end, at - end));
                        }
                        track
                            .insertTimeRange_ofTrack_atTime_error(span(f0, f1 - f0), &source, time(at))
                            .map_err(|e| {
                                system(format!("couldn't lay out the sound: {}", e.localizedDescription()))
                            })?;
                        let length = (f1 - f0) / clip.speed;
                        if (clip.speed - 1.0).abs() > 1e-3 {
                            track.scaleTimeRange_toDuration(span(at, f1 - f0), time(length));
                        }
                        if length > 4.0 * FADE {
                            params.setVolumeRampFromStartVolume_toEndVolume_timeRange(
                                0.0,
                                level,
                                span(at, FADE),
                            );
                            params.setVolumeRampFromStartVolume_toEndVolume_timeRange(
                                level,
                                0.0,
                                span(at + length - FADE, FADE),
                            );
                        }
                        end = at + length;
                    }
                    out += (clip.end - clip.start) / clip.speed;
                }
                if end <= 0.0 {
                    composition.removeTrack(&track);
                    continue;
                }
                tracks.push(Retained::into_super(Retained::into_super(track)));
                inputs.push(Retained::into_super(params));
            }
            if tracks.is_empty() {
                return Ok(None);
            }

            let mix = AVMutableAudioMix::audioMix();
            mix.setInputParameters(&NSArray::from_retained_slice(&inputs));
            let reader = AVAssetReader::assetReaderWithAsset_error(&composition)
                .map_err(|e| system(format!("couldn't read the sound: {}", e.localizedDescription())))?;
            // Interleaved 32-bit float stereo: what the mix produces natively and the encoder takes.
            let lpcm = NSNumber::new_u32(u32::from_be_bytes(*b"lpcm"));
            let rate = NSNumber::new_f64(f64::from(RATE));
            let two = NSNumber::new_u32(2);
            let bits = NSNumber::new_u32(32);
            let (yes, no) = (NSNumber::new_bool(true), NSNumber::new_bool(false));
            let settings = NSDictionary::<NSString, AnyObject>::from_slices(
                &[
                    ns_string!("AVFormatIDKey"),
                    ns_string!("AVSampleRateKey"),
                    ns_string!("AVNumberOfChannelsKey"),
                    ns_string!("AVLinearPCMBitDepthKey"),
                    ns_string!("AVLinearPCMIsFloatKey"),
                    ns_string!("AVLinearPCMIsNonInterleaved"),
                    ns_string!("AVLinearPCMIsBigEndianKey"),
                ],
                &[
                    lpcm.as_ref(),
                    rate.as_ref(),
                    two.as_ref(),
                    bits.as_ref(),
                    yes.as_ref(),
                    no.as_ref(),
                    no.as_ref(),
                ],
            );
            let output = AVAssetReaderAudioMixOutput::assetReaderAudioMixOutputWithAudioTracks_audioSettings(
                &NSArray::from_retained_slice(&tracks),
                Some(&settings),
            );
            output.setAudioMix(Some(&mix));
            // Sped-up parts keep their pitch.
            if let Some(spectral) = AVAudioTimePitchAlgorithmSpectral {
                output.setAudioTimePitchAlgorithm(spectral);
            }
            if !reader.canAddOutput(&output) {
                return Err(system("the sound can't be mixed"));
            }
            reader.addOutput(&output);
            if !reader.startReading() {
                let reason = reader.error().map(|e| e.localizedDescription().to_string()).unwrap_or_default();
                return Err(system(format!("couldn't start mixing the sound: {reason}")));
            }
            Ok(Some(Self {
                reader,
                output,
                _composition: composition,
                gain: loudest as f32,
                pending: None,
                done: false,
            }))
        }
    }

    /// When the next buffer of sound starts, in seconds of the export; `None` once it's all been taken.
    pub fn peek(&mut self) -> Result<Option<f64>, RenderError> {
        while self.pending.is_none() && !self.done {
            // SAFETY: the reader is reading and the output belongs to it.
            match unsafe { self.output.copyNextSampleBuffer() } {
                // SAFETY: the buffer was just made for this code.
                Some(sample) if unsafe { sample.num_samples() } > 0 => {
                    self.level(&sample);
                    self.pending = Some(sample);
                }
                Some(_) => {}
                None => {
                    self.done = true;
                    // SAFETY: reading the reader's state after it stopped handing out samples.
                    if unsafe { self.reader.status() } == AVAssetReaderStatus::Failed {
                        let reason = unsafe { self.reader.error() }
                            .map(|e| e.localizedDescription().to_string())
                            .unwrap_or_default();
                        return Err(system(format!("the sound couldn't be mixed: {reason}")));
                    }
                }
            }
        }
        // SAFETY: reading a buffer's time is a pure operation.
        Ok(self.pending.as_ref().map(|s| unsafe { s.presentation_time_stamp().seconds() }))
    }

    /// The buffer `peek` looked at.
    pub fn take(&mut self) -> Option<Retained<CMSampleBuffer>> {
        self.pending.take()
    }

    /// Applies the overall gain and keeps peaks under full scale, in place.
    fn level(&self, sample: &CMSampleBuffer) {
        // SAFETY: the buffer was copied out for this code alone (the output copies by default), so its memory can
        // be changed before it goes to the encoder. Only a contiguous block of whole floats is touched.
        unsafe {
            let Some(block) = sample.data_buffer() else { return };
            let (mut contiguous, mut total) = (0usize, 0usize);
            let mut data: *mut std::ffi::c_char = std::ptr::null_mut();
            if block.data_pointer(0, &mut contiguous, &mut total, &mut data) != 0
                || data.is_null()
                || contiguous != total
                || !data.cast::<f32>().is_aligned()
            {
                return;
            }
            let floats = std::slice::from_raw_parts_mut(data.cast::<f32>(), total / 4);
            for x in floats {
                *x = soft_limit(*x * self.gain);
            }
        }
    }
}

impl Drop for Sound {
    fn drop(&mut self) {
        // SAFETY: cancelling is allowed in any state.
        unsafe { self.reader.cancelReading() };
    }
}

#[cfg(test)]
mod tests {
    use super::soft_limit;

    #[test]
    fn the_limiter_leaves_normal_levels_alone_and_rounds_off_peaks() {
        assert_eq!(soft_limit(0.5), 0.5);
        assert_eq!(soft_limit(-0.9), -0.9);
        // Far above full scale it settles at full scale, never beyond.
        assert!(soft_limit(3.0) <= 1.0 && soft_limit(3.0) > 0.99);
        assert!(soft_limit(-3.0) >= -1.0);
        assert!(soft_limit(0.95) > 0.9 && soft_limit(0.95) < 0.95);
    }
}
