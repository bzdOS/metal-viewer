// START_AI_HEADER
// MODULE: mac-companion/metal-viewer/src/input.rs
// PURPOSE: Input forwarding from macOS NSEvent → Zenoh publish (keyboard/pointer/scroll events).
// INTENT: Captures NSEvent on the main AppKit thread via local event monitor and forwards to
//         bsdos/input/keyboard and bsdos/input/pointer Zenoh topics via a background publisher thread.
// DEPENDENCIES: Zenoh (session, put), tokio (runtime, block_on), std (sync::Arc/Mutex/OnceLock, thread).
// PUBLIC_API: InputForwarder (new, new_no_session), push_key_event, push_pointer_event,
//             push_scroll_event, macos_keycode_to_evdev.
// END_AI_HEADER

// Input forwarding: NSEvent → Zenoh publish
// Keyboard events → bsdos/input/keyboard
// Pointer events  → bsdos/input/pointer
//
// Wire formats (Zenoh payload — bsdos-core prepends type byte before forwarding to socket):
//
// KeyEvent (16 bytes, Zenoh):
//   [key_code: u32 LE][action: u8][modifiers: u8][pad: u2][ts_ms: u64 LE]
//   action: 0=up, 1=down, 2=repeat
//   key_code: Linux evdev scancode (after macos_keycode_to_evdev translation)
//
// PointerEvent (17 bytes, Zenoh):
//   [x: f32 LE][y: f32 LE][buttons: u8][scroll_x: f32 LE][scroll_y: f32 LE]
//   x/y: absolute surface coordinates (NOT deltas)
//   buttons: bit0=left, bit1=right, bit2=middle
//
// NOTE: bsdos-core prepends [0x00] or [0x01] type byte → socket sees 17/18 bytes total.
// wayland-tunnel reads: keyboard at offset 1 (7 bytes needed), pointer at offset 1 (17 bytes needed).

#[cfg(target_os = "macos")]
pub mod input_forwarder {
    use std::sync::{Arc, Mutex};

    pub struct InputForwarder {
        // Publisher thread sends to Zenoh; events are queued via EVENT_BUFFER.
        // We keep session/handle for the publisher thread spawned in start_publisher().
        _session: Option<Arc<zenoh::Session>>,
        _handle: Option<tokio::runtime::Handle>,
    }

    impl InputForwarder {
        /// Create forwarder and spawn background publisher thread.
        /// The publisher drains EVENT_BUFFER every 4ms and sends to Zenoh.
        // new:start
//   purpose: Create InputForwarder and spawn background publisher thread that drains EVENT_BUFFER every 4ms.
//   input:  session: Arc<zenoh::Session>, handle: tokio runtime handle, kb_topic/ptr_topic: per-app Zenoh topics.
//   output: InputForwarder instance (publisher thread runs in background).
//   sideEffects: Spawns "bsdos-input-pub" thread; reads EVENT_BUFFER via Mutex every 4ms;
//                publishes 16-byte key events to kb_topic, 17-byte pointer events to ptr_topic.
        pub fn new(
            session: Arc<zenoh::Session>,
            handle: tokio::runtime::Handle,
            kb_topic: String,
            ptr_topic: String,
        ) -> Self {
            // Spawn publisher thread: owns its own clone of session+handle+topics
            let session_clone = session.clone();
            let handle_clone = handle.clone();
            std::thread::Builder::new()
                .name("bsdos-input-pub".into())
                .spawn(move || {
                    loop {
                        std::thread::sleep(std::time::Duration::from_millis(4));
                        if let Some(events) = drain_events() {
                            for ev in events {
                                match ev {
                                    InputEvent::Key { key_code, action, modifiers, ts } => {
                                        // Wire: [key_code:4][action:1][modifiers:1][pad:2][ts_ms:8] = 16 bytes
                                        let mut buf = [0u8; 16];
                                        buf[0..4].copy_from_slice(&key_code.to_le_bytes());
                                        buf[4] = action;
                                        buf[5] = modifiers;
                                        // buf[6..8] = pad (zeroed)
                                        buf[8..16].copy_from_slice(&ts.to_le_bytes());

                                        let s = session_clone.clone();
                                        let topic = kb_topic.clone();
                                        let _ = handle_clone.block_on(async move {
                                            let _ = s.put(&topic, &buf[..]).await;
                                        });
                                    }
                                    InputEvent::Pointer { x, y, buttons, scroll_x, scroll_y } => {
                                        // Wire: [x:4][y:4][buttons:1][scroll_x:4][scroll_y:4] = 17 bytes
                                        // bsdos-core prepends [0x01] → 18 bytes at socket
                                        // wayland-tunnel: input_buf[1..5]=x, [5..9]=y, [9]=buttons,
                                        //                 [10..14]=scroll_x, [14..18]=scroll_y
                                        let mut buf = [0u8; 17];
                                        buf[0..4].copy_from_slice(&x.to_le_bytes());
                                        buf[4..8].copy_from_slice(&y.to_le_bytes());
                                        buf[8] = buttons;
                                        buf[9..13].copy_from_slice(&scroll_x.to_le_bytes());
                                        buf[13..17].copy_from_slice(&scroll_y.to_le_bytes());

                                        let s = session_clone.clone();
                                        let topic = ptr_topic.clone();
                                        let _ = handle_clone.block_on(async move {
                                            let _ = s.put(&topic, &buf[..]).await;
                                        });
                                    }
                                }
                            }
                        }
                    }
                })
                .ok();

            Self {
                _session: Some(session),
                _handle: Some(handle),
            }
        }
        // new:end

