// START_AI_HEADER
// MODULE: mac-companion/wayland-client/src/main.rs
// PURPOSE: Mac-side Wayland tunnel client — subscribes to Zenoh Wayland stream and replays to local compositor
// INTENT: Provides remote Wayland application display on macOS via Zenoh tunnel with length-prefix support
// DEPENDENCIES: tokio, zenoh, libc
// PUBLIC_API: main
// END_AI_HEADER

// bsdos-wayland-client: Mac-side Wayland tunnel client
//
// Подписывается на bsdos/global/wayland/stream (Zenoh)
// Каждый WaylandPacket → реплеит в локальный Wayland compositor
//
// Формат (length-prefix):
//   [4B length u32 LE][payload]
//   payload = [16B WaylandPacket header][args]
//
// Legacy (без length-prefix): данные начинаются с WaylandPacket header

use std::env;
use tokio::net::UnixStream;
use tokio::io::AsyncWriteExt;

const STREAM_TOPIC: &str = "bsdos/global/wayland/stream";
const INPUT_TOPIC: &str = "bsdos/input/keyboard";

// get_wayland_socket:start
//   purpose: Construct the local Wayland compositor socket path from environment variables
//   input:  none (reads XDG_RUNTIME_DIR and WAYLAND_DISPLAY env vars)
//   output: String — full path to the Wayland compositor socket
//   sideEffects: reads environment variables, calls libc::getuid
fn get_wayland_socket() -> String {
    let runtime_dir = env::var("XDG_RUNTIME_DIR")
        .unwrap_or_else(|_| format!("/run/user/{}", unsafe { libc::getuid() }));
    let display = env::var("WAYLAND_DISPLAY")
        .unwrap_or_else(|_| "wayland-0".to_string());
    format!("{}/{}", runtime_dir, display)
}
// get_wayland_socket:end

/// Detect if data uses length-prefix framing
// is_length_prefixed:start
//   purpose: Detect if incoming data uses length-prefix framing by checking the declared length field
//   input:  data: &[u8] — raw bytes starting with potential 4-byte length prefix
//   output: bool — true if data appears to be length-prefixed
//   sideEffects: none
fn is_length_prefixed(data: &[u8]) -> bool {
    if data.len() < 8 {
        return false;
    }
    let declared = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
    declared > 0 && declared <= data.len() - 4 && declared >= 16
}
// is_length_prefixed:end

/// Strip length-prefix if present, return payload slice
fn strip_length_prefix<'a>(data: &'a [u8]) -> &'a [u8] {
    if is_length_prefixed(data) {
        let len = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
        let end = (4 + len).min(data.len());
        &data[4..end]
    } else {
        data
    }
}

// build_zenoh_config:start
//   purpose: Build a Zenoh configuration with peer endpoint, optional token auth, and TLS settings
//   input:  peer_str: &str — Zenoh peer endpoint (tcp/ or tls/ prefix)
//   output: Result<zenoh::Config, Box<dyn std::error::Error + Send + Sync>> — configured Zenoh session config
//   sideEffects: reads env vars (BSDOS_TOKEN, ZENOH_TLS, ZENOH_TLS_CA), configures TLS and auth
fn build_zenoh_config(peer_str: &str) -> Result<zenoh::Config, Box<dyn std::error::Error + Send + Sync>> {
    let mut config = zenoh::Config::default();
    config.insert_json5("connect/endpoints", &format!("[\"{}\"]", peer_str))
        .map_err(|e| format!("Config endpoints: {}", e))?;

    // Token auth
    if let Ok(token) = env::var("BSDOS_TOKEN") {
        config.insert_json5("auth/username", "\"bsdos\"")
            .map_err(|e| format!("Auth user config: {}", e))?;
        config.insert_json5("auth/password", &format!("\"{}\"", token))
            .map_err(|e| format!("Auth pass config: {}", e))?;
        eprintln!("[wl-client] Token auth enabled");
    }

    // TLS
    let tls_enabled = env::var("ZENOH_TLS")
        .map(|v| v != "0")
        .unwrap_or_else(|_| peer_str.starts_with("tls/"));

    if tls_enabled {
        let ca_cert = env::var("ZENOH_TLS_CA")
            .unwrap_or_else(|_| "/etc/bsdos/ca.pem".to_string());
        config.insert_json5("transport/link/tls/root_ca_certificate",
            &format!("\"{}\"", ca_cert))
            .map_err(|e| format!("TLS CA config: {}", e))?;
        config.insert_json5("transport/link/tls/server_name_verification", "false")
            .map_err(|e| format!("TLS config: {}", e))?;
        eprintln!("[wl-client] TLS mode (CA={})", ca_cert);
    } else {
        eprintln!("[wl-client] Plain TCP mode (endpoint={})", peer_str);
    }

    Ok(config)
}
// build_zenoh_config:end

