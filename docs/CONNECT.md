# bsdOS Mac Companion — Подключение

## Endpoint

```
tls/203.0.113.11:7447
```


## Токен

Токен **постоянный** — не меняется при перезапуске pipeline.
Меняется только при `make zenoh-rotate-token`.

```
975fc7cc6ef6a8f0fc69be8851e2e8eec9507957c4034542449e2ebc06ee9113
```

Обновлённый токен всегда в `STATUS.md` и в выводе `make vm-start-wayland`.

---

## Быстрый старт

```bash
# 1. Пересобрать (если изменился код или протокол)
cd mac-companion/metal-viewer && cargo build --release

# 2. Запустить
BSDOS_TOKEN=975fc7cc6ef6a8f0fc69be8851e2e8eec9507957c4034542449e2ebc06ee9113 \
BSDOS_PEER=tcp/203.0.113.11:7447 \
./target/release/bsdos-metal-viewer
```

---

## Диагностика

```bash
# Проверить что сервер жив (должен приходить JSON каждую секунду):
# subscribe to bsdos/health
# → {"ts":1749123456,"ok":true}

# Логи bsdos-core в реальном времени:
# subscribe to bsdos/logs/core

# Логи wayland-tunnel:
# subscribe to bsdos/logs/tunnel
```

Если `bsdos/health` не приходит в 5 секунд:
1. Проверь BSDOS_TOKEN — получи актуальный: `make zenoh-token`
2. Проверь `connect/endpoints` — должно быть `tcp/203.0.113.11:7447`
3. Проверь сеть: `nc -z 203.0.113.11 7447`

---

## Zenoh Config (обязательно)

```rust
cfg.insert_json5("connect/endpoints", "[\"tcp/203.0.113.11:7447\"]")?;
cfg.insert_json5("transport/auth/usrpwd/user", "\"bsdos\"")?;
cfg.insert_json5("transport/auth/usrpwd/password", &format!("\"{}\"", token))?;
// CA cert НЕ нужен — токен обеспечивает авторизацию
cfg.insert_json5("transport/link/tls/verify_name_on_connect", "false")?;
```

Без `connect/endpoints` Zenoh уходит в mDNS scouting → "Scouting delay elapsed" → нет соединения.

---

## Zenoh Topics

| Key | Направление | Содержимое |
|---|---|---|
| `bsdos/global/wayland/stream` | ← server | Wayland protocol events (см. ниже) |
| `bsdos/jail/appBrowser/stream` | ← server | Chromium stream |
| `bsdos/health` | ← server | `{"ts":N,"ok":true}` каждую секунду |
| `bsdos/telemetry` | ← server | HAL метрики (Cap'n Proto) |
| `bsdos/logs/core` | ← server | Логи bsdos-core |
| `bsdos/logs/tunnel` | ← server | Логи wayland-tunnel |
| `bsdos/input/keyboard` | → server | KeyEvent `[type:u8][key_code:u32][action:u8][mods:u8]` |
| `bsdos/input/pointer` | → server | PointerEvent `[type:u8][dx:f32][dy:f32][buttons:u8][sx:f32][sy:f32]` |

---

## Stream Protocol (bsdos/global/wayland/stream)

Каждое Zenoh сообщение: `[payload_size: u32 LE][event_type: u8][event data]`

### 0x03 POOL_DATA
```
[pool_id: u32][width: u16][height: u16][stride: u32][format: u32]
[raw_len: u32][lz4_len: u32]
[data: lz4_len bytes]   ← LZ4 если lz4_len < raw_len, иначе raw
```

**ВАЖНО**: если `lz4_len < raw_len` → данные сжаты LZ4, нужен `lz4_flex::decompress(data, raw_len)`.
Добавь в Cargo.toml: `lz4_flex = "0.11"`

```rust
let pixels = if lz4_len < raw_len {
    lz4_flex::decompress(data, raw_len as usize)?
} else {
    data.to_vec()
};
```

### 0x04 SURFACE_COMMIT
```
[surface_id: u32][pool_id: u32][offset: u32]
[width: u16][height: u16][stride: u32][format: u32]
[damage_x: u16][damage_y: u16][damage_w: u16][damage_h: u16]
```
Render: `pool_cache[pool_id][offset..]` as texture, update damage region.

### 0x05 CURSOR_MOVE
```
[x: i32][y: i32]
```

### 0xFE SESSION_RESET
```
[reason: u8][msg_len: u8][msg: bytes]
```
Очисти весь pool_cache, жди новые POOL_DATA.

### 0xFF ERROR
```
[code: u16][msg_len: u8][msg: bytes]
```
Логируй, продолжай работу.

---

## Pixel Format

XRGB8888 (format=1) в памяти: `[B][G][R][X]` per pixel = `MTLPixelFormatBGRA8Unorm`.
stride = 5120 для 1280px (4 bytes/pixel).

---

## Управление токеном

```bash
make zenoh-token           # текущий токен
make zenoh-rotate-token    # новый токен (старые клиенты отваливаются)
make zenoh-cron-rotation   # daily 04:00 автоматическая ротация
```

---

## Сервер (для справки)

- Dev машина: `203.0.113.10` (SSH)
- Zenoh endpoint: `203.0.113.11:7447` (dedicated IP на VM)
- `make vm-start-wayland` — запустить pipeline
- `make vm-start-browser-stream` — второй pipeline для Chromium
- Полный спек протокола: `github.com/bzdOS/WLStream (spec)`
