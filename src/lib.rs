// START_AI_HEADER
// MODULE: lib.rs
// PURPOSE: bsdos-metal-viewer library surface — re-exports the wlstream crate's
//          parser and compositor under the wayland_stream::{stream_parser,compositor}
//          paths main.rs already imports, plus a protocol re-export.
// INTENT: wayland_stream.rs and protocol.rs used to be hand-rolled forks of the
//         wlstream crate. Now they are thin re-export shims, so main.rs and its
//         tests need no changes, but there is exactly one implementation of the
//         wire protocol + compositor logic instead of a silently-diverging copy.
//         The macOS-specific code (Metal renderer, NSEvent capture, AppKit window)
//         stays inside the binary target and is gated by
//         `[target.'cfg(target_os = "macos")'.dependencies]` in Cargo.toml.
// DEPENDENCIES: wlstream.
// PUBLIC_API:
//   wayland_stream::stream_parser::{
//       parse_events(&[u8]) -> Vec<Result<StreamEvent<'_>, String>>,
//       is_v1_protocol(&[u8]) -> bool,
//       StreamEvent::{SurfaceCreate, SurfaceDestroy, PoolData, SurfaceCommit, CursorMove},
//       EV_SURFACE_CREATE, EV_SURFACE_DESTROY, EV_POOL_DATA, EV_SURFACE_COMMIT, EV_CURSOR_MOVE,
//   }
//   wayland_stream::compositor::{Compositor, FrameOutput}
// START_INVARIANTS
//   - wayland_stream and protocol re-export wlstream verbatim — no local logic to drift.
//   - Test coverage for parser/compositor/protocol behavior lives in the wlstream
//     crate itself; this crate has no reason to duplicate it.
// END_INVARIANTS
// END_AI_HEADER

// bsdos-metal-viewer library
//
// Re-exports the wlstream crate's v1 Wayland stream protocol parser and
// compositor state machine under this crate's existing module paths.

pub mod wayland_stream;
pub mod protocol;
