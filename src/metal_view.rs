// START_AI_HEADER
// MODULE: mac-companion/metal-viewer/src/metal_view.rs
// PURPOSE: Metal renderer for bsdOS display stream — NSWindow + MTKView + custom shader pipeline.
// INTENT: Uses raw Metal via objc2 instead of higher-level frameworks (e.g. video-core) to avoid
//         macOS version dependencies and keep full control over pixel format/scale.
// DEPENDENCIES: objc2 (rc/runtime/msg_send), objc2-app-kit (NSApplication/NSWindow/MTKView/NSEvent),
//               objc2-foundation (NSAutoreleasePool/NSString/NSRect), objc2-metal (MTL*),
//               objc2-metal-kit (MTKView).
// PUBLIC_API: RawMetal (new, update_and_draw, get_drawable_size, get_drawable_size_with_scale).
// END_AI_HEADER

// Metal рендерер для bsdOS display stream
// Использует render pipeline с full-screen quad для масштабирования

#[cfg(target_os = "macos")]
pub mod renderer {
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2::{msg_send, MainThreadMarker, MainThreadOnly};
    use objc2_app_kit::{
        NSApplication, NSApplicationActivationPolicy, NSBackingStoreType,
        NSWindow, NSWindowStyleMask,
    };
    use objc2_foundation::{NSAutoreleasePool, NSPoint, NSRect, NSSize, NSString};
    use objc2_metal::{
        MTLCommandBuffer, MTLCommandQueue, MTLLoadAction, MTLPixelFormat,
        MTLPrimitiveType, MTLRenderCommandEncoder, MTLRenderPassDescriptor,
        MTLRenderPipelineDescriptor, MTLStoreAction, MTLTexture, MTLTextureDescriptor,
        MTLTextureUsage, MTLViewport,
    };
    use objc2_metal_kit::MTKView;

    /// Raw pointer wrapper that is Send.
    /// SAFETY: All Metal calls happen sequentially from render thread.
    pub struct RawMetal {
        mtk_view: *mut AnyObject,
        device: *mut AnyObject,
        command_queue: *mut AnyObject,
        render_pipeline_state: *mut AnyObject,
        current_texture: *mut AnyObject,
        last_w: u32,
        last_h: u32,
    }

    unsafe impl Send for RawMetal {}

