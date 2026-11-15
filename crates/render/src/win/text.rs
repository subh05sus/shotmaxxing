//! Titles, captions and keystrokes drawn with DirectWrite in Segoe UI, the editor's font on Windows, through
//! Direct2D into a bitmap in memory.

use tiny_skia::{Color, IntSize, Pixmap};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_PIXEL_FORMAT,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_FEATURE_LEVEL_DEFAULT,
    D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE_SOFTWARE, D2D1_RENDER_TARGET_USAGE_NONE,
    D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE, D2D1CreateFactory, ID2D1Factory,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_METRICS, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL,
    DWRITE_FONT_WEIGHT, DWRITE_FONT_WEIGHT_EXTRA_BOLD, DWRITE_FONT_WEIGHT_MEDIUM,
    DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_LINE_METRICS, DWRITE_TEXT_METRICS, DWRITE_WORD_WRAPPING_NO_WRAP,
    DWriteCreateFactory, IDWriteFactory,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Imaging::{
    CLSID_WICImagingFactory, GUID_WICPixelFormat32bppPBGRA, IWICImagingFactory, WICBitmapCacheOnLoad,
};
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
use windows::core::{BOOL, IUnknown, w};

use crate::sprites::Weight;

/// The editor's interface font on Windows.
const FAMILY: windows::core::PCWSTR = w!("Segoe UI");

/// The font's ascent and descent at `size` pixels, which CSS centres in the line box.
fn extent(write: &IDWriteFactory, size: f64) -> Option<(f64, f64)> {
    // SAFETY: DirectWrite calls with valid out-pointers; every object is owned.
    unsafe {
        let mut fonts = None;
        write.GetSystemFontCollection(&mut fonts, false).ok()?;
        let fonts = fonts?;
        let (mut index, mut exists) = (0u32, BOOL(0));
        fonts.FindFamilyName(FAMILY, &mut index, &mut exists).ok()?;
        if !exists.as_bool() {
            return None;
        }
        let font = fonts
            .GetFontFamily(index)
            .ok()?
            .GetFirstMatchingFont(
                DWRITE_FONT_WEIGHT_SEMI_BOLD,
                DWRITE_FONT_STRETCH_NORMAL,
                DWRITE_FONT_STYLE_NORMAL,
            )
            .ok()?;
        let mut metrics = DWRITE_FONT_METRICS::default();
        font.GetMetrics(&mut metrics);
        let em = f64::from(metrics.designUnitsPerEm.max(1));
        Some((f64::from(metrics.ascent) / em * size, f64::from(metrics.descent) / em * size))
    }
}

/// DirectWrite's weight for the editor's: medium (`font-medium`), semibold and extra bold (`font-extrabold`).
fn weight_of(weight: Weight) -> DWRITE_FONT_WEIGHT {
    match weight {
        Weight::Medium => DWRITE_FONT_WEIGHT_MEDIUM,
        Weight::Semibold => DWRITE_FONT_WEIGHT_SEMI_BOLD,
        Weight::Heavy => DWRITE_FONT_WEIGHT_EXTRA_BOLD,
    }
}

/// `text` in Segoe UI at `size` pixels and `weight`, coloured `color`, on a transparent background. The box is
/// `line_height` × `size` tall with the glyphs centred in it, like a CSS line box.
pub fn draw(text: &str, size: f64, line_height: f64, color: Color, weight: Weight) -> Option<Pixmap> {
    // SAFETY: DirectWrite, Direct2D and WIC calls with valid arguments. The bitmap outlives the render target that
    // draws into it, and its pixels are copied into a buffer of exactly its size.
    unsafe {
        let write: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED).ok()?;
        let format = write
            .CreateTextFormat(
                FAMILY,
                None,
                weight_of(weight),
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                size as f32,
                w!("en-us"),
            )
            .ok()?;
        format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP).ok()?;
        let wide: Vec<u16> = text.encode_utf16().collect();
        let layout = write.CreateTextLayout(&wide, &format, 1e6, 1e6).ok()?;
        let mut metrics = DWRITE_TEXT_METRICS::default();
        layout.GetMetrics(&mut metrics).ok()?;
        let mut lines = [DWRITE_LINE_METRICS::default()];
        let mut count = 0;
        layout.GetLineMetrics(Some(&mut lines), &mut count).ok()?;

        let (ascent, descent) = extent(&write, size)?;
        let line_box = (size * line_height).round().max(1.0);
        let (w, h) = ((metrics.widthIncludingTrailingWhitespace.ceil() as u32).max(1) + 2, line_box as u32);
        // Where the baseline sits in a CSS line box, and where the layout's own first line puts it.
        let baseline = (line_box - (ascent + descent)) / 2.0 + ascent;
        let top = baseline - f64::from(lines[0].baseline);

        let imaging: IWICImagingFactory =
            CoCreateInstance(&CLSID_WICImagingFactory, None::<&IUnknown>, CLSCTX_INPROC_SERVER).ok()?;
        let bitmap = imaging.CreateBitmap(w, h, &GUID_WICPixelFormat32bppPBGRA, WICBitmapCacheOnLoad).ok()?;
        let d2d: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None).ok()?;
        let properties = D2D1_RENDER_TARGET_PROPERTIES {
            r#type: D2D1_RENDER_TARGET_TYPE_SOFTWARE,
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
            },
            dpiX: 96.0,
            dpiY: 96.0,
            usage: D2D1_RENDER_TARGET_USAGE_NONE,
            minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
        };
        let target = d2d.CreateWicBitmapRenderTarget(&bitmap, &properties).ok()?;
        // ClearType would tint the edges, and the label is composited later anyway.
        target.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
        let ink = D2D1_COLOR_F { r: color.red(), g: color.green(), b: color.blue(), a: color.alpha() };
        let brush = target.CreateSolidColorBrush(&ink, None).ok()?;
        target.BeginDraw();
        target.Clear(Some(&D2D1_COLOR_F::default()));
        let origin = windows_numerics::Vector2 { X: 1.0, Y: top as f32 };
        target.DrawTextLayout(origin, &layout, &brush, D2D1_DRAW_TEXT_OPTIONS_NONE);
        target.EndDraw(None, None).ok()?;
        drop(target);

        let mut data = vec![0u8; (w * h * 4) as usize];
        bitmap.CopyPixels(std::ptr::null(), w * 4, &mut data).ok()?;
        // Both are premultiplied; only the order of the colours differs.
        for px in data.as_chunks_mut::<4>().0 {
            px.swap(0, 2);
        }
        Pixmap::from_vec(data, IntSize::from_wh(w, h)?)
    }
}
