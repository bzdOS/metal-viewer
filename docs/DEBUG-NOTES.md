# bsdOS Project Memory

## Zenoh 1.9 API (Critical — differs from docs)

Config key paths in Zenoh 1.9:
- **Auth**: `transport/auth/usrpwd/user` + `transport/auth/usrpwd/password` (NOT `auth/username`)
- **TLS CA**: `transport/link/tls/root_ca_certificate` = **file path**, NOT inline PEM content
- **TLS verify**: `transport/link/tls/verify_name_on_connect` (NOT `server_name_verification`)
- Config API: `cfg.insert_json5("key", "json5_value")` — keys must match zenoh-config crate struct

## Zenoh Connection Debugging

Symptom: `Scouting delay elapsed before start conditions are met` repeats every ~7s
- Means: Zenoh is in multicast scouting mode, can't find peer
- **Fix**: must set `connect/endpoints = ["tls/203.0.113.11:7447"]` explicitly in config
- Without this, Zenoh tries mDNS/multicast discovery which fails across subnets
- `Skipping name verification of TLS server` = TLS connects but routing incomplete
- If `bsdos/health` doesn't arrive within 5s = session not established
- Token is now **persistent** (see above) — no longer regenerates on restart

Known config that works (confirmed):
```
transport/auth/usrpwd/user = "bsdos"
transport/auth/usrpwd/password = "<token>"
transport/link/tls/root_ca_certificate = "/path/to/ca.pem"
transport/link/tls/verify_name_on_connect = false
connect/endpoints = ["tls/203.0.113.11:7447"]
```

## objc2 0.6 API (macOS Rust bindings)

- `Id` → `Retained`, `Class` → `AnyClass`
- String literals: `c"ClassName"` (C-string)
- `msg_send_id!` deprecated → use `msg_send!` with explicit return types
- `Retained<T>` not `Send` → use raw pointer wrapper with `unsafe impl Send`
- `MTLCreateSystemDefaultDevice()` C function for GPU device (not class method)
- Must `dlopen` MetalKit.framework before using `MTKView` class
- `MTKView::alloc(mtm)` requires `MainThreadMarker` + `MainThreadOnly` trait in scope
- NSWindow title: `window.setTitle(&NSString::from_str("..."))`
- `CGBitmapContextCreate` returns `Option<CFRetained<CGContext>>` — managed by Drop, no CGContextRelease
- `CGColorSpace::new_device_rgb()` for color space
- `CGTextEncoding::EncodingMacRoman` (deprecated but works)
- `CGContextSelectFont` + `CGContextShowTextAtPoint` on `Option<&CGContext>`

## Server-side pipeline (203.0.113.11)

- `make vm-start-wayland` starts cage + wayland-tunnel + bsdos-core
- Token is now **PERSISTENT** — reused across bsdos-core restarts from `/run/bsdos-access.token`
- Token only changes on `make zenoh-rotate-token` (explicit rotation) or if file deleted
- Get token: `make zenoh-token` or check STATUS.md (auto-updated on pipeline start)
- Heartbeat: `bsdos/health` JSON `{"ts":..., "ok":true}` every second
- Input forwarding: bsdos-core subscribes to `bsdos/input/keyboard` + `bsdos/input/pointer`
- Input writes to `/tmp/wayland-run/input.sock` (Unix socket, `[type:u8][payload]`)
- **v1 Protocol**: `[payload_size:u32 LE][event_type:u8][event data]`
  - 0x01 SURFACE_CREATE, 0x02 SURFACE_DESTROY, 0x03 POOL_DATA (LZ4, currently raw), 0x04 SURFACE_COMMIT, 0x05 CURSOR_MOVE
- Pool data: pool_id(4) + width(2) + height(2) + stride(4) + format(4) + raw_len(4) + lz4_len(4) + data
- Surface commit: surface_id(4) + pool_id(4) + offset(4) + buf_w(2) + buf_h(2) + stride(4) + format(4) + damage(8) = 32 bytes
- **LZ4 now active** — if lz4_len < raw_len: LZ4 block compressed. If equal: raw (backward compat).
- **Pool hash caching** — POOL_DATA only sent when buffer hash changes (cursor blink → new hash)
- **Damage tracking** — SURFACE_COMMIT includes actual dirty rect from wl_surface.damage opcode
- Browser stream: `bsdos/jail/appBrowser/stream` (separate pipeline)

## File locations

- Metal viewer: `mac-companion/metal-viewer/src/{main.rs, metal_view.rs, input.rs, overlay.rs, wayland_stream.rs}`
- Wayland client: `mac-companion/wayland-client/src/main.rs`
- CA cert: `certs/ca.pem`
- Protocol spec: `github.com/bzdOS/WLStream (spec)`
- Server: `root@203.0.113.10` (dev), Zenoh at `203.0.113.11:7447`
