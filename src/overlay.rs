// START_AI_HEADER
// MODULE: mac-companion/metal-viewer/src/overlay.rs
// PURPOSE: On-screen overlay rendering (FPS + ping) via Core Graphics into an RGBA buffer.
// INTENT: Uses Core Graphics (CGContext) instead of Metal for overlay to avoid recompiling the Metal
//         pipeline on every frame; the overlay is alpha-blended onto pixel data before Metal texture upload.
// DEPENDENCIES: objc2-core-foundation (CFRetained), objc2-core-graphics (CGBitmapContextCreate, CGContext*, CGColorSpace).
// PUBLIC_API: OverlayRenderer (new, render), blend_overlay.
// END_AI_HEADER

// On-screen overlay: FPS + ping rendered via Core Graphics into RGBA buffer
// Blended onto the frame pixel data before Metal texture upload

#[cfg(target_os = "macos")]
pub mod overlay {
    use objc2_core_foundation::CFRetained;
    use objc2_core_graphics::{
        CGBitmapContextCreate, CGColorSpace, CGContext, CGContextSetRGBFillColor,
        CGContextShowTextAtPoint,
    };
    use std::ffi::c_void;

    // kCGImageAlphaPremultipliedLast = 2 (RGBA with premultiplied alpha)
    const K_CG_IMAGE_ALPHA_PREMULTIPLIED_LAST: u32 = 2;

    pub struct OverlayRenderer {
        width: usize,
        height: usize,
        buffer: Vec<u8>,
    }

    impl OverlayRenderer {
        // new:start
//   purpose: Create overlay renderer with fixed 220×36 internal RGBA buffer.
//   input:  none.
//   output: OverlayRenderer with zeroed 220×36 RGBA buffer.
//   sideEffects: Allocates 31.7 KB (220*36*4) heap buffer.
        pub fn new() -> Self {
            Self {
                width: 220,
                height: 36,
                buffer: vec![0u8; 220 * 36 * 4],
            }
        }
        // new:end

        /// Render status text into internal RGBA buffer.
        /// Returns (width, height, buffer_slice).
        // render:start
//   purpose: Render FPS and optional ping latency text into internal RGBA buffer using Core Graphics.
//   input:  fps: current frames-per-second value (formatted as "{:.0} fps"), ping_ms: optional latency in ms.
//   output: (width, height, buffer_slice) — 220×36 RGBA buffer with semi-transparent black background + white Menlo text.
//   sideEffects: Writes into internal buffer via CGBitmapContext using deprecated CGContextShowTextAtPoint.
        pub fn render(&mut self, fps: f64, ping_ms: Option<f64>) -> (usize, usize, &[u8]) {
            let text = match ping_ms {
                Some(p) => format!("{:.0} fps | {:.0} ms", fps, p),
                None => format!("{:.0} fps", fps),
            };

            // Fill semi-transparent black background
            for pixel in self.buffer.chunks_exact_mut(4) {
                pixel[0] = 0;   // R
                pixel[1] = 0;   // G
                pixel[2] = 0;   // B
                pixel[3] = 153; // A = 0.6 * 255
            }

            let color_space = match CGColorSpace::new_device_rgb() {
                Some(cs) => cs,
                None => return (self.width, self.height, &self.buffer),
            };

            unsafe {
                let ctx = match CGBitmapContextCreate(
                    self.buffer.as_mut_ptr() as *mut c_void,
                    self.width,
                    self.height,
                    8, // bits per component
                    self.width * 4, // bytes per row
                    Some(&color_space),
                    K_CG_IMAGE_ALPHA_PREMULTIPLIED_LAST,
                ) {
                    Some(ctx) => ctx,
                    None => return (self.width, self.height, &self.buffer),
                };

                // White text
                CGContextSetRGBFillColor(
                    Some(&ctx),
                    1.0,
                    1.0,
                    1.0,
                    1.0,
                );

                #[allow(deprecated)]
                {
                    use objc2_core_graphics::{CGContextSelectFont, CGTextEncoding};

                    CGContextSelectFont(
                        Some(&ctx),
                        b"Menlo\0".as_ptr() as *const i8,
                        14.0,
                        CGTextEncoding::EncodingMacRoman,
                    );

                    let text_bytes = text.as_bytes();
                    CGContextShowTextAtPoint(
                        Some(&ctx),
                        8.0,
                        24.0,
                        text_bytes.as_ptr() as *const i8,
                        text_bytes.len(),
                    );
                }

                // ctx is CFRetained<CGContext> — dropped here, calls CGContextRelease
            }

            (self.width, self.height, &self.buffer)
        }
        // render:end
    }

    /// Alpha-blend overlay onto the top-left corner of the main frame buffer.
    /// `pixels` is RGBA8 (4 bytes per pixel), width*height size.
    // blend_overlay:start
//   purpose: Alpha-blend overlay RGBA buffer onto top-left corner of the main frame pixel data.
//   input:  pixels: main frame RGBA8 buffer (mutated in-place), pw/_ph: main frame dimensions,
//           overlay: overlay RGBA8 buffer, ow/oh: overlay pixel dimensions.
//   output: none (pixels is mutated in-place).
//   sideEffects: Mutates pixels in-place with per-pixel alpha blending (SRC_OVER).
    pub fn blend_overlay(
        pixels: &mut [u8],
        pw: u32,
        _ph: u32,
        overlay: &[u8],
        ow: usize,
        oh: usize,
    ) {
        for y in 0..oh {
            for x in 0..ow {
                let si = (y * ow + x) * 4;
                if si + 3 >= overlay.len() {
                    break;
                }
                let a = overlay[si + 3] as f32 / 255.0;
                if a < 0.01 {
                    continue;
                }

                let dx = x as u32;
                let dy = y as u32;
                if dx >= pw {
                    break;
                }

                let di = ((dy * pw + dx) * 4) as usize;
                if di + 3 >= pixels.len() {
                    break;
                }

                let inv_a = 1.0 - a;
                pixels[di] = (overlay[si] as f32 * a + pixels[di] as f32 * inv_a) as u8;
                pixels[di + 1] = (overlay[si + 1] as f32 * a + pixels[di + 1] as f32 * inv_a) as u8;
                pixels[di + 2] = (overlay[si + 2] as f32 * a + pixels[di + 2] as f32 * inv_a) as u8;
                pixels[di + 3] = 255;
            }
        }
    }
    // blend_overlay:end
}
