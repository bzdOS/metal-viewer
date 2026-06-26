// START_AI_HEADER
// MODULE: mac-companion/metal-viewer/src/wayland_stream.rs
// PURPOSE: Wayland Stream Protocol v1 parser + compositor — cross-platform (no Metal/objc2).
// INTENT: Decodes v1 wire format events (SURFACE_CREATE/DESTROY, POOL_DATA, SURFACE_COMMIT, CURSOR_MOVE)
//         into StreamEvent enum and composites RGBA frames from pool data in Compositor.
// DEPENDENCIES: std (collections::HashMap), lz4_flex (decompress).
// PUBLIC_API: stream_parser (parse_events, is_v1_protocol, StreamEvent, EV_* constants),
//             compositor (Compositor::new, handle_*, pool_count, surface_count, FrameOutput).
// START_INVARIANTS
//   - stream_parser is pure: no IO, no allocations beyond returned Vec.
//   - parse_events returns Vec<Result<...>>: per-event errors, never panics.
//   - Compositor state is fully encapsulated; handle_surface_commit silently
//     drops events referencing unknown pool_ids (logged via eprintln).
//   - frame.dirty is set on every successful SURFACE_COMMIT; the Metal
//     renderer reads this flag to skip unchanged frames.
// END_INVARIANTS
// END_AI_HEADER

// Wayland Stream Protocol v1 parser + compositor
// See WAYLAND_STREAM_PROTOCOL.md for wire format
//
// Events: SURFACE_CREATE, SURFACE_DESTROY, POOL_DATA (LZ4), SURFACE_COMMIT, CURSOR_MOVE
// Compositor: tracks surfaces + pools, produces RGBA frame on SURFACE_COMMIT
//
// Both submodules are cross-platform (no Metal / objc2) so the lib can be
// unit-tested on Linux CI hosts. macOS-specific code lives in main.rs.

pub mod stream_parser {
    // Event type constants
    pub const EV_SURFACE_CREATE: u8 = 0x01;
    pub const EV_SURFACE_DESTROY: u8 = 0x02;
    pub const EV_POOL_DATA: u8 = 0x03;
    pub const EV_SURFACE_COMMIT: u8 = 0x04;
    pub const EV_CURSOR_MOVE: u8 = 0x05;