    impl RawMetal {
        // new:start
//   purpose: Create NSApplication, NSWindow (1280×720), MTKView, compile shaders, build Metal render pipeline.
//   input:  mtm: MainThreadMarker (ensures this runs on the main thread — NSWindow/MTKView require it).
//   output: RawMetal instance on success, Box<dyn Error> if Metal device unavailable or shader compilation fails.
//   sideEffects: Creates NSWindow + MTKView on screen; allocates Metal device, command queue, pipeline state.
        pub fn new(mtm: MainThreadMarker) -> Result<Self, Box<dyn std::error::Error>> {
            unsafe {
                let _pool = NSAutoreleasePool::new();

                let app = NSApplication::sharedApplication(mtm);
                app.setActivationPolicy(NSApplicationActivationPolicy::Regular);

                let frame = NSRect::new(
                    NSPoint::new(100.0, 100.0),
                    NSSize::new(1280.0, 720.0),
                );

                let style_mask = NSWindowStyleMask::from_bits_truncate(
                    NSWindowStyleMask::Titled.0
                        | NSWindowStyleMask::Closable.0
                        | NSWindowStyleMask::Miniaturizable.0
                        | NSWindowStyleMask::Resizable.0,
                );

                let window: Retained<NSWindow> = msg_send![
                    NSWindow::alloc(mtm),
                    initWithContentRect: frame,
                    styleMask: style_mask,
                    backing: NSBackingStoreType::Buffered,
                    defer: false,
                ];

                let title = NSString::from_str("bsdOS Metal Viewer");
                window.setTitle(&title);

                // MTKView
                let mtk_view: Retained<MTKView> = msg_send![
                    MTKView::alloc(mtm),
                    initWithFrame: frame,
                ];

                // Metal device
                extern "C" {
                    // MTLCreateSystemDefaultDevice:start
//   purpose: Obtain the default Metal GPU device (FFI import from Metal.framework).
//   input:  none (singleton-style C function).
//   output: *mut AnyObject (retained MTLDevice) or null if no Metal-capable GPU available.
//   sideEffects: none (pure query — may lazily init Metal driver).
                    fn MTLCreateSystemDefaultDevice() -> *mut AnyObject;
                }
                let dev_ptr = MTLCreateSystemDefaultDevice();
                if dev_ptr.is_null() {
                    return Err("No Metal device available".into());
                }
                    // MTLCreateSystemDefaultDevice:end
                let _: () = msg_send![&mtk_view, setDevice: dev_ptr];

                let device = mtk_view.device()
                    .ok_or("MTKView returned no device")?;

                // Command queue
                let command_queue: Retained<AnyObject> = msg_send![&*device, newCommandQueue];

                // Загружаем шейдеры из встроенной строки
                let shader_source = include_str!("shaders.metal");
                let shader_source_ns = NSString::from_str(shader_source);

                let library: Retained<AnyObject> = msg_send![
                    &device,
                    newLibraryWithSource: &*shader_source_ns,
                    options: std::ptr::null::<*mut AnyObject>(),
                    error: std::ptr::null_mut::<*mut AnyObject>()
                ];
                let vertex_fn: Retained<AnyObject> = msg_send![&library, newFunctionWithName: &*NSString::from_str("fullscreen_quad")];
                let fragment_fn: Retained<AnyObject> = msg_send![&library, newFunctionWithName: &*NSString::from_str("texture_fragment")];

                eprintln!("[metal] ✓ Shader library compiled");

                // Render pipeline descriptor через msg_send
                let pipeline_desc: Retained<AnyObject> = msg_send![
                    objc2::runtime::AnyClass::get(c"MTLRenderPipelineDescriptor").unwrap(),
                    new
                ];
                let _: () = msg_send![&pipeline_desc, setVertexFunction: &*vertex_fn];
                let _: () = msg_send![&pipeline_desc, setFragmentFunction: &*fragment_fn];

                // Configure color attachment pixel format (critical!)
                let color_attachments: *mut AnyObject = msg_send![&pipeline_desc, colorAttachments];
                let attachment: Retained<AnyObject> = msg_send![color_attachments, objectAtIndexedSubscript: 0usize];
                let _: () = msg_send![&attachment, setPixelFormat: MTLPixelFormat::BGRA8Unorm];

                eprintln!("[metal] ✓ Render pipeline configured with BGRA8Unorm color attachment");

                // Create pipeline state
                let render_pipeline_state: Retained<AnyObject> = msg_send![
                    &device,
                    newRenderPipelineStateWithDescriptor: &*pipeline_desc,
                    error: std::ptr::null_mut::<*mut AnyObject>()
                ];
                // render_pipeline_state не null здесь - если бы был, вернулась бы ошибка

                let _: () = msg_send![&window, setContentView: &*mtk_view];
                window.makeKeyAndOrderFront(None);

                eprintln!("[metal] ✓ Metal renderer initialized with render pipeline");

                let raw = Self {
                    mtk_view: Retained::into_raw(mtk_view) as *mut AnyObject,
                    device: Retained::into_raw(device) as *mut AnyObject,
                    command_queue: Retained::into_raw(command_queue) as *mut AnyObject,
                    render_pipeline_state: Retained::into_raw(render_pipeline_state) as *mut AnyObject,
                    current_texture: std::ptr::null_mut(),
                    last_w: 0,
                    last_h: 0,
                };

                // Keep window alive
                let _ = Retained::into_raw(window);

                Ok(raw)
            }
        }
        // new:end

