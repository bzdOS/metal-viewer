# bsdOS Mac Companion — Статус и вопросы

## ВАЖНО: Смена протокола (2026-06-07)

Переходим с pixel frame streaming (screen capture) на Wayland Protocol Forwarding.

### Что изменилось на сервере

Zenoh key `bsdos/global/wayland/stream` теперь содержит:
- **POOL_DATA (0x03)**: wl_shm buffer (пока без LZ4, TODO)
- **SURFACE_COMMIT (0x04)**: информация о commit

Формат каждого Zenoh payload:
```
[payload_size: u32 LE]
[event_type: u8]
[event data: N bytes]
```

### Что нужно изменить в metal-viewer (Mac)

#### 1. Убрать старый pixel frame decoder
- Удалить код который ожидал `[24-byte header][pixels]`
- Убрать length-prefix hack

#### 2. Добавить event dispatcher
```rust
fn handle_wayland_event(data: &[u8]) {
    if data.len() < 5 { return; }
    let payload_size = u32::from_le_bytes(data[0..4].try_into().unwrap()) as usize;
    let event_type = data[4];
    let payload = &data[5..5+payload_size-1];
    
    match event_type {
        0x03 => handle_pool_data(payload),
        0x04 => handle_surface_commit(payload),
        _ => eprintln!("[viewer] unknown event 0x{:02x}", event_type),
    }
}
```

#### 3. POOL_DATA handler (0x03)
```rust
fn handle_pool_data(payload: &[u8]) {
    // Parse:
    let pool_id = u32::from_le_bytes(payload[0..4]);
    let width = u16::from_le_bytes(payload[4..6]);
    let height = u16::from_le_bytes(payload[6..8]);
    let stride = u32::from_le_bytes(payload[8..12]);
    let format = u32::from_le_bytes(payload[12..16]);
    let raw_len = u32::from_le_bytes(payload[16..20]);
    let lz4_len = u32::from_le_bytes(payload[20..24]);
    let compressed_data = &payload[24..24+lz4_len as usize];
    
    // Decompress (TODO: LZ4, пока raw == compressed)
    let pixels = compressed_data; // временно без LZ4
    
    // Cache в HashMap<u32, PoolData>
    POOL_CACHE.insert(pool_id, PoolData { width, height, stride, format, pixels: pixels.to_vec() });
    
    // Upload to MTLTexture
    upload_texture(pool_id, width, height, stride, format, pixels);
}
```

#### 4. SURFACE_COMMIT handler (0x04)
```rust
fn handle_surface_commit(payload: &[u8]) {
    let surface_id = u32::from_le_bytes(payload[0..4]);
    let pool_id = u32::from_le_bytes(payload[4..8]);
    let offset = u32::from_le_bytes(payload[8..12]);
    let width = u16::from_le_bytes(payload[12..14]);
    let height = u16::from_le_bytes(payload[14..16]);
    // damage rect in payload[20..28]
    
    // Render: lookup pool cache, render region to screen
    render_surface(surface_id, pool_id, offset, width, height);
}
```

#### 5. Metal render pipeline
```rust
fn render_surface(surface_id: u32, pool_id: u32, offset: u32, width: u16, height: u16) {
    if let Some(pool) = POOL_CACHE.get(&pool_id) {
        let pixel_data = &pool.pixels[offset as usize..];
        
        // Update MTLTexture region
        texture.replace(
            MTLRegion::new(0, 0, width as u64, height as u64),
            0, // mip level
            pixel_data,
            pool.stride as u64,
        );
        
        // Render to screen using Metal command encoder
        render_to_screen(&texture);
    }
}
```

### Полная спецификация
Читай `/srv/bsdos/github.com/bzdOS/WLStream (spec)` — полный spec протокола.

## Что сделано (2026-06-07 обновлено)

### Вторая Wayland stream инфраструктура (Browser)
- [x] Chromium установлен в FreeBSD VM (phantom headless с CDP на :9222)
- [x] vm-start-browser-stream.sh создан — запускает отдельный pipeline для Chromium:
  - phantom headless (Chromium без X11)
  - wayland-tunnel (второй инстанс) → слушает wayland-ghost-browser
  - bsdos-core (второй инстанс) → публикует на bsdos/jail/appBrowser/stream
- [x] Обновлено vm-setup-phantom.sh чтобы использовать `chrome` вместо `chromium`
- [x] Параметризация:
  - BSDOS_GHOST_SOCK=/tmp/wayland-run/wayland-ghost-browser
  - BSDOS_STREAM_SOCK=/tmp/wayland-run/wayland-stream-browser.sock
  - BSDOS_ZENOH_KEY=bsdos/jail/appBrowser/stream
  - BSDOS_COMPOSITOR_SOCK=/tmp/wayland-run/wayland-1 (для второго tunnel)
- [x] Makefile: добавлены таргеты vm-start-browser-stream
- [x] Оба wayland-tunnel и bsdos-core уже поддерживают параметризацию через env

### metal-viewer
- [x] Сборка на macOS (objc2 0.6, Metal, NSWindow)
- [x] Zenoh 1.9 подключение через TLS + token auth
- [x] Length-prefix protocol (авто-detect, backward compatible)
- [x] Embedded CA cert (include_bytes! → temp file для Zenoh)
- [x] FPS logging (stderr каждые 3 сек)
- [x] Input forwarding module (input.rs) — не интегрирован в main.rs