    #[derive(Debug)]
    pub enum StreamEvent<'a> {
        SurfaceCreate { surface_id: u32 },
        SurfaceDestroy { surface_id: u32 },
        PoolData {
            pool_id: u32,
            width: u16,
            height: u16,
            stride: u32,
            format: u32,
            raw_len: u32,
            lz4_data: &'a [u8],
        },
        SurfaceCommit {
            surface_id: u32,
            pool_id: u32,
            offset: u32,
            buf_width: u16,
            buf_height: u16,
            buf_stride: u32,
            format: u32,
            damage_x: u16,
            damage_y: u16,
            damage_w: u16,
            damage_h: u16,
        },
        CursorMove { x: i32, y: i32 },
    }

    /// Parse all events from a Zenoh payload.
    /// Format: [total_size: u32 LE][events...]
    /// Each event: [event_type: u8][payload...]
    // parse_events:start
    //   purpose: Parse all Wayland stream events from a Zenoh payload (v1 protocol).
    //   input:  data: raw bytes from Zenoh subscription (format: [total_size:u32 LE][events...]).
    //   output: Vec of parsed events (Ok) or parse errors (Err). Stops on first unknown event type.
    //   sideEffects: none (pure parsing).
    pub fn parse_events(data: &[u8]) -> Vec<Result<StreamEvent<'_>, String>> {
        let mut results = Vec::new();

        if data.len() < 4 {
            return results;
        }

        let total_size = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
        let end = (4 + total_size).min(data.len());
        let mut pos = 4;

        while pos < end {
            if pos >= data.len() {
                break;
            }
            let event_type = data[pos];
            pos += 1;

            let result = match event_type {
                EV_SURFACE_CREATE => parse_surface_create(&data[pos..end]),
                EV_SURFACE_DESTROY => parse_surface_destroy(&data[pos..end]),
                EV_POOL_DATA => parse_pool_data(&data[pos..end]),
                EV_SURFACE_COMMIT => parse_surface_commit(&data[pos..end]),
                EV_CURSOR_MOVE => parse_cursor_move(&data[pos..end]),
                _ => {
                    // Unknown event type — skip
                    // We don't know the size, so we can't continue parsing
                    results.push(Err(format!("Unknown event type 0x{:02x}", event_type)));
                    break;
                }
            };

            match result {
                Ok((event, consumed)) => {
                    pos += consumed;
                    results.push(Ok(event));
                }
                Err(e) => {
                    results.push(Err(e));
                    break;
                }
            }
        }

        results
    }
    // parse_events:end

    /// Check if data looks like v1 protocol (starts with reasonable total_size)
    // is_v1_protocol:start
    //   purpose: Heuristic check if data looks like v1 protocol (starts with reasonable total_size and valid event type).
    //   input:  data: raw bytes to check.
    //   output: true if data appears to be v1 protocol, false otherwise.
    //   sideEffects: none (pure check).
    pub fn is_v1_protocol(data: &[u8]) -> bool {
        if data.len() < 5 {
            return false;
        }
        let total_size = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
        // total_size should be roughly data.len() - 4, and first event type should be valid
        let first_event = data[4];
        total_size > 0
            && total_size <= data.len() - 4
            && first_event >= 0x01
            && first_event <= 0x05
    }
    // is_v1_protocol:end

    // Individual event parsers return (event, bytes_consumed)

    // parse_surface_create:start
    //   purpose: Parse SURFACE_CREATE event (4 bytes: surface_id).
    //   input:  data: bytes starting at event payload (after event_type byte).
    //   output: (StreamEvent::SurfaceCreate, bytes_consumed=4) or error if < 4 bytes.
    //   sideEffects: none (pure parsing).
    fn parse_surface_create(data: &[u8]) -> Result<(StreamEvent<'_>, usize), String> {
        if data.len() < 4 {
            return Err("SURFACE_CREATE: need 4 bytes".into());
        }
        let surface_id = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        Ok((StreamEvent::SurfaceCreate { surface_id }, 4))
    }
    // parse_surface_create:end

    // parse_surface_destroy:start
    //   purpose: Parse SURFACE_DESTROY event (4 bytes: surface_id).
    //   input:  data: bytes starting at event payload.
    //   output: (StreamEvent::SurfaceDestroy, bytes_consumed=4) or error if < 4 bytes.
    //   sideEffects: none (pure parsing).
    fn parse_surface_destroy(data: &[u8]) -> Result<(StreamEvent<'_>, usize), String> {
        if data.len() < 4 {
            return Err("SURFACE_DESTROY: need 4 bytes".into());
        }
        let surface_id = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        Ok((StreamEvent::SurfaceDestroy { surface_id }, 4))
    }
    // parse_surface_destroy:end

    // parse_pool_data:start
    //   purpose: Parse POOL_DATA event (24-byte header + lz4_data). Header: pool_id(4) + width(2) + height(2) + stride(4) + format(4) + raw_len(4) + lz4_len(4).
    //   input:  data: bytes starting at event payload.
    //   output: (StreamEvent::PoolData, bytes_consumed=24+lz4_len) or error if insufficient bytes.
    //   sideEffects: none (pure parsing).
    fn parse_pool_data(data: &[u8]) -> Result<(StreamEvent<'_>, usize), String> {
        // pool_id(4) + width(2) + height(2) + stride(4) + format(4) + raw_len(4) + lz4_len(4) = 24 header
        if data.len() < 24 {
            return Err(format!("POOL_DATA: need 24 bytes header, got {}", data.len()));
        }
        let pool_id = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        let width = u16::from_le_bytes([data[4], data[5]]);
        let height = u16::from_le_bytes([data[6], data[7]]);
        let stride = u32::from_le_bytes([data[8], data[9], data[10], data[11]]);
        let format = u32::from_le_bytes([data[12], data[13], data[14], data[15]]);
        let raw_len = u32::from_le_bytes([data[16], data[17], data[18], data[19]]);
        let lz4_len = u32::from_le_bytes([data[20], data[21], data[22], data[23]]) as usize;

        if data.len() < 24 + lz4_len {
            return Err(format!(
                "POOL_DATA: need {} bytes lz4 data, got {}",
                lz4_len,
                data.len() - 24
            ));
        }

        let lz4_data = &data[24..24 + lz4_len];
        Ok((
            StreamEvent::PoolData {
                pool_id, width, height, stride, format, raw_len, lz4_data,
            },
            24 + lz4_len,
        ))
    }
    // parse_pool_data:end

    // parse_surface_commit:start
    //   purpose: Parse SURFACE_COMMIT event (32 bytes): surface_id(4) + pool_id(4) + offset(4) + buf_width(2) + buf_height(2) + buf_stride(4) + format(4) + damage_x(2) + damage_y(2) + damage_w(2) + damage_h(2).
    //   input:  data: bytes starting at event payload.
    //   output: (StreamEvent::SurfaceCommit, bytes_consumed=32) or error if < 32 bytes.
    //   sideEffects: none (pure parsing).
    fn parse_surface_commit(data: &[u8]) -> Result<(StreamEvent<'_>, usize), String> {
        // surface_id(4) + pool_id(4) + offset(4) + buf_width(2) + buf_height(2)
        // + buf_stride(4) + format(4) + damage(8) = 32 bytes
        if data.len() < 32 {
            return Err(format!("SURFACE_COMMIT: need 32 bytes, got {}", data.len()));
        }
        let surface_id = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        let pool_id = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
        let offset = u32::from_le_bytes([data[8], data[9], data[10], data[11]]);
        let buf_width = u16::from_le_bytes([data[12], data[13]]);
        let buf_height = u16::from_le_bytes([data[14], data[15]]);
        let buf_stride = u32::from_le_bytes([data[16], data[17], data[18], data[19]]);
        let format = u32::from_le_bytes([data[20], data[21], data[22], data[23]]);
        let damage_x = u16::from_le_bytes([data[24], data[25]]);
        let damage_y = u16::from_le_bytes([data[26], data[27]]);
        let damage_w = u16::from_le_bytes([data[28], data[29]]);
        let damage_h = u16::from_le_bytes([data[30], data[31]]);

        Ok((
            StreamEvent::SurfaceCommit {
                surface_id, pool_id, offset, buf_width, buf_height,
                buf_stride, format, damage_x, damage_y, damage_w, damage_h,
            },
            32,
        ))
    }
    // parse_surface_commit:end

    // parse_cursor_move:start
    //   purpose: Parse CURSOR_MOVE event (8 bytes: x:i32 + y:i32).
    //   input:  data: bytes starting at event payload.
    //   output: (StreamEvent::CursorMove, bytes_consumed=8) or error if < 8 bytes.
    //   sideEffects: none (pure parsing).
    fn parse_cursor_move(data: &[u8]) -> Result<(StreamEvent<'_>, usize), String> {
        if data.len() < 8 {
            return Err("CURSOR_MOVE: need 8 bytes".into());
        }
        let x = i32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        let y = i32::from_le_bytes([data[4], data[5], data[6], data[7]]);
        Ok((StreamEvent::CursorMove { x, y }, 8))
    }
    // parse_cursor_move:end
}