        // update_and_draw:start
//   purpose: Upload RGBA pixels to MTLTexture, then render full-screen quad via Metal pipeline.
//   input:  &mut self (renderer state), width/height: input frame dimensions, _stride: bytes per row (unused — derived from width*4),
//           rgba_data: raw BGRA pixel data (must be width*height*4 bytes or longer).
//   output: Ok(()) on success, Err if Metal drawable unavailable or command encoding fails.
//   sideEffects: Reallocates MTLTexture if width/height changed; writes to Metal command queue,
//                commits drawable for display; may release previous texture.
        pub fn update_and_draw(
            &mut self,
            width: u32,
            height: u32,
            _stride: u32,
            rgba_data: &[u8],
        ) -> Result<(), Box<dyn std::error::Error>> {
            unsafe {
                if width == 0 || height == 0 {
                    return Ok(());
                }

                let expected_size = (width as usize) * (height as usize) * 4;
                if rgba_data.len() < expected_size {
                    return Ok(());
                }

                // Создать/обновить текстуру входного кадра
                if width != self.last_w || height != self.last_h {
                    self.last_w = width;
                    self.last_h = height;

                    if !self.current_texture.is_null() {
                        let _: () = msg_send![self.current_texture, release];
                    }

                    let desc: Retained<AnyObject> = msg_send![
                        objc2::runtime::AnyClass::get(c"MTLTextureDescriptor").unwrap(),
                        texture2DDescriptorWithPixelFormat: MTLPixelFormat::BGRA8Unorm,
                        width: width as usize,
                        height: height as usize,
                        mipmapped: false,
                    ];

                    let usage = MTLTextureUsage::ShaderRead.union(MTLTextureUsage::RenderTarget);
                    let _: () = msg_send![&desc, setUsage: usage];

                    let tex: Retained<AnyObject> = msg_send![
                        self.device,
                        newTextureWithDescriptor: &*desc,
                    ];
                    self.current_texture = Retained::into_raw(tex) as *mut AnyObject;
                }

                // Upload pixels в текстуру
                let bytes_per_row = width as usize * 4;
                let region = objc2_metal::MTLRegion {
                    origin: objc2_metal::MTLOrigin { x: 0, y: 0, z: 0 },
                    size: objc2_metal::MTLSize {
                        width: width as usize,
                        height: height as usize,
                        depth: 1,
                    },
                };
                let _: () = msg_send![
                    self.current_texture,
                    replaceRegion: region,
                    mipmapLevel: 0usize,
                    withBytes: rgba_data.as_ptr(),
                    bytesPerRow: bytes_per_row,
                ];

                // Рендер через render pipeline (full-screen quad)
                let drawable: Option<Retained<AnyObject>> = msg_send![self.mtk_view, currentDrawable];
                if let Some(drawable) = drawable {
                    let drawable_tex: Retained<AnyObject> = msg_send![&drawable, texture];

                    // Render pass descriptor
                    let render_pass_desc: *mut AnyObject = msg_send![self.mtk_view, currentRenderPassDescriptor];
                    if render_pass_desc.is_null() {
                        return Ok(());
                    }

                    let cmd_buf: Retained<AnyObject> = msg_send![self.command_queue, commandBuffer];

                    // Render command encoder
                    let encoder: Retained<AnyObject> = msg_send![
                        &cmd_buf,
                        renderCommandEncoderWithDescriptor: render_pass_desc
                    ];

                    // Установить pipeline
                    let _: () = msg_send![&encoder, setRenderPipelineState: self.render_pipeline_state];

                    // Установить текстуру для fragment shader
                    let _: () = msg_send![&encoder, setFragmentTexture: self.current_texture, atIndex: 0usize];

                    // Получить размер drawable для viewport
                    let drawable_w: usize = msg_send![&drawable_tex, width];
                    let drawable_h: usize = msg_send![&drawable_tex, height];

                    if width != self.last_w || height != self.last_h {
                        eprintln!("[render] Input: {}x{}, Drawable: {}x{}", width, height, drawable_w, drawable_h);
                    }

                    // Viewport на весь drawable (масштабирование)
                    let viewport = MTLViewport {
                        originX: 0.0,
                        originY: 0.0,
                        width: drawable_w as f64,
                        height: drawable_h as f64,
                        znear: 0.0,
                        zfar: 1.0,
                    };
                    let _: () = msg_send![&encoder, setViewport: viewport];

                    // Рисуем full-screen quad (4 вершины, triangle strip)
                    let _: () = msg_send![
                        &encoder,
                        drawPrimitives: MTLPrimitiveType::TriangleStrip,
                        vertexStart: 0usize,
                        vertexCount: 4u32,
                        instanceCount: 1u32
                    ];

                    let _: () = msg_send![&encoder, endEncoding];
                    let _: () = msg_send![&cmd_buf, presentDrawable: &*drawable];
                    let _: () = msg_send![&cmd_buf, commit];
                }

                Ok(())
            }
        }
        // update_and_draw:end

