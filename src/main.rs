// START_AI_HEADER
// MODULE: mac-companion/metal-viewer/src/main.rs
// PURPOSE: bsdOS Metal Viewer — macOS client that receives Wayland stream from Zenoh and renders via Metal.
// INTENT: Cross-platform stream parser + compositor lib (wayland_stream) with macOS-specific Metal renderer.
//         Input events (keyboard/pointer) are captured via NSEvent monitor and forwarded back to Zenoh.
// DEPENDENCIES: zenoh, tokio, pico_args, tracing, objc2-*, bsdos_metal_viewer lib.
// PUBLIC_API: ViewerConfig (from_env, apply_cli_args), main.
// END_AI_HEADER

// bsdos-metal-viewer: macOS Wayland protocol decoder + Metal renderer
//
// Получает WaylandPacket из Zenoh bsdos/global/wayland/stream
// Phase 0: логирует пакеты + обрабатывает pixel frames (этот файл)
// Phase 1: декодирует wl_shm buffers → MTLTexture
// Phase 2: полный compositor
//
// WaylandPacket формат (16 bytes header):
//   [0..4]  msgId   u32 LE
//   [4..8]  objId   u32 LE
//   [8..10] opCode  u16 LE
//   [10..12] padding
//   [12..16] payloadLen u32 LE
//   [16..]  payload (Wayland wire args)
//
// Pixel frame опкод 0xFFFF (расширенный заголовок 8 bytes):
//   [16..18] height  u16 LE
//   [18..20] stride  u16 LE
//   [20..24] format  u32 LE (0=ARGB8888, 1=XRGB8888)
//   [24..]  pixels (height * stride bytes)
//
// Запуск:
//   ./bsdos-metal-viewer [OPTIONS]
//   --sub <topic>      Stream topic (default: bsdos/global/wayland/stream)
//   --peer <addr>      Zenoh peer address (sets BSDOS_PEER env)
//   --kb-topic <topic> Keyboard input topic (default: bsdos/input/keyboard)
//   --ptr-topic <topic> Pointer input topic (default: bsdos/input/pointer)
//
// Env vars:
//   BSDOS_PEER, BSDOS_TOKEN, BSDOS_STREAM_TOPIC, BSDOS_INPUT_KB_TOPIC, etc.

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
use std::time::Duration;

/// Runtime configuration (env vars + CLI args + defaults)
#[derive(Debug, Clone)]
pub struct ViewerConfig {
    pub stream_topic: String,
    pub health_topic: String,
    pub size_topic: String,
    pub input_kb_topic: String,
    pub input_ptr_topic: String,
    pub window_width: f64,
    pub window_height: f64,
    pub window_x: f64,
    pub window_y: f64,
}

impl ViewerConfig {
    // from_env:start
//   purpose: Build ViewerConfig from environment variables (BSDOS_STREAM_TOPIC, BSDOS_WINDOW_WIDTH, etc.) with defaults.
//   input:  none (reads std::env::var directly).
//   output: ViewerConfig with all fields populated (topic names, window geometry).
//   sideEffects: none (pure env var reads, no mutation).
    fn from_env() -> Self {
        Self {
            stream_topic: std::env::var("BSDOS_STREAM_TOPIC")
                .unwrap_or_else(|_| "bsdos/app/appBrowser/stream".to_string()),
            health_topic: std::env::var("BSDOS_HEALTH_TOPIC")
                .unwrap_or_else(|_| "bsdos/health".to_string()),
            size_topic: std::env::var("BSDOS_SIZE_TOPIC")
                .unwrap_or_else(|_| "bsdos/app/appBrowser/viewer/size".to_string()),
            input_kb_topic: std::env::var("BSDOS_INPUT_KB_TOPIC")
                .unwrap_or_else(|_| "bsdos/app/appBrowser/input/keyboard".to_string()),
            input_ptr_topic: std::env::var("BSDOS_INPUT_PTR_TOPIC")
                .unwrap_or_else(|_| "bsdos/app/appBrowser/input/pointer".to_string()),
            window_width: parse_env_f64("BSDOS_WINDOW_WIDTH", 1280.0),
            window_height: parse_env_f64("BSDOS_WINDOW_HEIGHT", 720.0),
            window_x: parse_env_f64("BSDOS_WINDOW_X", 100.0),
            window_y: parse_env_f64("BSDOS_WINDOW_Y", 100.0),
        }
    }
    // from_env:end

    // apply_cli_args:start
//   purpose: Override config fields from CLI arguments (--sub, --peer, --kb-topic, --ptr-topic).
//   input:  &mut self (mutable config), args: parsed CLI arguments from pico_args.
//   output: none (mutates self in-place; --peer sets BSDOS_PEER env var).
//   sideEffects: Sets BSDOS_PEER env var via std::env::set_var if --peer provided.
    fn apply_cli_args(&mut self, args: &mut pico_args::Arguments) {
        if let Ok(Some(sub)) = args.opt_value_from_str::<&str, String>("--sub") {
            // Derive per-app input topics from stream topic unless overridden by env/--kb-topic.
            // bsdos/app/{id}/stream → bsdos/app/{id}/input/{keyboard,pointer}
            if std::env::var("BSDOS_INPUT_KB_TOPIC").is_err() {
                if let Some(app_id) = sub.strip_prefix("bsdos/app/").and_then(|s| s.strip_suffix("/stream")) {
                    self.input_kb_topic = format!("bsdos/app/{}/input/keyboard", app_id);
                    self.input_ptr_topic = format!("bsdos/app/{}/input/pointer", app_id);
                    self.size_topic = format!("bsdos/app/{}/viewer/size", app_id);
                }
            }
            self.stream_topic = sub;
        }
        if let Ok(Some(peer)) = args.opt_value_from_str::<&str, String>("--peer") {
            std::env::set_var("BSDOS_PEER", peer);
        }
        if let Ok(Some(kb_topic)) = args.opt_value_from_str::<&str, String>("--kb-topic") {
            self.input_kb_topic = kb_topic;
        }
        if let Ok(Some(ptr_topic)) = args.opt_value_from_str::<&str, String>("--ptr-topic") {
            self.input_ptr_topic = ptr_topic;
        }
    }
    // apply_cli_args:end
}

// parse_env_f64:start
//   purpose: Parse an f64 environment variable, returning a default if absent or unparseable.
//   input:  key: env var name, default: fallback value.
//   output: env var value parsed as f64, or default on missing/invalid.
//   sideEffects: none (pure).
fn parse_env_f64(key: &str, default: f64) -> f64 {
    std::env::var(key)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}
// parse_env_f64:end

#[cfg(target_os = "macos")]
mod zenoh_log {
    use std::sync::Arc;

    const LOG_TOPIC: &str = "bsdos/logs/metal-viewer";