pub mod compositor {
    use std::collections::HashMap;
    use crate::protocol::{Rect, clamp_damage_to_surface};

    /// Decompressed pool data stored by pool_id
    struct Pool {
        data: Vec<u8>,
        width: u16,
        height: u16,
        stride: u32,
        format: u32,
    }

    /// Surface state
    struct Surface {
        _active: bool,
        damage: Rect,
    }

    pub struct Compositor {
        pools: HashMap<u32, Pool>,
        surfaces: HashMap<u32, Surface>,
        /// Last composited frame (RGBA)
        pub frame: FrameOutput,
        pub dirty: bool,
        /// Last damage rect from SURFACE_COMMIT
        pub last_damage: Rect,
    }

    #[derive(Debug, Clone)]
    pub struct FrameOutput {
        pub width: u32,
        pub height: u32,
        pub stride: u32,
        pub data: Vec<u8>,
    }

    impl Compositor {
        // new:start
        //   purpose: Create empty compositor with no pools/surfaces and zero-size frame.
        //   input:  none.
        //   output: new Compositor instance.
        //   sideEffects: none (pure construction).
        pub fn new() -> Self {
            Self {
                pools: HashMap::new(),
                surfaces: HashMap::new(),
                frame: FrameOutput {
                    width: 0,
                    height: 0,
                    stride: 0,
                    data: Vec::new(),
                },
                dirty: false,
                last_damage: Rect::zero(),
            }
        }
        // new:end

        // handle_surface_create:start
        //   purpose: Register new surface in compositor state.
        //   input:  surface_id: Wayland surface identifier.
        //   output: void.
        //   sideEffects: inserts surface into surfaces HashMap, logs creation.
        pub fn handle_surface_create(&mut self, surface_id: u32) {
            self.surfaces.insert(surface_id, Surface { _active: true, damage: Rect::zero() });
            eprintln!("[compositor] Surface {} created", surface_id);
        }
        // handle_surface_create:end

