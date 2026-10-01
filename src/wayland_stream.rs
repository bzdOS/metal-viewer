// START_AI_HEADER
// MODULE: mac-companion/metal-viewer/src/wayland_stream.rs
// PURPOSE: Wayland Stream Protocol v1 parser + compositor — re-exported from the
//          wlstream crate.
// INTENT: This used to be a hand-rolled fork of wlstream's parser + compositor
//         (byte-for-byte identical logic, copy-pasted). Forking meant a security
//         fix landing upstream (#186, bounding LZ4 decompress against a
//         wire-controlled size) never reached this copy, and it carried the exact
//         same memory-exhaustion DoS independently until it was found and patched
//         here by hand. Converge on the single upstream crate so future fixes
//         apply to every consumer at once.
// DEPENDENCIES: wlstream.
// PUBLIC_API: stream_parser (parse_events, is_v1_protocol, StreamEvent, EV_* constants),
//             compositor (Compositor, FrameOutput).
// END_AI_HEADER

pub mod stream_parser {
    pub use wlstream::parser::*;
}

pub mod compositor {
    pub use wlstream::compositor::*;
}
