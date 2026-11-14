//! Labels drawn with Core Text in the system font, so titles and keystrokes look like the editor's preview.

use std::ffi::c_void;
use std::ptr::NonNull;

use objc2_core_foundation::{
    CFAttributedString, CFBoolean, CFDictionary, CFNumber, CFRetained, CFString, CFType, CGFloat,
};
use objc2_core_graphics::{CGColorSpace, CGContext, kCGColorSpaceSRGB};
use objc2_core_text::{
    CTFont, CTFontDescriptor, CTFontUIFontType, CTLine, kCTFontAttributeName,
    kCTForegroundColorFromContextAttributeName, kCTFontTraitsAttribute, kCTFontWeightTrait,
};
use tiny_skia::{Color, IntSize, Pixmap};

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C-unwind" {
    fn CGBitmapContextCreate(
        data: *mut c_void,
        width: usize,
        height: usize,
        bits_per_component: usize,
        bytes_per_row: usize,
        space: Option<&CGColorSpace>,
        bitmap_info: u32,
    ) -> Option<NonNull<CGContext>>;
}

/// Premultiplied RGBA, red first in memory.
const PREMULTIPLIED_LAST: u32 = 1;
/// Semibold, as `font-semibold` in the editor.
const SEMIBOLD: f64 = 0.3;

fn semibold_system_font(size: f64) -> Option<CFRetained<CTFont>> {
    // SAFETY: Core Text creates and copies fonts from valid descriptors; every object here is owned.
    unsafe {
        let base = CTFont::new_ui_font_for_language(CTFontUIFontType::System, size, None)?;
        let weight = CFNumber::new_f64(SEMIBOLD);
        let traits = CFDictionary::<CFString, CFType>::from_slices(&[kCTFontWeightTrait], &[weight.as_ref()]);
        let attributes =
            CFDictionary::<CFString, CFType>::from_slices(&[kCTFontTraitsAttribute], &[traits.as_ref()]);
        let descriptor = CTFontDescriptor::with_attributes(attributes.as_opaque());
        Some(base.copy_with_attributes(size, std::ptr::null(), Some(&descriptor)))
    }
}

/// `text` in the semibold system font at `size` pixels, coloured `color`, on a transparent background. The box is
/// `line_height` × `size` tall with the glyphs centred in it, like a CSS line box.
pub fn draw(text: &str, size: f64, line_height: f64, color: Color) -> Option<Pixmap> {
    let font = semibold_system_font(size)?;
    // SAFETY: plain Core Text and Core Graphics calls. The bitmap context draws into `data`, which outlives it.
    unsafe {
        let string = CFString::from_str(text);
        let attributes = CFDictionary::<CFString, CFType>::from_slices(
            &[kCTFontAttributeName, kCTForegroundColorFromContextAttributeName],
            &[font.as_ref(), CFBoolean::new(true).as_ref()],
        );
        let attributed = CFAttributedString::new(None, Some(&string), Some(attributes.as_opaque()))?;
        let line = CTLine::with_attributed_string(&attributed);
        let (mut ascent, mut descent, mut leading): (CGFloat, CGFloat, CGFloat) = (0.0, 0.0, 0.0);
        let width = line.typographic_bounds(&mut ascent, &mut descent, &mut leading);
        let line_box = (size * line_height).round().max(1.0);
        let (w, h) = ((width.ceil() as u32).max(1) + 2, line_box as u32);
        let mut data = vec![0u8; (w * h * 4) as usize];
        let space = CGColorSpace::with_name(Some(kCGColorSpaceSRGB))?;
        let context = CGBitmapContextCreate(
            data.as_mut_ptr().cast(),
            w as usize,
            h as usize,
            8,
            w as usize * 4,
            Some(&space),
            PREMULTIPLIED_LAST,
        )?;
        let context = CFRetained::from_raw(context);
        CGContext::set_allows_font_smoothing(Some(&context), false);
        CGContext::set_should_smooth_fonts(Some(&context), false);
        CGContext::set_rgb_fill_color(
            Some(&context),
            f64::from(color.red()),
            f64::from(color.green()),
            f64::from(color.blue()),
            f64::from(color.alpha()),
        );
        // Core Graphics counts up from the bottom. CSS centres ascent plus descent in the line box.
        CGContext::set_text_position(Some(&context), 1.0, (line_box - (ascent + descent)) / 2.0 + descent);
        line.draw(&context);
        drop(context);
        Pixmap::from_vec(data, IntSize::from_wh(w, h)?)
    }
}