        // handle_surface_destroy:start
        //   purpose: Remove surface from compositor state.
        //   input:  surface_id: Wayland surface identifier.
        //   output: void.
        //   sideEffects: removes surface from surfaces HashMap, logs destruction.
        pub fn handle_surface_destroy(&mut self, surface_id: u32) {
            self.surfaces.remove(&surface_id);
            eprintln!("[compositor] Surface {} destroyed", surface_id);
        }
        // handle_surface_destroy:end

        // handle_pool_data:start
        //   purpose: Decompress (if needed) and cache pool pixel data. If lz4_data.len() == raw_len, data is uncompressed (backward compat). Otherwise, LZ4 decompress.
        //   input:  pool_id: SHM pool identifier; width/height/stride/format: pixel geometry; raw_len: uncompressed size; lz4_data: compressed or raw pixel bytes.
        //   output: void.
        //   sideEffects: inserts/updates pool in pools HashMap, logs pool update and first 16 bytes for debugging.
        pub fn handle_pool_data(
            &mut self,
            pool_id: u32,
            width: u16,
            height: u16,
            stride: u32,
            format: u32,
            raw_len: u32,
            lz4_data: &[u8],
        ) {
            // If lz4_data.len() == raw_len, data is uncompressed (backward compat)
            let decompressed = if lz4_data.len() == raw_len as usize {
                lz4_data.to_vec()
            } else {
                match lz4_flex::decompress(lz4_data, raw_len as usize) {
                    Ok(d) => d,
                    Err(e) => {
                        eprintln!("[compositor] LZ4 decompress failed for pool {}: {}", pool_id, e);
                        return;
                    }
                }
            };

            eprintln!(
                "[compositor] Pool {} updated: {}x{} stride={} fmt={} ({}B compressed → {}B)",
                pool_id, width, height, stride, format, lz4_data.len(), decompressed.len()
            );

            // Debug: log first 16 bytes (4 pixels) to check if data is valid
            if decompressed.len() >= 16 {
                eprintln!("[dbg] pixels[0..16] = {:?}", &decompressed[0..16]);
            } else if !decompressed.is_empty() {
                eprintln!("[dbg] pixels[0..{}] = {:?}", decompressed.len(), decompressed.as_slice());
            } else {
                eprintln!("[dbg] WARNING: decompressed data is EMPTY!");
            }

            self.pools.insert(pool_id, Pool {
                data: decompressed,
                width,
                height,
                stride,
                format,
            });
        }
        // handle_pool_data:end

        // handle_surface_commit:start
        //   purpose: Composite surface frame from pool data. Extract pixels from pool[offset..offset+stride*height], normalize stride to width*4, convert ARGB8888→RGBA if format==0, update frame output.
        //   input:  surface_id: Wayland surface; pool_id: SHM pool; offset: byte offset in pool; buf_width/height/stride/format: buffer geometry; damage_x/y/w/h: dirty rect.
        //   output: void.
        //   sideEffects: updates compositor.frame with new RGBA pixels, sets dirty=true, stores clamped damage in last_damage and surface.damage, logs errors if pool not found or too small.
        pub fn handle_surface_commit(
            &mut self,
            surface_id: u32,
            pool_id: u32,
            offset: u32,
            buf_width: u16,
            buf_height: u16,
            buf_stride: u32,
            format: u32,
            damage_x: u16,
            damage_y: u16,
            damage_w: u16,
            damage_h: u16,
        ) {
            let pool = match self.pools.get(&pool_id) {
                Some(p) => p,
                None => {
                    eprintln!("[compositor] SURFACE_COMMIT: pool {} not found", pool_id);
                    return;
                }
            };

            let w = buf_width as u32;
            let h = buf_height as u32;
            let s = buf_stride as usize;
            let off = offset as usize;

            // Extract pixel data from pool at [offset..offset+stride*height]
            let needed = off + s * (h as usize);
            if pool.data.len() < needed {
                eprintln!(
                    "[compositor] Pool {} too small: need {} have {}",
                    pool_id, needed, pool.data.len()
                );
                return;
            }

            // Build RGBA frame: normalize stride to width*4
            let expected_stride = w as usize * 4;
            let mut pixels = vec![0u8; expected_stride * (h as usize)];

            for row in 0..(h as usize) {
                let src_start = off + row * s;
                let src_end = src_start + expected_stride.min(s);
                let dst_start = row * expected_stride;
                let copy_len = (src_end - src_start).min(expected_stride);
                if src_end <= pool.data.len() && dst_start + copy_len <= pixels.len() {
                    pixels[dst_start..dst_start + copy_len]
                        .copy_from_slice(&pool.data[src_start..src_end]);
                }
            }

            // Wayland ARGB8888/XRGB8888 in little-endian memory = [B,G,R,A] or [B,G,R,X].
            // Metal BGRA8Unorm expects exactly [B,G,R,A].
            // So for format 0 (ARGB): data is already correct — pass through.
            // For format 1 (XRGB): set padding byte to 0xFF for opaque alpha.
            if format == 1 {
                for chunk in pixels.chunks_exact_mut(4) {
                    chunk[3] = 0xFF;
                }
            }

            // Clamp damage rect to surface bounds
            let damage = Rect::new(damage_x, damage_y, damage_w, damage_h);
            let clamped = clamp_damage_to_surface(damage, buf_width, buf_height)
                .unwrap_or_else(|| Rect::new(0, 0, buf_width, buf_height));

            // Store damage in surface state
            if let Some(surface) = self.surfaces.get_mut(&surface_id) {
                surface.damage = clamped;
            }

            // Store last damage for Metal renderer
            self.last_damage = clamped;

            self.frame.width = w;
            self.frame.height = h;
            self.frame.stride = w * 4;
            self.frame.data = pixels;
            self.dirty = true;
        }
        // handle_surface_commit:end

