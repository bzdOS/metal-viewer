// START_AI_HEADER
// MODULE: lib.rs
// PURPOSE: bsdos-metal-viewer library surface — exposes the cross-platform v1
//          Wayland stream parser and the LZ4-based compositor state machine
//          so they can be unit-tested without the macOS-only Metal/AppKit stack.
// INTENT: Carve the testable logic (parser, compositor) into a lib target so
//         that the same code path runs on the host (Linux CI) and on the Mac
//         viewer binary. The macOS-specific code (Metal renderer, NSEvent
//         capture, AppKit window) stays inside the binary target and is gated
//         by `[target.'cfg(target_os = "macos")'.dependencies]` in Cargo.toml.
//         Adding a new pure helper to either submodule is the recommended way
//         to grow test coverage — do not bury protocol logic in main.rs.
// DEPENDENCIES: lz4_flex (LZ4 decompress for pool pixel data), std::collections::HashMap.
// PUBLIC_API:
//   wayland_stream::stream_parser::{
//       parse_events(&[u8]) -> Vec<Result<StreamEvent<'_>, String>>,
//       is_v1_protocol(&[u8]) -> bool,
//       StreamEvent::{SurfaceCreate, SurfaceDestroy, PoolData, SurfaceCommit, CursorMove},
//       EV_SURFACE_CREATE, EV_SURFACE_DESTROY, EV_POOL_DATA, EV_SURFACE_COMMIT, EV_CURSOR_MOVE,
//   }
//   wayland_stream::compositor::{Compositor, FrameOutput}
// START_INVARIANTS
//   - stream_parser is pure (no IO, no allocations beyond returned Vec).
//   - Compositor state is fully encapsulated in the struct; Compositor::new()
//     returns the only valid starting state.
//   - Tests live inside wayland_stream.rs (#[cfg(test)] mod tests) — there
//     are 18 unit tests covering parser round-trip, error paths, and
//     compositor surface/pool lifecycle; see `make metal-viewer-test`.
// END_INVARIANTS
// END_AI_HEADER

// bsdos-metal-viewer library
//
// Exposes the v1 Wayland stream protocol parser and compositor state machine.
// Both modules are cross-platform (no Metal, no objc2) and can be unit-tested
// on Linux CI hosts.

pub mod wayland_stream;
pub mod protocol;