#[tokio::main]
// main:start
//   purpose: Connect to local Wayland compositor, open Zenoh session, subscribe to stream topic, relay packets
//   input:  none (reads BSDOS_PEER/ZENOH_PEER env vars for Zenoh endpoint)
//   output: Result<(), Box<dyn std::error::Error + Send + Sync>> — Ok on exit or error
//   sideEffects: connects to Wayland socket, opens Zenoh session, writes Wayland messages to local compositor
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let peer = env::var("BSDOS_PEER")
        .or_else(|_| env::var("ZENOH_PEER"))
        .unwrap_or_else(|_| "tcp/localhost:7447".to_string());

    let peer_str = if peer.starts_with("tcp/") || peer.starts_with("tls/") {
        peer.clone()
    } else {
        format!("tcp/{}", peer)
    };

    let wayland_socket = get_wayland_socket();

    eprintln!("[wl-client] Zenoh peer: {}", peer_str);
    eprintln!("[wl-client] Wayland socket: {}", wayland_socket);
    eprintln!("[wl-client] Topics: {} (rx) + {} (tx)", STREAM_TOPIC, INPUT_TOPIC);

    let wl_stream = UnixStream::connect(&wayland_socket).await
        .map_err(|e| format!("Cannot connect to Wayland compositor {}: {}", wayland_socket, e))?;
    eprintln!("[wl-client] Connected to local Wayland compositor");

    let config = build_zenoh_config(&peer_str)?;
    let session = zenoh::open(config).await?;

    let sub = session.declare_subscriber(STREAM_TOPIC).await?;
    eprintln!("[wl-client] Subscribed. Waiting for frames...");

    let mut wl_writer = wl_stream;
    let mut packet_count = 0u64;

    while let Ok(sample) = sub.recv_async().await {
        let payload_bytes = sample.payload().to_bytes();
        let raw = payload_bytes.as_ref();

        // Strip length-prefix if present
        let data = strip_length_prefix(raw);

        if data.len() < 16 {
            continue;
        }

        // Parse WaylandPacket header
        let obj_id = u32::from_le_bytes(data[4..8].try_into()?);
        let op_code = u16::from_le_bytes(data[8..10].try_into()?);
        let payload_len = u32::from_le_bytes(data[12..16].try_into()?) as usize;

        // Reconstruct Wayland wire format: [obj_id u32][((size<<16)|opcode) u32][args]
        let wire_size = (8 + payload_len) as u16;
        let mut wire_header = [0u8; 8];
        wire_header[0..4].copy_from_slice(&obj_id.to_le_bytes());
        let size_op = ((wire_size as u32) << 16) | (op_code as u32);
        wire_header[4..8].copy_from_slice(&size_op.to_le_bytes());

        wl_writer.write_all(&wire_header).await?;
        if payload_len > 0 && data.len() >= 16 + payload_len {
            wl_writer.write_all(&data[16..16 + payload_len]).await?;
        }

        packet_count += 1;
        if packet_count % 1000 == 0 {
            eprintln!("[wl-client] {} packets relayed", packet_count);
        }
    }

    Ok(())
}
// main:end