        // handle_cursor_move:start
        //   purpose: Update cursor position (currently no-op, TODO: overlay cursor on frame).
        //   input:  x, y: cursor coordinates.
        //   output: void.
        //   sideEffects: none (placeholder for future cursor rendering).
        pub fn handle_cursor_move(&mut self, x: i32, y: i32) {
            // TODO: overlay cursor position on frame
            let _ = (x, y);
        }
        // handle_cursor_move:end

        // pool_count:start
        //   purpose: Return number of cached pools.
        //   input:  none.
        //   output: usize count of pools.
        //   sideEffects: none (pure accessor).
        pub fn pool_count(&self) -> usize {
            self.pools.len()
        }
        // pool_count:end

        // surface_count:start
        //   purpose: Return number of tracked surfaces.
        //   input:  none.
        //   output: usize count of surfaces.
        //   sideEffects: none (pure accessor).
        pub fn surface_count(&self) -> usize {
            self.surfaces.len()
        }
        // surface_count:end
    }
}

#[cfg(test)]
// tests:start
//   purpose: unit-test the cross-platform v1 stream parser and Compositor
//            state machine. Covers: is_v1_protocol edge cases, parse_events
//            round-trips for all 5 event types, parse_events error paths,
//            and Compositor surface/pool/commit/cursor lifecycle. Run via
//            `make metal-viewer-test` (18 tests; no Metal, no network).
//   sideEffects: none — every test is self-contained and asserts in-memory state.
mod tests {
    use super::compositor::Compositor;
    use super::stream_parser::{
        is_v1_protocol, parse_events, StreamEvent, EV_CURSOR_MOVE, EV_POOL_DATA,
        EV_SURFACE_COMMIT, EV_SURFACE_CREATE, EV_SURFACE_DESTROY,
    };

    // ── is_v1_protocol ─────────────────────────────────────────────────────

    #[test]
    fn is_v1_protocol_rejects_short_buffers() {
        assert!(!is_v1_protocol(&[]));
        assert!(!is_v1_protocol(&[0, 0, 0, 0])); // < 5 bytes
    }

    #[test]
    fn is_v1_protocol_accepts_well_formed_envelope() {
        // [total_size=33][event=0x04]... padded out to 4+33=37 bytes so
        // the check `total_size <= data.len() - 4` is satisfied.
        let mut buf = vec![0u8; 4 + 33];
        buf[0..4].copy_from_slice(&33u32.to_le_bytes());
        buf[4] = EV_SURFACE_COMMIT;
        assert!(is_v1_protocol(&buf));
    }

    #[test]
    fn is_v1_protocol_rejects_oversized_total_size() {
        // total_size > data.len() - 4 → false
        let mut buf = vec![0u8; 5];
        buf[0..4].copy_from_slice(&1000u32.to_le_bytes());
        buf[4] = EV_POOL_DATA;
        assert!(!is_v1_protocol(&buf));
    }

    #[test]
    fn is_v1_protocol_rejects_unknown_event_type() {
        let mut buf = vec![0u8; 5];
        buf[0..4].copy_from_slice(&1u32.to_le_bytes());
        buf[4] = 0xAB;
        assert!(!is_v1_protocol(&buf));
    }