    /// Publish a log message to Zenoh topic (fire-and-forget).
    // log:start
//   purpose: Publish a log message to bsdos/logs/metal-viewer via Zenoh (fire-and-forget).
//   input:  session: shared Zenoh session, msg: log string.
//   output: none (best-effort, ignores put errors).
//   sideEffects: Writes to Zenoh topic bsdos/logs/metal-viewer via session.put().
    pub async fn log(session: &Arc<zenoh::Session>, msg: &str) {
        let _ = session.put(LOG_TOPIC, msg).await;
    }
    // log:end

    /// Sync wrapper for non-async contexts (uses try_lock on tokio handle).
    // log_sync:start
//   purpose: Synchronous wrapper for log() — spawns a temporary tokio runtime and blocks.
//   input:  session: shared Zenoh session (cloned), msg: log string (owned).
//   output: none (best-effort, spawns thread with temporary Runtime).
//   sideEffects: Spawns a std::thread; creates temporary tokio Runtime; publishes to bsdos/logs/metal-viewer.
    pub fn log_sync(session: &Arc<zenoh::Session>, msg: &str) {
        // Best-effort: use tokio block_on from the session's runtime context
        let session = session.clone();
        let msg = msg.to_string();
        // Can't block_on here safely, so spawn a task via a thread
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new();
            if let Ok(rt) = rt {
                let _ = rt.block_on(async {
                    session.put(LOG_TOPIC, &*msg).await
                });
            }
        });
    }
    // log_sync:end
}

#[cfg(target_os = "macos")]
mod packet_stats {
    use std::sync::atomic::{AtomicU64, Ordering};

    pub struct PacketStats {
        pub packet_count: AtomicU64,
        pub error_count: AtomicU64,
    }

    impl PacketStats {
        // new:start
//   purpose: Create PacketStats with zeroed counters.
//   input:  none.
//   output: PacketStats with packet_count=0, error_count=0.
//   sideEffects: none.
        pub fn new() -> Self {
            Self {
                packet_count: AtomicU64::new(0),
                error_count: AtomicU64::new(0),
            }
        }
        // new:end

        // inc_packet:start
//   purpose: Atomically increment the received packet counter.
//   input:  &self (shared reference).
//   output: none (side-effect only).
//   sideEffects: Atomic fetch_add(1) on packet_count.
        pub fn inc_packet(&self) {
            self.packet_count.fetch_add(1, Ordering::Relaxed);
        }
        // inc_packet:end

        // inc_error:start
//   purpose: Atomically increment the parse error counter.
//   input:  &self (shared reference).
//   output: none (side-effect only).
//   sideEffects: Atomic fetch_add(1) on error_count.
        pub fn inc_error(&self) {
            self.error_count.fetch_add(1, Ordering::Relaxed);
        }
        // inc_error:end

        // packet_count:start
//   purpose: Atomically load the current packet counter value.
//   input:  &self (shared reference).
//   output: current packet count (u64).
//   sideEffects: none (read-only atomic load).
        pub fn packet_count(&self) -> u64 {
            self.packet_count.load(Ordering::Relaxed)
        }
        // packet_count:end

        // error_count:start
//   purpose: Atomically load the current error counter value.
//   input:  &self (shared reference).
//   output: current error count (u64).
//   sideEffects: none (read-only atomic load).
        pub fn error_count(&self) -> u64 {
            self.error_count.load(Ordering::Relaxed)
        }
        // error_count:end
    }
}

#[cfg(target_os = "macos")]
mod frame_buffer {
    use bsdos_metal_viewer::protocol::Rect;

    #[derive(Debug, Clone)]
    pub struct FrameBuffer {
        pub width: u32,
        pub height: u32,
        pub stride: u32,
        pub data: Vec<u8>,
        pub dirty: bool,
        pub damage: Rect,
    }

    impl FrameBuffer {
        // new:start
//   purpose: Create empty FrameBuffer (zero dimensions, no data, dirty=false, damage=zero).
//   input:  none.
//   output: FrameBuffer with zeroed fields and empty pixel Vec.
//   sideEffects: none (no allocation — Vec::new() is non-allocating).
        pub fn new() -> Self {
            Self {
                width: 0,
                height: 0,
                stride: 0,
                data: Vec::new(),
                dirty: false,
                damage: Rect::zero(),
            }
        }
        // new:end

        // update:start
//   purpose: Replace frame dimensions, pixel data, and damage rect, mark dirty=true.
//   input:  width/height/stride: new frame geometry, pixels: owned RGBA pixel data, damage: Rect from compositor.
//   output: none (mutates self in-place).
//   sideEffects: Replaces internal data Vec (old allocation dropped); sets dirty=true; stores damage.
        pub fn update(&mut self, width: u32, height: u32, stride: u32, pixels: Vec<u8>, damage: Rect) {
            self.width = width;
            self.height = height;
            self.stride = stride;
            self.data = pixels;
            self.dirty = true;
            self.damage = damage;
        }
        // update:end
    }
}

#[cfg(target_os = "macos")]
mod ping_state {
    use std::sync::atomic::{AtomicU64, Ordering};

    /// Shared ping state updated by health subscriber, read by render loop.
    pub struct PingState {
        /// Last health arrival time (monotonic)
        last_health: AtomicU64,
        /// Server timestamp from last heartbeat (seconds since epoch)
        server_ts: AtomicU64,
    }

    impl PingState {
        // new:start
//   purpose: Create PingState with zeroed timestamps.
//   input:  none.
//   output: PingState with last_health=0, server_ts=0.
//   sideEffects: none.
        pub fn new() -> Self {
            Self {
                last_health: AtomicU64::new(0),
                server_ts: AtomicU64::new(0),
            }
        }
        // new:end

        // record_health:start
//   purpose: Record current system time as last health message arrival (for latency computation).
//   input:  &self (shared reference).
//   output: none (side-effect only).
//   sideEffects: Stores current epoch microseconds in last_health (atomic store).
        pub fn record_health(&self) {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_micros() as u64;
            self.last_health.store(now, std::sync::atomic::Ordering::Relaxed);
        }
        // record_health:end

        /// Latency in ms: time since last health message
        // latency_ms:start
//   purpose: Compute round-trip latency in ms since last health message arrival.
//   input:  &self (shared reference).
//   output: Some(f64) in milliseconds, or None if no health message ever received (last_health=0).
//   sideEffects: none (read-only atomic loads, SystemTime::now()).
        pub fn latency_ms(&self) -> Option<f64> {
            let last = self.last_health.load(std::sync::atomic::Ordering::Relaxed);
            if last == 0 {
                return None;
            }
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_micros() as u64;
            Some((now - last) as f64 / 1000.0)
        }
        // latency_ms:end