        // new_no_session:start
//   purpose: Create InputForwarder without a Zenoh session (events are queued but not sent).
//   input:  none.
//   output: InputForwarder with None session/handle — all publish attempts silently no-op.
//   sideEffects: none (no thread spawned, no Zenoh connection).
        pub fn new_no_session() -> Self {
            Self {
                _session: None,
                _handle: None,
            }
        }
        // new_no_session:end
    }

    enum InputEvent {
        Key { key_code: u32, action: u8, modifiers: u8, ts: u64 },
        // x/y are absolute surface coordinates (NOT deltas)
        Pointer { x: f32, y: f32, buttons: u8, scroll_x: f32, scroll_y: f32 },
    }

    static EVENT_BUFFER: std::sync::OnceLock<Mutex<Vec<InputEvent>>> = std::sync::OnceLock::new();

    // event_buffer:start
//   purpose: Get or create the global static EVENT_BUFFER (Mutex<Vec<InputEvent>>).
//   input:  none.
//   output: &'static Mutex<Vec<InputEvent>> — lazily initialized via OnceLock.
//   sideEffects: On first call: allocates an empty Vec inside a Mutex (stored in OnceLock).
    fn event_buffer() -> &'static Mutex<Vec<InputEvent>> {
        EVENT_BUFFER.get_or_init(|| Mutex::new(Vec::new()))
    }
    // event_buffer:end

    // drain_events:start
//   purpose: Atomically drain all queued input events from EVENT_BUFFER (used by publisher thread).
//   input:  none.
//   output: Some(Vec<InputEvent>) if events available, None if buffer empty or lock poisoned.
//   sideEffects: Takes and releases Mutex lock; replaces buffer with empty Vec via std::mem::take.
    fn drain_events() -> Option<Vec<InputEvent>> {
        let buf = event_buffer();
        if let Ok(mut guard) = buf.lock() {
            if guard.is_empty() {
                return None;
            }
            Some(std::mem::take(&mut *guard))
        } else {
            None
        }
    }
    // drain_events:end

    /// Push a keyboard event into the buffer.
    /// key_code: Linux evdev scancode (caller must translate macOS VK first).
    /// action: 0=up, 1=down, 2=repeat.
    /// modifiers: bitmask (bit0=shift, bit1=ctrl, bit2=alt/option, bit3=meta/cmd).
    // push_key_event:start