    // ── parse_events: surface create / destroy ─────────────────────────────

    #[test]
    fn parse_events_surface_create_and_destroy() {
        // Wire layout: 4-byte total_size + payload.
        // payload = [type=0x01][surface_id=42][type=0x02][surface_id=99] = 10 bytes
        let mut buf = vec![0u8; 4 + 1 + 4 + 1 + 4];
        let total = (buf.len() - 4) as u32;
        buf[0..4].copy_from_slice(&total.to_le_bytes());
        buf[4] = EV_SURFACE_CREATE;
        buf[5..9].copy_from_slice(&42u32.to_le_bytes());
        buf[9] = EV_SURFACE_DESTROY;
        buf[10..14].copy_from_slice(&99u32.to_le_bytes());

        let events = parse_events(&buf);
        assert_eq!(events.len(), 2);
        match &events[0] {
            Ok(StreamEvent::SurfaceCreate { surface_id }) => assert_eq!(*surface_id, 42),
            other => panic!("expected SurfaceCreate, got {:?}", other),
        }
        match &events[1] {
            Ok(StreamEvent::SurfaceDestroy { surface_id }) => assert_eq!(*surface_id, 99),
            other => panic!("expected SurfaceDestroy, got {:?}", other),
        }
    }

    // ── parse_events: pool data ────────────────────────────────────────────

    #[test]
    fn parse_events_pool_data_round_trip() {
        // [total_size=29][type=0x03][pool_id=7][w=8][h=2][stride=32]
        //                   [format=0][raw_len=64][lz4_len=5][5 bytes lz4 data]
        let lz4_data = [0xDE, 0xAD, 0xBE, 0xEF, 0x42];
        let mut buf = vec![0u8; 4 + 1 + 4 + 2 + 2 + 4 + 4 + 4 + 4 + lz4_data.len()];
        let total = (buf.len() - 4) as u32;
        buf[0..4].copy_from_slice(&total.to_le_bytes());
        buf[4] = EV_POOL_DATA;
        buf[5..9].copy_from_slice(&7u32.to_le_bytes());
        buf[9..11].copy_from_slice(&8u16.to_le_bytes());
        buf[11..13].copy_from_slice(&2u16.to_le_bytes());
        buf[13..17].copy_from_slice(&32u32.to_le_bytes());
        buf[17..21].copy_from_slice(&0u32.to_le_bytes());
        buf[21..25].copy_from_slice(&64u32.to_le_bytes());
        buf[25..29].copy_from_slice(&(lz4_data.len() as u32).to_le_bytes());
        buf[29..].copy_from_slice(&lz4_data);

        let events = parse_events(&buf);
        assert_eq!(events.len(), 1);
        match &events[0] {
            Ok(StreamEvent::PoolData {
                pool_id,
                width,
                height,
                stride,
                format,
                raw_len,
                lz4_data: data,
            }) => {
                assert_eq!(*pool_id, 7);
                assert_eq!(*width, 8);
                assert_eq!(*height, 2);
                assert_eq!(*stride, 32);
                assert_eq!(*format, 0);
                assert_eq!(*raw_len, 64);
                assert_eq!(*data, lz4_data);
            }
            other => panic!("expected PoolData, got {:?}", other),
        }
    }

    #[test]
    fn parse_events_pool_data_truncated_returns_error() {
        // Header says lz4_len=10, but only 3 bytes follow
        let mut buf = vec![0u8; 4 + 1 + 4 + 2 + 2 + 4 + 4 + 4 + 4 + 3];
        let total = (buf.len() - 4) as u32;
        buf[0..4].copy_from_slice(&total.to_le_bytes());
        buf[4] = EV_POOL_DATA;
        buf[25..29].copy_from_slice(&10u32.to_le_bytes());

        let events = parse_events(&buf);
        assert_eq!(events.len(), 1);
        assert!(events[0].is_err());
    }

    // ── parse_events: surface commit ───────────────────────────────────────