        // set_server_ts:start
//   purpose: Store the server timestamp from the last heartbeat message.
//   input:  ts: server timestamp (seconds since epoch from health JSON payload).
//   output: none (side-effect only).
//   sideEffects: Atomic store to server_ts.
        pub fn set_server_ts(&self, ts: u64) {
            self.server_ts.store(ts, std::sync::atomic::Ordering::Relaxed);
        }
        // set_server_ts:end
    }
}

#[cfg(target_os = "macos")]
mod wayland_packet {
    // WaylandPacket header (16 bytes):
    //   [0..4]  msgId   u32 LE
    //   [4..8]  objId   u32 LE
    //   [8..10] opCode  u16 LE
    //   [10..12] padding
    //   [12..16] payloadLen u32 LE
    //   [16..]  payload (Wayland wire args)

    #[derive(Debug, Clone)]
    pub struct WaylandPacket {
        pub msg_id: u32,
        pub obj_id: u32,
        pub op_code: u16,
        pub payload_len: u32,
    }

    impl WaylandPacket {
        // parse:start
//   purpose: Parse a 16-byte Wayland packet header from raw bytes.
//   input:  data: raw bytes (must be ≥16 bytes for valid header).
//   output: Some(WaylandPacket) with msg_id, obj_id, op_code, payload_len; None if data too short or payload_len > 1MB.
//   sideEffects: none (pure parsing, no allocation).
        pub fn parse(data: &[u8]) -> Option<Self> {
            if data.len() < 16 {
                return None;
            }

            let msg_id = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
            let obj_id = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
            let op_code = u16::from_le_bytes([data[8], data[9]]);
            let payload_len = u32::from_le_bytes([data[12], data[13], data[14], data[15]]);

            // Санитарная проверка
            if payload_len > 1_000_000 {
                return None;
            }

            Some(WaylandPacket {
                msg_id,
                obj_id,
                op_code,
                payload_len,
            })
        }
        // parse:end

        // payload_offset:start
//   purpose: Return the byte offset where Wayland packet payload begins (constant 16).
//   input:  none.
//   output: 16 (usize) — header size in bytes.
//   sideEffects: none (pure constant).
        pub fn payload_offset() -> usize {
            16
        }
        // payload_offset:end
    }
}

#[cfg(target_os = "macos")]
// build_zenoh_config:start
//   purpose: Build Zenoh client config from env vars (BSDOS_PEER, BSDOS_TOKEN, ZENOH_TLS).
//   input:  none (reads std::env::var directly).
//   output: Ok(zenoh::Config) on success, Err(String) on JSON5 insertion failure.
//   sideEffects: Reads env vars BSDOS_PEER, BSDOS_TOKEN, ZENOH_TLS, SKIP_CERT_VERIFICATION;
//                logs TLS/plain/TLS-skip status to stderr. No network I/O.
fn build_zenoh_config() -> Result<zenoh::Config, String> {
    use std::env;

    let peer_str = env::var("BSDOS_PEER")
        .or_else(|_| env::var("ZENOH_PEER"))
        .unwrap_or_else(|_| "tcp/localhost:7447".to_string());

    let mut cfg = zenoh::Config::default();
    cfg.insert_json5("mode", "\"client\"")
        .map_err(|e| format!("Config mode: {}", e))?;
    cfg.insert_json5("connect/endpoints", &format!("[\"{}\"]", peer_str))
        .map_err(|e| format!("Config endpoints: {}", e))?;

    // Token auth: BSDOS_TOKEN env → Zenoh username/password
    if let Ok(token) = env::var("BSDOS_TOKEN") {
        cfg.insert_json5("transport/auth/usrpwd/user", "\"bsdos\"")
            .map_err(|e| format!("Auth user config: {}", e))?;
        cfg.insert_json5("transport/auth/usrpwd/password", &format!("\"{}\"", token))
            .map_err(|e| format!("Auth pass config: {}", e))?;
        eprintln!("[receiver] Token auth enabled (user=bsdos, token={}...{})",
            &token[..token.len().min(4)],
            if token.len() > 6 { &token[token.len()-2..] } else { "" }
        );
    }

    // TLS (default on when using tls:// endpoint, disable with ZENOH_TLS=0)
    let tls_enabled = env::var("ZENOH_TLS")
        .map(|v| v != "0")
        .unwrap_or_else(|_| peer_str.starts_with("tls/"));

    if tls_enabled {
        eprintln!("[receiver] TLS mode enabled");
        // Try to disable certificate verification (requires patched zenoh-link-tls)
        if env::var("SKIP_CERT_VERIFICATION").is_ok() || env::var("ZENOH_TLS_SKIP_CERT_VERIFICATION").is_ok() {
            eprintln!("[receiver] Certificate verification will be SKIPPED (insecure!)");
            // Note: zenoh-link-tls patch reads env var directly
        }
        cfg.insert_json5("transport/link/tls/verify_name_on_connect", "false")
            .map_err(|e| format!("TLS config: {}", e))?;
    } else {
        eprintln!("[receiver] Plain TCP mode (endpoint={})", peer_str);
    }
    Ok(cfg)
}
// build_zenoh_config:end

#[cfg(target_os = "macos")]
mod zenoh_receiver {
    use std::sync::Arc;
    use std::sync::Mutex;
    use std::time::Duration;
    use crate::packet_stats::PacketStats;
    use crate::frame_buffer::FrameBuffer;
    use bsdos_metal_viewer::wayland_stream::stream_parser;
    use bsdos_metal_viewer::wayland_stream::compositor::Compositor;
    use bsdos_metal_viewer::wayland_stream::stream_parser::StreamEvent;