### wayland-client
- [x] Сборка на macOS
- [x] Length-prefix stripping
- [x] Token auth + TLS config

## Zenoh 1.9 API заметки

Config ключи в Zenoh 1.9 отличаются от документации:
- Auth: `transport/auth/usrpwd/user` + `transport/auth/usrpwd/password`
- TLS CA: `transport/link/tls/root_ca_certificate` = **путь к файлу**, не PEM content
- TLS verify: `transport/link/tls/verify_name_on_connect` (не `server_name_verification`)

## Подключение тест

```
[receiver] ✓ Subscribed to bsdos/global/wayland/stream
[receiver] ✓ Listening (length-prefix + legacy formats)
```

**ТЕКУЩИЙ ТОКЕН** (обновляется при каждом рестарте bsdos-core):
```
975fc7cc6ef6a8f0fc69be8851e2e8eec9507957c4034542449e2ebc06ee9113
```
Актуальный всегда: `make zenoh-token` на сервере.

**Pipeline запущен.** Events идут: 0x03 POOL_DATA (3.5MB) + 0x04 SURFACE_COMMIT (37b).

**Картинки нет потому что**: viewer нужно пересобрать с новыми handlers из секции выше.
- Если viewer уже реализовал handle_pool_data/handle_surface_commit — просто пересобери и запусти с новым токеном
- Если нет — реализуй по коду выше

## Ответы от сервера (2026-06-07)

### 1. Pipeline нужно запускать явно
```bash
make vm-start-wayland   # cage + wayland-tunnel + bsdos-core (на dev машине)
```
После этого bsdos-core начинает публиковать фреймы в Zenoh. Viewer просто ждёт без ошибок — да, ожидаемо.

### 2. Токен пересоздаётся при каждом старте bsdos-core
Генерируется из `/dev/urandom` при старте, сохраняется в `/run/bsdos-access.token`.
Получить актуальный: `make zenoh-token`.
Ротация: `make zenoh-rotate-token`. Авто-ротация по cron: `make zenoh-cron-rotation` (daily 04:00).

### 3. Viewer ждёт без ошибок — ожидаемо ✓
bsdos-core имеет retry loop для подключения к wayland stream socket.
Добавлен heartbeat: `bsdos/health` публикуется каждую секунду — можно подписаться чтобы видеть что сервер жив.

### 4. Input forwarding серверная сторона — ГОТОВА
- **bsdos-core** подписывается на `bsdos/input/keyboard` и `bsdos/input/pointer`
- Пишет события в `/tmp/wayland-run/input.sock` (Unix socket)
- **Формат**: `[type: u8][payload]` где type=0=keyboard, type=1=pointer
- Полный протокол: `wayland-tunnel/INPUT_PROTOCOL.md`
- wayland-tunnel читает из этого socket и должен инжектировать в Wayland — **это следующий шаг** (пока не реализовано в tunnel)

### 5. Wayland Protocol Format — новый формат с энкодингом событий
На серверной стороне wayland-tunnel теперь отправляет структурированные события (не pixel frames).
Формат: `[payload_size: u32 LE][event_type: u8][event data]` с поддержкой SURFACE_CREATE, POOL_DATA, SURFACE_COMMIT, CURSOR_MOVE.
Полная спецификация в `/srv/bsdos/github.com/bzdOS/WLStream (spec)`.

### Heartbeat и логи (новое)

Подпишись для диагностики:
```bash
# Убедиться что сервер жив (JSON каждую секунду):
./bsdos-metal-viewer --sub bsdos/health
# → {"ts":1749123456,"ok":true}

# Логи bsdos-core в реальном времени:
./bsdos-metal-viewer --sub bsdos/logs/core

# Логи wayland-tunnel в реальном времени:
./bsdos-metal-viewer --sub bsdos/logs/tunnel
```

Если `bsdos/health` НЕ идёт → проблема с токеном или сетью.
Если идёт но нет картинки → проблема в рендеринге viewer.

### Текущий статус pipeline
```
tls/203.0.113.11:7447   ← bsdos-core listening (TLS + token auth)
make vm-start-wayland     ← запускает cage + tunnel + foot
bsdos/global/wayland/stream ← Wayland protocol events (SURFACE_CREATE/POOL_DATA/SURFACE_COMMIT/CURSOR_MOVE)
bsdos/health              ← heartbeat JSON {"ts":..., "ok":true} каждую секунду
bsdos/input/keyboard      → принимает KeyEvent (не реализовано в tunnel-стороне)
bsdos/input/pointer       → принимает PointerEvent (не реализовано в tunnel-стороне)
```

## Второй stream (Browser)

```
make vm-start-browser-stream  ← запускает phantom + tunnel + core для Chromium
bsdos/jail/appBrowser/stream  ← pixel frames из Chromium headless
```

**Запуск на Mac:**
```bash
BSDOS_PEER=tls/203.0.113.11:7447 ./bsdos-metal-viewer --sub bsdos/jail/appBrowser/stream
```