    #[test]
    fn parse_events_surface_commit_round_trip() {
        // [total_size=33][type=0x04][surface_id=1][pool_id=2][offset=0]
        //                   [bw=10][bh=20][bstride=40][format=0]
        //                   [dx=0][dy=0][dw=10][dh=20]
        let mut buf = vec![0u8; 4 + 33];
        buf[0..4].copy_from_slice(&33u32.to_le_bytes());
        buf[4] = EV_SURFACE_COMMIT;
        buf[5..9].copy_from_slice(&1u32.to_le_bytes());
        buf[9..13].copy_from_slice(&2u32.to_le_bytes());
        buf[13..17].copy_from_slice(&0u32.to_le_bytes());
        buf[17..19].copy_from_slice(&10u16.to_le_bytes());
        buf[19..21].copy_from_slice(&20u16.to_le_bytes());
        buf[21..25].copy_from_slice(&40u32.to_le_bytes());
        buf[25..29].copy_from_slice(&0u32.to_le_bytes());
        buf[29..31].copy_from_slice(&0u16.to_le_bytes());
        buf[31..33].copy_from_slice(&0u16.to_le_bytes());
        buf[33..35].copy_from_slice(&10u16.to_le_bytes());
        buf[35..37].copy_from_slice(&20u16.to_le_bytes());

        let events = parse_events(&buf);
        assert_eq!(events.len(), 1);
        match &events[0] {
            Ok(StreamEvent::SurfaceCommit {
                surface_id,
                pool_id,
                buf_width,
                buf_height,
                buf_stride,
                damage_w,
                damage_h,
                ..
            }) => {
                assert_eq!(*surface_id, 1);
                assert_eq!(*pool_id, 2);
                assert_eq!(*buf_width, 10);
                assert_eq!(*buf_height, 20);
                assert_eq!(*buf_stride, 40);
                assert_eq!(*damage_w, 10);
                assert_eq!(*damage_h, 20);
            }
            other => panic!("expected SurfaceCommit, got {:?}", other),
        }
    }

    // ── parse_events: cursor move ──────────────────────────────────────────

    #[test]
    fn parse_events_cursor_move() {
        // [total_size=9][type=0x05][x=1234][y=-567]
        let mut buf = vec![0u8; 4 + 1 + 4 + 4];
        buf[0..4].copy_from_slice(&9u32.to_le_bytes());
        buf[4] = EV_CURSOR_MOVE;
        buf[5..9].copy_from_slice(&1234i32.to_le_bytes());
        buf[9..13].copy_from_slice(&(-567i32).to_le_bytes());

        let events = parse_events(&buf);
        assert_eq!(events.len(), 1);
        match &events[0] {
            Ok(StreamEvent::CursorMove { x, y }) => {
                assert_eq!(*x, 1234);
                assert_eq!(*y, -567);
            }
            other => panic!("expected CursorMove, got {:?}", other),
        }
    }

    // ── parse_events: error handling ───────────────────────────────────────

    #[test]
    fn parse_events_short_buffer_returns_empty_vec() {
        let events = parse_events(&[0, 0, 0]);
        assert!(events.is_empty());
    }

    #[test]
    fn parse_events_unknown_event_returns_error_and_stops() {
        // [total_size=5][type=0xAB][...]
        let mut buf = vec![0u8; 10];
        buf[0..4].copy_from_slice(&5u32.to_le_bytes());
        buf[4] = 0xAB;

        let events = parse_events(&buf);
        assert_eq!(events.len(), 1);
        assert!(events[0].is_err());
    }

    // ── Compositor: surface lifecycle ──────────────────────────────────────

    #[test]
    fn compositor_surface_create_destroy() {
        let mut c = Compositor::new();
        assert_eq!(c.surface_count(), 0);
        c.handle_surface_create(1);
        c.handle_surface_create(2);
        assert_eq!(c.surface_count(), 2);
        c.handle_surface_destroy(1);
        assert_eq!(c.surface_count(), 1);
    }

    // ── Compositor: pool data uncompressed fast-path ──────────────────────

    #[test]
    fn compositor_pool_data_uncompressed_path() {
        // lz4_data.len() == raw_len → handle_pool_data treats it as raw bytes
        // (no decompression), so we can avoid the lz4_flex dependency in tests.
        let mut c = Compositor::new();
        let raw = vec![0u8; 16]; // 4 pixels of BGRA zeros
        c.handle_pool_data(1, 4, 1, 16, 0, 16, &raw);
        assert_eq!(c.pool_count(), 1);
    }

    // ── Compositor: surface commit produces a frame ───────────────────────

    #[test]
    fn compositor_surface_commit_produces_frame() {
        let mut c = Compositor::new();
        // Pool: 4x1 of zeros, stride=16 (4 px * 4 B/px)
        let raw = vec![0u8; 16];
        c.handle_pool_data(1, 4, 1, 16, 0, 16, &raw);

        c.handle_surface_commit(99, 1, 0, 4, 1, 16, 0, 0, 0, 4, 1);

        assert!(c.dirty);
        assert_eq!(c.frame.width, 4);
        assert_eq!(c.frame.height, 1);
        assert_eq!(c.frame.stride, 16);
        assert_eq!(c.frame.data.len(), 16);
    }

