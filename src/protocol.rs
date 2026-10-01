// START_AI_HEADER
// MODULE: mac-companion/metal-viewer/src/protocol.rs
// PURPOSE: Damage rect primitives — re-exported from the wlstream crate.
// INTENT: This used to be a hand-rolled fork of wlstream::protocol (byte-for-byte
//         identical logic under a different function name). Forking meant fixes to
//         one copy never reached the other; converge on the single upstream crate.
// DEPENDENCIES: wlstream.
// PUBLIC_API: Rect, clamp_damage_to_surface, merge_damage.
// END_AI_HEADER

pub use wlstream::protocol::Rect;
pub use wlstream::protocol::clamp_damage as clamp_damage_to_surface;
pub use wlstream::protocol::merge_damage;