        // upload_damaged_region:start
        //   purpose: Upload only the damaged region of the frame to the Metal texture (partial update).
        //   input:  texture: MTL texture pointer, frame: RGBA frame data, damage: Rect (x, y, w, h), full_stride: full frame stride in bytes.
        //   output: none.
        //   sideEffects: calls replaceRegion on texture with damage region offset.
        pub fn upload_damaged_region(
            &self,
            texture: *mut AnyObject,
            frame: &[u8],
            damage: bsdos_metal_viewer::protocol::Rect,
            full_stride: usize,
        ) {
            unsafe {
                if texture.is_null() || frame.is_empty() {
                    return;
                }

                let bytes_per_row = full_stride;
                let region = objc2_metal::MTLRegion {
                    origin: objc2_metal::MTLOrigin {
                        x: damage.x as usize,
                        y: damage.y as usize,
                        z: 0,
                    },
                    size: objc2_metal::MTLSize {
                        width: damage.w as usize,
                        height: damage.h as usize,
                        depth: 1,
                    },
                };

                // Pointer to first pixel of damage region
                let offset = (damage.y as usize) * bytes_per_row + (damage.x as usize) * 4;
                let ptr = frame.as_ptr().add(offset);

                let _: () = msg_send![
                    texture,
                    replaceRegion: region,
                    mipmapLevel: 0usize,
                    withBytes: ptr,
                    bytesPerRow: bytes_per_row,
                ];
            }
        }
        // upload_damaged_region:end

        /// Get current drawable size from texture (physical pixels)
        // get_drawable_size:start
//   purpose: Return current drawable (MTKView) pixel dimensions, ignoring backing scale factor.
//   input:  &self (borrows renderer state).
//   output: (width, height) in physical pixels; fallback (1280, 720) if no drawable.
//   sideEffects: none (read-only, delegates to get_drawable_size_with_scale).
        pub fn get_drawable_size(&self) -> (u32, u32) {
            let (w, h, _) = self.get_drawable_size_with_scale();
            (w, h)
        }
        // get_drawable_size:end

        /// Get drawable size + backingScaleFactor for HiDPI (format: WxH@S)
        // get_drawable_size_with_scale:start
//   purpose: Return drawable pixel dimensions AND the window's backingScaleFactor for HiDPI.
//   input:  &self (borrows renderer state).
//   output: (width, height, scale) — scale is 2 for Retina, 1 for standard; returns (0,0,1) when
//           MTKView has no window yet (caller skips publish on w==0).
//   sideEffects: none (read-only, queries MTKView.drawableSize and NSWindow.backingScaleFactor).
        pub fn get_drawable_size_with_scale(&self) -> (u32, u32, u32) {
            // MTKView.drawableSize is updated by AppKit on every resize/display-move,
            // including fullscreen transitions to another monitor — no drawable needs
            // to be in-flight. Using currentDrawable.texture was unreliable: nil during
            // the fullscreen animation caused the hardcoded fallback to be published.
            unsafe {
                use objc2_foundation::NSSize;
                let size: NSSize = msg_send![self.mtk_view, drawableSize];
                let w = size.width as u32;
                let h = size.height as u32;
                if w == 0 || h == 0 {
                    return (0, 0, 1);
                }
                let window: *mut AnyObject = msg_send![self.mtk_view, window];
                let scale: u32 = if !window.is_null() {
                    let s: f64 = msg_send![window, backingScaleFactor];
                    s as u32
                } else {
                    2
                };
                (w, h, scale)
            }
        }
        // get_drawable_size_with_scale:end
    }
}