//   purpose: Queue a keyboard event (key code, action, modifiers) with current timestamp into EVENT_BUFFER.
//   input:  key_code: Linux evdev scancode (caller must translate macOS VK first),
//           action: 0=up, 1=down, 2=repeat, modifiers: bitmask (bit0=shift, bit1=ctrl, bit2=alt, bit3=meta).
//   output: none (best-effort — silently drops if Mutex poisoned).
//   sideEffects: Locks EVENT_BUFFER Mutex; pushes InputEvent::Key with SystemTime epoch millis.
    pub fn push_key_event(key_code: u32, action: u8, modifiers: u8) {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        if let Ok(mut buf) = event_buffer().lock() {
            buf.push(InputEvent::Key { key_code, action, modifiers, ts });
        }
    }
    // push_key_event:end

    /// Push a pointer motion/click event with absolute surface coordinates.
    /// x/y: absolute position in surface pixels (NOT deltas).
    /// buttons: bit0=left, bit1=right, bit2=middle (0=no buttons).
    // push_pointer_event:start
//   purpose: Queue a pointer motion/click event with absolute surface coordinates into EVENT_BUFFER.
//   input:  x/y: absolute position in surface pixels (NOT deltas), buttons: bit0=left, bit1=right, bit2=middle.
//   output: none (best-effort — silently drops if Mutex poisoned).
//   sideEffects: Locks EVENT_BUFFER Mutex; pushes InputEvent::Pointer with zero scroll deltas.
    pub fn push_pointer_event(x: f32, y: f32, buttons: u8) {
        if let Ok(mut buf) = event_buffer().lock() {
            buf.push(InputEvent::Pointer { x, y, buttons, scroll_x: 0.0, scroll_y: 0.0 });
        }
    }
    // push_pointer_event:end

    /// Push a scroll event (no position update, no click).
    // push_scroll_event:start
//   purpose: Queue a scroll event (no position update, no click) into EVENT_BUFFER.
//   input:  x/y: current absolute pointer position, scroll_x/scroll_y: scroll delta values.
//   output: none (best-effort — silently drops if Mutex poisoned).
//   sideEffects: Locks EVENT_BUFFER Mutex; pushes InputEvent::Pointer with buttons=0 and given scroll deltas.
    pub fn push_scroll_event(x: f32, y: f32, scroll_x: f32, scroll_y: f32) {
        if let Ok(mut buf) = event_buffer().lock() {
            buf.push(InputEvent::Pointer { x, y, buttons: 0, scroll_x, scroll_y });
        }
    }
    // push_scroll_event:end

    /// Translate macOS virtual key code to Linux evdev scancode.
    /// Source: macOS Carbon HIToolbox/Events.h → input-event-codes.h
    // macos_keycode_to_evdev:start