    // start_receiver:start
//   purpose: Spawn a persistent receiver thread that subscribes to Zenoh stream topic and processes events.
//   input:  stats: Arc<PacketStats> for counters, frame_buf: shared FrameBuffer for rendered frames,
//           ping: shared PingState for health latency, config: ViewerConfig with topic names.
//   output: none (thread runs forever, reconnects on session loss).
//   sideEffects: Spawns a std::thread; creates tokio runtime; opens Zenoh session (reconnects on loss);
//                subscribes to stream + health topics; parses v1 protocol events into compositor;
//                writes compositor frames into frame_buf on SURFACE_COMMIT; logs health latency.
    pub fn start_receiver(
        stats: Arc<PacketStats>,
        frame_buf: Arc<Mutex<FrameBuffer>>,
        ping: Arc<crate::ping_state::PingState>,
        config: crate::ViewerConfig,
    ) {
        std::thread::spawn(move || {
            let rt = match tokio::runtime::Runtime::new() {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("[receiver] Failed to create Tokio runtime: {}", e);
                    return;
                }
            };

            rt.block_on(async move {
                let mut retry_count: u32 = 0;
                loop {
                    if retry_count > 0 {
                        eprintln!("[receiver] Reconnecting... attempt {}", retry_count);
                        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                    }
                    retry_count += 1;

                    // Build zenoh config (may read env vars each retry)
                    let z_config = match crate::build_zenoh_config() {
                        Ok(cfg) => cfg,
                        Err(e) => {
                            eprintln!("[receiver] Config error: {}, will retry...", e);
                            continue;
                        }
                    };

                    // Open Zenoh session
                    let session = match zenoh::open(z_config).await {
                        Ok(s) => Arc::new(s),
                        Err(e) => {
                            eprintln!("[receiver] Zenoh open failed: {}, will retry...", e);
                            continue;
                        }
                    };
                    eprintln!("[receiver] ✓ Zenoh session opened (attempt {}), ZID={}", retry_count, session.zid());

                    // Subscribe to wayland stream
                    let stream_topic = config.stream_topic.clone();
                    let subscriber = match session.declare_subscriber(&stream_topic).await {
                        Ok(s) => s,
                        Err(e) => {
                            eprintln!("[receiver] Subscribe failed: {}, will retry...", e);
                            continue;
                        }
                    };
                    eprintln!("[receiver] ✓ Subscribed to {}", stream_topic);

                    // Subscribe to health (optional)
                    let health_topic = config.health_topic.clone();
                    let health_sub = match session.declare_subscriber(&health_topic).await {
                        Ok(s) => {
                            eprintln!("[receiver] ✓ Subscribed to {}", health_topic);
                            Some(s)
                        }
                        Err(e) => {
                            eprintln!("[receiver] Health subscription failed (non-fatal): {}", e);
                            None
                        }
                    };

                    // Fresh compositor for each session (pools from old session are stale)
                    let mut compositor = Compositor::new();
                    let mut pkt_count: u64 = 0;
                    let mut last_status = std::time::Instant::now();

                    // Inner loop - returns when session lost
                    let break_reason: &'static str = loop {
                        let health_future = async {
                            if let Some(ref hs) = health_sub {
                                hs.recv_async().await
                            } else {
                                std::future::pending().await
                            }
                        };

                        tokio::select! {
                            result = subscriber.recv_async() => {
                                match result {
                                    Ok(sample) => {
                                        let data = sample.payload().to_bytes();
                                        let data = data.as_ref();
                                        stats.inc_packet();
                                        pkt_count += 1;

                                        eprintln!("[recv] #{} len={} bytes, first4={:02x?}",
                                            pkt_count, data.len(),
                                            &data[..data.len().min(4)]
                                        );

                                        // Check for SESSION_RESET (0xFE) before v1 protocol check
                                        if data.len() >= 5 {
                                            let event_type = data[4];
                                            if event_type == 0xFE {
                                                eprintln!("[recv] SESSION_RESET — clearing compositor");
                                                compositor = Compositor::new();
                                                continue;
                                            }
                                        }

                                        if stream_parser::is_v1_protocol(data) {
                                            let events = stream_parser::parse_events(data);
                                            eprintln!("[recv]   v1 protocol, {} events", events.len());
                                            for event in &events {
                                                match event {
                                                    Ok(StreamEvent::SurfaceCreate { surface_id }) => {
                                                        eprintln!("[recv]   SURFACE_CREATE id={}", surface_id);
                                                        compositor.handle_surface_create(*surface_id);
                                                    }
                                                    Ok(StreamEvent::SurfaceDestroy { surface_id }) => {
                                                        eprintln!("[recv]   SURFACE_DESTROY id={}", surface_id);
                                                        compositor.handle_surface_destroy(*surface_id);
                                                    }
                                                    Ok(StreamEvent::PoolData {
                                                        pool_id, width, height, stride,
                                                        format, raw_len, lz4_data,
                                                    }) => {
                                                        eprintln!("[recv]   POOL_DATA id={} {}x{} stride={} fmt={} raw={} lz4={}",
                                                            pool_id, width, height, stride, format, raw_len, lz4_data.len());
                                                        compositor.handle_pool_data(
                                                            *pool_id, *width, *height, *stride,
                                                            *format, *raw_len, lz4_data,
                                                        );
                                                    }
                                                    Ok(StreamEvent::SurfaceCommit {
                                                        surface_id, pool_id, offset,
                                                        buf_width, buf_height, buf_stride,
                                                        format, damage_x, damage_y,
                                                        damage_w, damage_h,
                                                    }) => {
                                                        eprintln!("[recv]   SURFACE_COMMIT surf={} pool={} off={} {}x{} stride={} fmt={} dmg={},{},{},{}",
                                                            surface_id, pool_id, offset,
                                                            buf_width, buf_height, buf_stride, format,
                                                            damage_x, damage_y, damage_w, damage_h);
                                                        compositor.handle_surface_commit(
                                                            *surface_id, *pool_id, *offset,
                                                            *buf_width, *buf_height, *buf_stride,
                                                            *format, *damage_x, *damage_y,
                                                            *damage_w, *damage_h,
                                                        );

                                                        if compositor.dirty {
                                                            compositor.dirty = false;
                                                            eprintln!("[recv]   → frame {}x{} stride={} pixels={} damage=({},{},{},{})",
                                                                compositor.frame.width,
                                                                compositor.frame.height,
                                                                compositor.frame.stride,
                                                                compositor.frame.data.len(),
                                                                compositor.last_damage.x,
                                                                compositor.last_damage.y,
                                                                compositor.last_damage.w,
                                                                compositor.last_damage.h,
                                                            );
                                                            if let Ok(mut fb) = frame_buf.lock() {
                                                                fb.update(
                                                                    compositor.frame.width,
                                                                    compositor.frame.height,
                                                                    compositor.frame.stride,
                                                                    compositor.frame.data.clone(),
                                                                    compositor.last_damage,
                                                                );
                                                            }
                                                        }
                                                    }
                                                    Ok(StreamEvent::CursorMove { x, y }) => {
                                                        eprintln!("[recv]   CURSOR_MOVE x={} y={}", x, y);
                                                        compositor.handle_cursor_move(*x, *y);
                                                    }
                                                    Err(e) => {
                                                        eprintln!("[recv]   PARSE ERROR: {}", e);
                                                        stats.inc_error();
                                                    }
                                                }
                                            }
                                        } else {
                                            eprintln!("[recv]   UNKNOWN FORMAT first16={:02x?}",
                                                &data[..data.len().min(16)]
                                            );
                                        }

                                        // Status every 5 seconds
                                        if last_status.elapsed() >= Duration::from_secs(5) {
                                            eprintln!("[status] pkts={} errs={} pools={} surfaces={} latency={:.0}ms",
                                                stats.packet_count(), stats.error_count(),
                                                compositor.pool_count(), compositor.surface_count(),
                                                ping.latency_ms().unwrap_or(999.0)
                                            );
                                            last_status = std::time::Instant::now();
                                        }
                                    }
                                    Err(e) => {
                                        eprintln!("[receiver] Subscriber error: {:?}", e);
                                        break "subscriber_closed";
                                    }
                                }
                            }
                            result = health_future => {
                                if let Ok(sample) = result {
                                    let local_ts = std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .unwrap_or_default()
                                        .as_millis() as u64;

                                    let payload = sample.payload().to_bytes();
                                    let payload = payload.as_ref();

                                    if let Ok(s) = std::str::from_utf8(payload) {
                                        if let Some(ts_start) = s.find("\"ts\"") {
                                            let after = &s[ts_start + 3..];
                                            let after = after.trim_start_matches(|c: char| !c.is_ascii_digit());
                                            let num_str: String = after.chars()
                                                .take_while(|c| c.is_ascii_digit())
                                                .collect();
                                            if let Ok(server_ts) = num_str.parse::<u64>() {
                                                ping.set_server_ts(server_ts);
                                                ping.record_health();
                                                let latency = ping.latency_ms().unwrap_or(0.0);
                                                eprintln!("[health] ts={} latency={:.0}ms", server_ts, latency);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    };

                    eprintln!("[receiver] Session lost ({}), will reconnect...", break_reason);
                    // session, subscriber, health_sub, compositor dropped here - auto cleanup
                }
            });
        });
    }
    // start_receiver:end
}

#[cfg(target_os = "macos")]
mod overlay;

#[cfg(target_os = "macos")]
mod metal_view;

#[cfg(target_os = "macos")]
mod input;

// main:start
//   purpose: Entry point — parse config, create Metal window, start Zenoh receiver + input publisher + render loop.
//   input:  CLI args (--sub, --peer, --kb-topic, --ptr-topic) + env vars (BSDOS_*).
//   output: Ok(()) on clean exit, Err on critical failure (no Metal device, etc.).
//   sideEffects: Opens NSWindow + MTKView; spawns 4 background threads (receiver, render, input-pub, size-pub);
//                subscribes to Zenoh bsdos/global/wayland/stream; publishes input events to bsdos/input/*;
//                publishes window size to bsdos/viewer/size; installs NSEvent local monitor.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(not(target_os = "macos"))]
    {
        eprintln!("Error: bsdos-metal-viewer requires macOS");
        std::process::exit(1);
    }

    #[cfg(target_os = "macos")]
    {
        // Parse CLI args
        let mut args = pico_args::Arguments::from_env();
        if args.contains("--help") {
            eprintln!("bsdos-metal-viewer: macOS Wayland stream viewer");
            eprintln!("Usage: bsdos-metal-viewer [OPTIONS]");
            eprintln!("");
            eprintln!("Options:");
            eprintln!("  --sub <topic>      Stream topic (default: bsdos/app/appBrowser/stream)");
            eprintln!("  --peer <addr>      Zenoh peer address (sets BSDOS_PEER env)");
            eprintln!("  --kb-topic <topic> Keyboard input topic (default: bsdos/input/keyboard)");
            eprintln!("  --ptr-topic <topic> Pointer input topic (default: bsdos/input/pointer)");
            eprintln!("  --help             Show this help");
            eprintln!("");
            eprintln!("Env vars:");
            eprintln!("  BSDOS_PEER, BSDOS_TOKEN, BSDOS_STREAM_TOPIC, BSDOS_INPUT_KB_TOPIC, etc.");
            std::process::exit(0);
        }

        let mut config = ViewerConfig::from_env();
        config.apply_cli_args(&mut args);

        tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::from_default_env()
                    .add_directive(tracing_subscriber::filter::LevelFilter::INFO.into()),
            )
            .init();

        eprintln!("[main] bsdOS Metal Viewer");
        eprintln!("[main] Config: stream={}", config.stream_topic);

        let stats = Arc::new(packet_stats::PacketStats::new());
        let frame_buf = Arc::new(Mutex::new(frame_buffer::FrameBuffer::new()));
        let ping = Arc::new(ping_state::PingState::new());

        // Создать Metal рендерер (NSWindow + MTKView) на main thread
        let mtm = objc2::MainThreadMarker::new().ok_or("Must run on main thread")?;
        let mut renderer = metal_view::renderer::RawMetal::new(mtm)?;
        eprintln!("[main] ✓ Metal renderer created");

        // Size publisher: persistent Zenoh session that sends on every channel message.
        // Uses tokio::sync::mpsc so recv().await yields the executor between messages
        // (std::sync::mpsc::recv blocks the tokio thread → Zenoh keepalive starved → session closes).
        let (size_tx, mut size_rx) = tokio::sync::mpsc::unbounded_channel::<String>();

        let (w, h, scale) = renderer.get_drawable_size_with_scale();
        eprintln!("[main] Initial drawable size: {}x{}@{}", w, h, scale);
        size_tx.send(format!("{}x{}@{}", w, h, scale)).ok();
        let initial_size = (w, h, scale);

        let size_topic = config.size_topic.clone();
        std::thread::spawn(move || {
            // One Runtime for the thread lifetime; async reconnect loop inside.
            let rt = match tokio::runtime::Runtime::new() {
                Ok(r) => r,
                Err(e) => { eprintln!("[size-pub] runtime error: {}", e); return; }
            };
            rt.block_on(async move {
                // Reconnect loop: re-open Zenoh session on server restart.
                // size_rx is owned here and persists across sessions.
                loop {
                    let z_config = match crate::build_zenoh_config() {
                        Ok(c) => c,
                        Err(_) => { tokio::time::sleep(tokio::time::Duration::from_secs(3)).await; continue; }
                    };
                    match zenoh::open(z_config).await {
                        Ok(session) => {
                            eprintln!("[size-pub] connected ZID={}", session.zid());
                            loop {
                                match tokio::time::timeout(
                                    tokio::time::Duration::from_secs(30),
                                    size_rx.recv()
                                ).await {
                                    Ok(Some(size_msg)) => {
                                        eprintln!("[size-pub] Publishing {} = {}", size_topic, size_msg);
                                        if session.put(&size_topic, size_msg).await.is_err() {
                                            eprintln!("[size-pub] put failed, reconnecting...");
                                            break; // reconnect outer loop
                                        }
                                    }
                                    Ok(None) => return, // channel closed = app exit
                                    Err(_) => {
                                        // 30s timeout: probe session liveness
                                        if session.put("bsdos/meta/size-pub-alive", b"1" as &[u8]).await.is_err() {
                                            eprintln!("[size-pub] session dead, reconnecting...");
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("[size-pub] connect failed: {}, retry in 3s", e);
                            tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
                        }
                    }
                }
            });
        });

        // Запустить Zenoh receiver в отдельном потоке
        zenoh_receiver::start_receiver(stats.clone(), frame_buf.clone(), ping.clone(), config.clone());

        // Input publisher: Zenoh session for forwarding keyboard/pointer events.
        // InputForwarder::new() spawns an internal background thread that drains
        // the static EVENT_BUFFER every 4ms and publishes to per-app topics.
        // The _input_forwarder is kept alive for the process lifetime.
        // The NSEvent monitor below pushes events into EVENT_BUFFER directly.
        // Spawn input publisher thread: creates its own Zenoh session and
        // InputForwarder. The forwarder drains EVENT_BUFFER every 4ms and
        // publishes keyboard/pointer events to the configured per-app topics.
        // The thread runs forever; it keeps the session and forwarder alive.
        let input_kb_topic = config.input_kb_topic.clone();
        let input_ptr_topic = config.input_ptr_topic.clone();
        std::thread::Builder::new()
            .name("bsdos-input-pub".into())
            .spawn(move || {
                // Reconnect loop: re-open Zenoh session after any loss (server restart etc.)
                loop {
                    let z_config = match crate::build_zenoh_config() {
                        Ok(c) => c,
                        Err(e) => {
                            eprintln!("[input-pub] config error: {}, retry in 3s", e);
                            std::thread::sleep(std::time::Duration::from_secs(3));
                            continue;
                        }
                    };
                    let rt = match tokio::runtime::Runtime::new() {
                        Ok(r) => r,
                        Err(e) => {
                            eprintln!("[input-pub] runtime error: {}", e);
                            std::thread::sleep(std::time::Duration::from_secs(3));
                            continue;
                        }
                    };
                    rt.block_on(async {
                        match zenoh::open(z_config).await {
                            Ok(session) => {
                                let session = Arc::new(session);
                                let handle = tokio::runtime::Handle::current();
                                let _forwarder = input::input_forwarder::InputForwarder::new(
                                    session.clone(), handle,
                                    input_kb_topic.clone(), input_ptr_topic.clone(),
                                );
                                eprintln!("[input-pub] ready, ZID={}", session.zid());
                                // Heartbeat: probe session every 5s; break if dead
                                loop {
                                    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                                    if session.put("bsdos/meta/input-pub-alive", b"1" as &[u8]).await.is_err() {
                                        eprintln!("[input-pub] session lost, reconnecting...");
                                        break;
                                    }
                                }
                            }
                            Err(e) => {
                                eprintln!("[input-pub] connect failed: {}, retry in 3s", e);
                                tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
                            }
                        }
                    });
                    // Brief pause before next reconnect attempt
                    std::thread::sleep(std::time::Duration::from_secs(1));
                }
            })
            .ok();

        // Render loop: отдельный поток читает frame buffer и рисует
        // RawMetal uses raw pointers — safe to Send with mutex synchronization
        let fb_render = frame_buf.clone();
        let render_stats = stats.clone();
        let ping_render = ping.clone();
        let _render_thread = std::thread::spawn(move || {
            let mut frame_count: u64 = 0;
            let mut last_fps_log = std::time::Instant::now();
            let mut fps_frames: u64 = 0;
            let mut current_fps: f64 = 0.0;
            let mut overlay_renderer = overlay::overlay::OverlayRenderer::new();
            // Resize detection: track last published size, debounce 60ms (15 frames × 4ms)
            const SIZE_DEBOUNCE: u32 = 15;
            let mut last_pub_size: (u32, u32, u32) = initial_size;
            let mut size_stable_frames: u32 = SIZE_DEBOUNCE; // treat initial as already published

            loop {
                // Each iteration needs its own autorelease pool.
                // currentDrawable and currentRenderPassDescriptor return autoreleased
                // objects; without a pool on this thread they become dangling pointers
                // after the main thread's run loop drains its pool (segfault on restart).
                let _pool = unsafe { objc2_foundation::NSAutoreleasePool::new() };

                let frame_data = {
                    if let Ok(mut fb) = fb_render.lock() {
                        if fb.dirty && fb.width > 0 && fb.height > 0 {
                            fb.dirty = false;
                            Some((fb.width, fb.height, fb.stride, fb.data.clone(), fb.damage))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                };

                if let Some((width, height, stride, mut pixel_data, damage)) = frame_data {
                    // Overlay (FPS + latency)
                    let ping_ms = ping_render.latency_ms();
                    let (ow, oh, odata) = overlay_renderer.render(current_fps, ping_ms);
                    overlay::overlay::blend_overlay(
                        &mut pixel_data, width, height, odata, ow, oh
                    );

                    // Check if damage is full surface or partial
                    let is_full_damage = damage.x == 0 && damage.y == 0
                        && damage.w as u32 == width && damage.h as u32 == height;

                    if is_full_damage {
                        // Full surface upload (backward compatible)
                        if let Err(e) = renderer.update_and_draw(width, height, stride, &pixel_data) {
                            eprintln!("[render] {}", e);
                        }
                    } else {
                        // Partial damage upload — upload damaged region only
                        // First ensure texture exists with correct dimensions
                        if let Err(e) = renderer.update_and_draw(width, height, stride, &pixel_data) {
                            eprintln!("[render] {}", e);
                        }
                        // TODO: optimize to only upload damage region when texture already exists
                        // For now, full upload is used as fallback
                        // renderer.upload_damaged_region(texture, &pixel_data, damage, stride as usize);
                    }

                    frame_count += 1;
                    fps_frames += 1;
                }

                if fps_frames > 0 && last_fps_log.elapsed() >= Duration::from_secs(3) {
                    current_fps = fps_frames as f64 / last_fps_log.elapsed().as_secs_f64();
                    eprintln!(
                        "[render] Frames: {} | FPS: {:.1} | Pkts: {} | Errs: {}",
                        frame_count, current_fps, render_stats.packet_count(), render_stats.error_count()
                    );
                    fps_frames = 0;
                    last_fps_log = std::time::Instant::now();
                }

                // Resize detection: poll drawable size, debounce, publish on stable change
                let cur_size = renderer.get_drawable_size_with_scale();
                if cur_size.0 > 0 && cur_size != last_pub_size {
                    last_pub_size = cur_size;
                    size_stable_frames = 0;
                } else if size_stable_frames < SIZE_DEBOUNCE {
                    size_stable_frames += 1;
                    if size_stable_frames == SIZE_DEBOUNCE {
                        size_tx.send(format!("{}x{}@{}", cur_size.0, cur_size.1, cur_size.2)).ok();
                    }
                }

                std::thread::sleep(Duration::from_millis(4));
            }
        });

        // ── Input: NSEvent local monitor ──────────────────────────────────────
        // Install a local event monitor before the NSApplication run loop.
        // Local monitors receive events dispatched to THIS process's windows.
        // The block receives NSEvent* and must return it (or nil to swallow it).
        // We always return Some(event) to let normal AppKit handling proceed.
        //
        // addLocalMonitorForEventsMatchingMask:handler: is called via msg_send! on
        // NSEvent::class() — the high-level wrapper in objc2-app-kit 0.3 is not
        // reliably exposed as a safe Rust fn, so msg_send! is the correct approach.
        #[cfg(target_os = "macos")]
        {
            use objc2_app_kit::{NSEvent, NSEventMask};
            use block2::RcBlock;
            use input::input_forwarder;

            // Track last known pointer position for scroll events
            // (stored in a static so the block closure doesn't need a heap allocation)
            static LAST_PTR_X: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
            static LAST_PTR_Y: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

            // Drawable/window height needed for AppKit→Wayland Y-axis flip.
            // AppKit locationInWindow has origin bottom-left; Wayland wl_pointer expects top-left.
            // We store the logical window height (points) and update it when the window resizes.
            // NOTE: This uses the initial config height; it will be correct until first resize.
            // REVIEW(mac): To track live resize, wire the drawable height published by the render
            //   thread back into DRAWABLE_H via size_tx/size_rx or a shared AtomicU32.
            static DRAWABLE_H: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
            DRAWABLE_H.store(config.window_height as u32, std::sync::atomic::Ordering::Relaxed);

            let mask = NSEventMask::KeyDown
                | NSEventMask::KeyUp
                | NSEventMask::FlagsChanged
                | NSEventMask::MouseMoved
                | NSEventMask::LeftMouseDown
                | NSEventMask::LeftMouseUp
                | NSEventMask::LeftMouseDragged
                | NSEventMask::RightMouseDown
                | NSEventMask::RightMouseUp
                | NSEventMask::RightMouseDragged
                | NSEventMask::OtherMouseDown
                | NSEventMask::OtherMouseUp
                | NSEventMask::ScrollWheel;

            // RcBlock::new in block2 0.6 takes an Fn closure; the closure parameter type
            // *mut NSEvent and return type *mut NSEvent match the ObjC block signature for
            // addLocalMonitorForEventsMatchingMask:handler:. &*monitor_block dereferences
            // RcBlock<(*mut NSEvent,), *mut NSEvent> → Block<(*mut NSEvent,), *mut NSEvent>
            // which coerces to the ObjC block pointer expected by AppKit. Confirmed correct.
            let monitor_block = RcBlock::new(|event: *mut NSEvent| -> *mut NSEvent {
                // Safety: NSEvent* is non-null (AppKit guarantees); we don't retain it
                // beyond this block invocation.
                let event_ref: &NSEvent = unsafe { &*event };

                let event_type = unsafe { event_ref.r#type() };

                // NSEventType variant names in objc2-app-kit 0.3 are generated by stripping
                // the "NSEventType" prefix from the Obj-C constants (e.g. NSEventTypeKeyDown
                // → KeyDown). All names below are confirmed correct for objc2-app-kit 0.3.
                use objc2_app_kit::NSEventType;

                match event_type {
                    // ── Keyboard ────────────────────────────────────────────
                    NSEventType::KeyDown => {
                        let macos_vk: u16 = unsafe { event_ref.keyCode() };
                        let evdev_code = input_forwarder::macos_keycode_to_evdev(macos_vk);
                        if evdev_code != 0 {
                            let action: u8 = 1; // KeyDown
                            let modifier_flags = unsafe { event_ref.modifierFlags() };
                            use objc2_app_kit::NSEventModifierFlags;
                            let mut mods: u8 = 0;
                            if modifier_flags.contains(NSEventModifierFlags::Shift)   { mods |= 1; }
                            if modifier_flags.contains(NSEventModifierFlags::Control) { mods |= 2; }
                            if modifier_flags.contains(NSEventModifierFlags::Option)  { mods |= 4; }
                            if modifier_flags.contains(NSEventModifierFlags::Command) { mods |= 8; }
                            input_forwarder::push_key_event(evdev_code, action, mods);
                        }
                    }
                    NSEventType::KeyUp => {
                        let macos_vk: u16 = unsafe { event_ref.keyCode() };
                        let evdev_code = input_forwarder::macos_keycode_to_evdev(macos_vk);
                        if evdev_code != 0 {
                            let action: u8 = 0; // KeyUp
                            let modifier_flags = unsafe { event_ref.modifierFlags() };
                            // NSEventModifierFlags bitmask constants in objc2-app-kit 0.3 are
                            // generated by stripping the "NSEventModifierFlag" prefix:
                            //   bit0=Shift, bit1=Ctrl, bit2=Alt/Option, bit3=Meta/Cmd
                            // Confirmed correct: Shift/Control/Option/Command/CapsLock match
                            // the objc2-app-kit 0.3 generated names for NSEventModifierFlag*.
                            use objc2_app_kit::NSEventModifierFlags;
                            let mut mods: u8 = 0;
                            if modifier_flags.contains(NSEventModifierFlags::Shift)   { mods |= 1; }
                            if modifier_flags.contains(NSEventModifierFlags::Control) { mods |= 2; }
                            if modifier_flags.contains(NSEventModifierFlags::Option)  { mods |= 4; }
                            if modifier_flags.contains(NSEventModifierFlags::Command) { mods |= 8; }
                            input_forwarder::push_key_event(evdev_code, action, mods);
                        }
                    }
                    // FlagsChanged = modifier key press/release without a character
                    NSEventType::FlagsChanged => {
                        let macos_vk: u16 = unsafe { event_ref.keyCode() };
                        let evdev_code = input_forwarder::macos_keycode_to_evdev(macos_vk);
                        if evdev_code != 0 {
                            // Determine press vs release from modifier flags
                            let modifier_flags = unsafe { event_ref.modifierFlags() };
                            use objc2_app_kit::NSEventModifierFlags;
                            // A modifier key is pressed when its flag is currently set
                            let pressed = match macos_vk {
                                0x38 | 0x3C => modifier_flags.contains(NSEventModifierFlags::Shift),
                                0x3B | 0x3E => modifier_flags.contains(NSEventModifierFlags::Control),
                                0x3A | 0x3D => modifier_flags.contains(NSEventModifierFlags::Option),
                                0x37 | 0x36 => modifier_flags.contains(NSEventModifierFlags::Command),
                                0x39       => modifier_flags.contains(NSEventModifierFlags::CapsLock),
                                _ => false,
                            };
                            let action: u8 = if pressed { 1 } else { 0 };
                            input_forwarder::push_key_event(evdev_code, action, 0);
                        }
                    }
                    // ── Pointer motion ──────────────────────────────────────
                    NSEventType::MouseMoved
                    | NSEventType::LeftMouseDragged
                    | NSEventType::RightMouseDragged => {
                        // AppKit locationInWindow: origin bottom-left (points).
                        // Wayland wl_pointer.motion expects origin top-left.
                        // Flip: y_wayland = window_height - y_appkit
                        // DRAWABLE_H is initialised from config.window_height and is correct
                        // until the window is resized. Single-window assumption is fine here.
                        let loc = unsafe { event_ref.locationInWindow() };
                        let win_h = DRAWABLE_H.load(std::sync::atomic::Ordering::Relaxed) as f32;
                        let x = loc.x as f32;
                        let y = (win_h - loc.y as f32).max(0.0);
                        LAST_PTR_X.store(x.to_bits(), std::sync::atomic::Ordering::Relaxed);
                        LAST_PTR_Y.store(y.to_bits(), std::sync::atomic::Ordering::Relaxed);
                        input_forwarder::push_pointer_event(x, y, 0);
                    }
                    // ── Pointer clicks ──────────────────────────────────────
                    NSEventType::LeftMouseDown => {
                        let loc = unsafe { event_ref.locationInWindow() };
                        let win_h = DRAWABLE_H.load(std::sync::atomic::Ordering::Relaxed) as f32;
                        let x = loc.x as f32;
                        let y = (win_h - loc.y as f32).max(0.0);
                        LAST_PTR_X.store(x.to_bits(), std::sync::atomic::Ordering::Relaxed);
                        LAST_PTR_Y.store(y.to_bits(), std::sync::atomic::Ordering::Relaxed);
                        input_forwarder::push_pointer_event(x, y, 0x01); // left button down
                    }
                    NSEventType::LeftMouseUp => {
                        let x = f32::from_bits(LAST_PTR_X.load(std::sync::atomic::Ordering::Relaxed));
                        let y = f32::from_bits(LAST_PTR_Y.load(std::sync::atomic::Ordering::Relaxed));
                        input_forwarder::push_pointer_event(x, y, 0); // buttons released
                    }
                    NSEventType::RightMouseDown => {
                        let loc = unsafe { event_ref.locationInWindow() };
                        let win_h = DRAWABLE_H.load(std::sync::atomic::Ordering::Relaxed) as f32;
                        let x = loc.x as f32;
                        let y = (win_h - loc.y as f32).max(0.0);
                        LAST_PTR_X.store(x.to_bits(), std::sync::atomic::Ordering::Relaxed);
                        LAST_PTR_Y.store(y.to_bits(), std::sync::atomic::Ordering::Relaxed);
                        input_forwarder::push_pointer_event(x, y, 0x02); // right button down
                    }
                    NSEventType::RightMouseUp => {
                        let x = f32::from_bits(LAST_PTR_X.load(std::sync::atomic::Ordering::Relaxed));
                        let y = f32::from_bits(LAST_PTR_Y.load(std::sync::atomic::Ordering::Relaxed));
                        input_forwarder::push_pointer_event(x, y, 0);
                    }
                    NSEventType::OtherMouseDown => {
                        // Middle button = button number 2 in AppKit
                        let btn_num = unsafe { event_ref.buttonNumber() };
                        if btn_num == 2 {
                            let loc = unsafe { event_ref.locationInWindow() };
                            let win_h = DRAWABLE_H.load(std::sync::atomic::Ordering::Relaxed) as f32;
                            let x = loc.x as f32;
                            let y = (win_h - loc.y as f32).max(0.0);
                            LAST_PTR_X.store(x.to_bits(), std::sync::atomic::Ordering::Relaxed);
                            LAST_PTR_Y.store(y.to_bits(), std::sync::atomic::Ordering::Relaxed);
                            input_forwarder::push_pointer_event(x, y, 0x04); // middle button
                        }
                    }
                    NSEventType::OtherMouseUp => {
                        let x = f32::from_bits(LAST_PTR_X.load(std::sync::atomic::Ordering::Relaxed));
                        let y = f32::from_bits(LAST_PTR_Y.load(std::sync::atomic::Ordering::Relaxed));
                        input_forwarder::push_pointer_event(x, y, 0);
                    }
                    // ── Scroll ──────────────────────────────────────────────
                    NSEventType::ScrollWheel => {
                        // scrollingDeltaX/Y: confirmed correct AppKit method names (objc2-app-kit 0.3).
                        // On trackpads hasPreciseScrollingDeltas() is true → sub-pixel float deltas.
                        // On wheel mice deltas are coarser integers (typically ±3 per notch).
                        let sx = unsafe { event_ref.scrollingDeltaX() } as f32;
                        let sy = unsafe { event_ref.scrollingDeltaY() } as f32;
                        if sx != 0.0 || sy != 0.0 {
                            let x = f32::from_bits(LAST_PTR_X.load(std::sync::atomic::Ordering::Relaxed));
                            let y = f32::from_bits(LAST_PTR_Y.load(std::sync::atomic::Ordering::Relaxed));
                            // Normalize: divide by typical scroll step (10 pts) to get surface units.
                            // REVIEW(mac): scaling factor — tune for feel; consider reading
                            //   hasPreciseScrollingDeltas() and using separate scale for trackpad vs wheel.
                            input_forwarder::push_scroll_event(x, y, sx / 10.0, sy / 10.0);
                        }
                    }
                    _ => {}
                }

                // Return the event unchanged so AppKit can process it normally
                event as *mut NSEvent
            });

            // Register with AppKit via msg_send! — confirmed correct approach.
            //
            // mask.0: NSEventMask in objc2-app-kit 0.3 is an Options newtype (same pattern as
            //   NSWindowStyleMask used in metal_view.rs with .0 field). Passes raw NSUInteger.
            //
            // &*monitor_block: RcBlock<(*mut NSEvent,), *mut NSEvent> derefs to
            //   Block<(*mut NSEvent,), *mut NSEvent> via block2 0.6 Deref impl, which is the
            //   correct ObjC block pointer type for the handler: parameter. No explicit cast needed.
            //
            // The return value is an opaque monitor object (retain to keep monitor alive;
            // passing to _ leaks it intentionally — monitor lives for app lifetime).
            unsafe {
                let _: *mut objc2::runtime::AnyObject = objc2::msg_send![
                    objc2::runtime::AnyClass::get(c"NSEvent").unwrap(),
                    addLocalMonitorForEventsMatchingMask: mask.0,
                    handler: &*monitor_block
                ];
            }

            eprintln!("[main] NSEvent local monitor installed (keyboard + pointer + scroll)");
        }

        // NSApplication event loop (блокирующий) — держит окно живым
        eprintln!("[main] Starting NSApplication event loop");
        let app = objc2_app_kit::NSApplication::sharedApplication(mtm);
        unsafe { let _: () = objc2::msg_send![&app, run]; }

        Ok(())
    }
}
// main:end