    #[test]
    fn compositor_surface_commit_missing_pool_is_noop() {
        let mut c = Compositor::new();
        // No pool registered — commit should be silently dropped.
        c.handle_surface_commit(1, 999, 0, 4, 1, 16, 0, 0, 0, 4, 1);
        assert!(!c.dirty);
        assert_eq!(c.frame.width, 0);
        assert_eq!(c.frame.data.len(), 0);
    }

    #[test]
    fn compositor_surface_commit_format_1_sets_alpha_to_0xff() {
        // format=1 is XRGB8888 in Wayland — the protocol marks the pad byte
        // as 'X' (don't care). The compositor normalises it to 0xFF so the
        // resulting RGBA is fully opaque.
        let mut c = Compositor::new();
        let mut raw = vec![0u8; 16];
        // Mark pad bytes (index 3, 7, 11, 15) as zero — they should become 0xFF
        raw[3] = 0x00;
        raw[7] = 0x00;
        raw[11] = 0x00;
        raw[15] = 0x00;
        c.handle_pool_data(1, 4, 1, 16, 1, 16, &raw);

        c.handle_surface_commit(1, 1, 0, 4, 1, 16, 1, 0, 0, 4, 1);

        assert_eq!(c.frame.data[3], 0xFF);
        assert_eq!(c.frame.data[7], 0xFF);
        assert_eq!(c.frame.data[11], 0xFF);
        assert_eq!(c.frame.data[15], 0xFF);
    }

    #[test]
    fn compositor_surface_commit_pool_too_small_does_not_panic() {
        let mut c = Compositor::new();
        let raw = vec![0u8; 4]; // not enough for 4x1 stride=16
        c.handle_pool_data(1, 4, 1, 16, 0, 16, &raw);

        c.handle_surface_commit(1, 1, 0, 4, 1, 16, 0, 0, 0, 4, 1);

        // Frame is left untouched (still zero-size from init).
        assert_eq!(c.frame.width, 0);
    }

    // ── Compositor: cursor move is a no-op placeholder ────────────────────

    #[test]
    fn compositor_cursor_move_is_noop() {
        let mut c = Compositor::new();
        // Should not panic or change dirty state.
        c.handle_cursor_move(10, 20);
        assert!(!c.dirty);
    }

    // ── Compositor: damage rect ────────────────────────────────────────────

    #[test]
    fn compositor_surface_commit_stores_damage() {
        let mut c = Compositor::new();
        let raw = vec![0u8; 16];
        c.handle_pool_data(1, 4, 1, 16, 0, 16, &raw);

        c.handle_surface_commit(99, 1, 0, 4, 1, 16, 0, 1, 0, 2, 1);

        assert!(c.dirty);
        assert_eq!(c.last_damage.x, 1);
        assert_eq!(c.last_damage.y, 0);
        assert_eq!(c.last_damage.w, 2);
        assert_eq!(c.last_damage.h, 1);
    }

    #[test]
    fn compositor_surface_commit_clamps_oversized_damage() {
        let mut c = Compositor::new();
        let raw = vec![0u8; 16];
        c.handle_pool_data(1, 4, 1, 16, 0, 16, &raw);

        // Damage extends beyond surface bounds (4x1)
        c.handle_surface_commit(99, 1, 0, 4, 1, 16, 0, 2, 0, 10, 5);

        assert!(c.dirty);
        // Should be clamped to surface bounds
        assert_eq!(c.last_damage.x, 2);
        assert_eq!(c.last_damage.y, 0);
        assert_eq!(c.last_damage.w, 2); // clamped from 10 to 2
        assert_eq!(c.last_damage.h, 1); // clamped from 5 to 1
    }

    #[test]
    fn compositor_surface_commit_zero_damage_means_full_surface() {
        let mut c = Compositor::new();
        let raw = vec![0u8; 16];
        c.handle_pool_data(1, 4, 1, 16, 0, 16, &raw);

        // Zero-sized damage means full surface (Wayland convention)
        c.handle_surface_commit(99, 1, 0, 4, 1, 16, 0, 0, 0, 0, 0);

        assert!(c.dirty);
        assert_eq!(c.last_damage.x, 0);
        assert_eq!(c.last_damage.y, 0);
        assert_eq!(c.last_damage.w, 4);
        assert_eq!(c.last_damage.h, 1);
    }
}