//   purpose: Translate macOS HIToolbox virtual key code to Linux evdev scancode.
//   input:  macos_vk: macOS virtual key code (from NSEvent.keyCode).
//   output: Linux evdev scancode (u32), or 0 (KEY_RESERVED) if unknown — ignored by wayland-tunnel.
//   sideEffects: none (pure lookup via match on ~80 known mappings).
    pub fn macos_keycode_to_evdev(macos_vk: u16) -> u32 {
        match macos_vk {
            // Letter row
            0x00 => 30,  // A
            0x0B => 48,  // B
            0x08 => 46,  // C
            0x02 => 32,  // D
            0x0E => 18,  // E
            0x03 => 33,  // F
            0x05 => 34,  // G
            0x04 => 35,  // H
            0x22 => 23,  // I
            0x26 => 36,  // J
            0x28 => 37,  // K
            0x25 => 38,  // L
            0x2E => 50,  // M
            0x2D => 49,  // N
            0x1F => 24,  // O
            0x23 => 25,  // P
            0x0C => 16,  // Q
            0x0F => 19,  // R
            0x01 => 31,  // S
            0x11 => 20,  // T
            0x20 => 22,  // U
            0x09 => 47,  // V
            0x0D => 17,  // W
            0x07 => 45,  // X
            0x10 => 21,  // Y (QWERTY layout)
            0x06 => 44,  // Z
            // Number row
            0x12 => 2,   // 1
            0x13 => 3,   // 2
            0x14 => 4,   // 3
            0x15 => 5,   // 4
            0x17 => 6,   // 5
            0x16 => 7,   // 6
            0x1A => 8,   // 7
            0x1C => 9,   // 8
            0x19 => 10,  // 9
            0x1D => 11,  // 0
            // Punctuation / special
            0x1B => 12,  // - (minus)
            0x18 => 13,  // = (equal)
            0x21 => 26,  // [ (left bracket)
            0x1E => 27,  // ] (right bracket)
            0x2A => 43,  // \ (backslash)
            0x29 => 39,  // ; (semicolon)
            0x27 => 40,  // ' (apostrophe)
            0x32 => 41,  // ` (grave)
            0x2B => 51,  // , (comma)
            0x2F => 52,  // . (period)
            0x2C => 53,  // / (slash)
            // Control keys
            0x24 => 28,  // Return → KEY_ENTER
            0x30 => 15,  // Tab → KEY_TAB
            0x31 => 57,  // Space → KEY_SPACE
            0x33 => 14,  // Delete/Backspace → KEY_BACKSPACE
            0x35 => 1,   // Escape → KEY_ESC
            0x75 => 111, // Forward Delete → KEY_DELETE
            0x73 => 102, // Home → KEY_HOME
            0x77 => 107, // End → KEY_END
            0x74 => 104, // Page Up → KEY_PAGEUP
            0x79 => 109, // Page Down → KEY_PAGEDOWN
            // Arrow keys
            0x7E => 103, // Up → KEY_UP
            0x7D => 108, // Down → KEY_DOWN
            0x7B => 105, // Left → KEY_LEFT
            0x7C => 106, // Right → KEY_RIGHT
            // Modifiers
            0x38 => 42,  // Left Shift → KEY_LEFTSHIFT
            0x3C => 54,  // Right Shift → KEY_RIGHTSHIFT
            0x3B => 29,  // Left Control → KEY_LEFTCTRL
            0x3E => 97,  // Right Control → KEY_RIGHTCTRL
            0x3A => 56,  // Left Option/Alt → KEY_LEFTALT
            0x3D => 100, // Right Option/Alt → KEY_RIGHTALT
            0x37 => 125, // Left Command/Meta → KEY_LEFTMETA
            0x36 => 126, // Right Command/Meta → KEY_RIGHTMETA
            0x39 => 58,  // Caps Lock → KEY_CAPSLOCK
            // Function keys
            0x7A => 59,  // F1
            0x78 => 60,  // F2
            0x63 => 61,  // F3
            0x76 => 62,  // F4
            0x60 => 63,  // F5
            0x61 => 64,  // F6
            0x62 => 65,  // F7
            0x64 => 66,  // F8
            0x65 => 67,  // F9
            0x6D => 68,  // F10
            0x67 => 87,  // F11
            0x6F => 88,  // F12
            // Numpad
            0x52 => 82,  // Numpad 0
            0x53 => 79,  // Numpad 1
            0x54 => 80,  // Numpad 2
            0x55 => 81,  // Numpad 3
            0x56 => 75,  // Numpad 4
            0x57 => 76,  // Numpad 5
            0x58 => 77,  // Numpad 6
            0x59 => 71,  // Numpad 7
            0x5B => 72,  // Numpad 8
            0x5C => 73,  // Numpad 9
            0x43 => 55,  // Numpad *
            0x45 => 78,  // Numpad +
            0x4E => 74,  // Numpad -
            0x41 => 83,  // Numpad .
            0x4B => 98,  // Numpad /
            0x4C => 96,  // Numpad Enter
            _ => 0,      // unknown → 0 (KEY_RESERVED, ignored by wayland-tunnel)
        }
    }
    // macos_keycode_to_evdev:end
}
