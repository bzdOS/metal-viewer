# ТЗ: bsdos-metal-viewer v0.2 (Mac клиент)

## Цель

Превратить dev-скрипт в используемое приложение. Пользователь вводит IP + токен один раз, дальше всё прозрачно.

---

## 1. Length-prefix protocol (КРИТИЧНО — фиксит надёжность)

**Текущая проблема:** 128KB cap, `early eof` баги из-за отсутствия явного framing.

**Решение:** 4-байтный LE length-prefix перед каждым сообщением на stream socket и в Zenoh.

Формат пакета:
```
[length: u32 LE][payload: N bytes]
payload = [24-byte tunnel header][pixels]
```

Стороны:
- `wayland-tunnel` (Zig): пишет length-prefix перед каждым frame
- `bsdos-core` (Rust): читает length-prefix, затем ровно N байт
- `bsdos-metal-viewer` (Rust/Mac): читает length-prefix из Zenoh payload

Приоритет: **P0** — без этого всё остальное ненадёжно.

---

## 2. Input forwarding

**Отсутствует.** Нужно замкнуть цикл: видим картинку → можем взаимодействовать.

### 2.1 Keyboard

Zenoh key: `bsdos/input/keyboard`

Формат (Cap'n Proto или простой binary):
```
struct KeyEvent {
  key_code: u32,   // Linux evdev key code
  action: u8,      // 0=up, 1=down, 2=repeat
  modifiers: u8,   // ctrl/shift/alt/meta bits
  timestamp_ms: u64,
}
```

Сторона VM: wayland-tunnel читает `bsdos/input/keyboard`, инжектирует через Wayland `wl_keyboard.key` на активную поверхность.

### 2.2 Pointer (mouse/trackpad)

Zenoh key: `bsdos/input/pointer`

```
struct PointerEvent {
  dx: f32, dy: f32,      // relative movement
  buttons: u8,           // bitmask left/right/middle
  scroll_x: f32, scroll_y: f32,
  timestamp_ms: u64,
}
```

### 2.3 Приоритет

| Устройство | Приоритет |
|---|---|
| Keyboard | P1 |
| Pointer (relative) | P1 |
| Scroll | P2 |
| Touch | P3 |

---

## 3. Connection UX

**Сейчас:** `BSDOS_TOKEN=xxx BSDOS_PEER=tls/IP:7447 ./binary`

**Нужно:** При первом запуске (нет сохранённого соединения) показывать окно:

```
┌─────────────────────────────────┐
│  bsdOS Connect                  │
│                                 │
│  Server:  [203.0.113.11     ] │
│  Token:   [················   ] │
│                                 │
│  [Connect]  [Remember]          │
└─────────────────────────────────┘
```

Хранение:
- `~/.config/bsdos/connection.toml` — server IP, token (encrypted via macOS Keychain API)
- CLI override всегда работает через env vars

---

## 4. TLS без раздачи ca.pem

**Вариант A (рекомендуется):** Embed CA в бинарь при сборке:
```rust
const CA_CERT: &[u8] = include_bytes!("../../../certs/ca.pem");
```
Минус: пересборка при смене CA.

**Вариант B:** Trust On First Use (TOFU):
- Первое подключение → показать SHA256 fingerprint сервера
- "Trust? [Yes/No/Always]"
- Сохранить в `~/.config/bsdos/known_servers`

Приоритет: Вариант A (P1), Вариант B (P2).

---

## 5. Рендеринг фрейма (Metal)

**Сейчас:** базовый blit.

**Нужно:**
- Decode: XRGB8888 / ARGB8888 → `MTLPixelFormatBGRA8Unorm` texture
- Scale: letterbox, preserve aspect ratio (VM: 1280×694 → Mac window)
- VSync: Metal display link, не spinloop
- FPS counter в заголовке окна (`bsdOS — 23 fps — 42ms`)

---

## 6. Статус-бар

```
[● 203.0.113.11] [TLS ✓] [Token ●●●●ab] [23 fps] [42ms]
```

Компоненты:
- Иконка соединения (зелёный ●, красный ○)
- IP сервера
- TLS статус
- Первые 4 + последние 2 символа токена (маскировка)
- FPS (moving average 30 frames)
- Round-trip latency (timestamp в пакете → delta при получении)

---

## 7. Архитектура кода

```
mac-companion/metal-viewer/src/
  main.rs           — точка входа, event loop (AppKit/NSApplication)
  connection.rs     — Zenoh setup, token auth, TLS, reconnect loop
  renderer.rs       — Metal pipeline, texture upload, blit
  input.rs          — NSEvent → KeyEvent/PointerEvent → Zenoh publish
  ui/
    connect_dialog.rs — первый запуск
    status_bar.rs     — нижняя строка статуса
```

---

## 8. Приоритеты реализации

| # | Задача | Сложность | Приоритет |
|---|---|---|---|
| 1 | Length-prefix protocol (tunnel + core + viewer) | Low | P0 |
| 2 | Keyboard input forwarding | Medium | P1 |
| 3 | Embed CA cert в бинарь | Low | P1 |
| 4 | Pointer input forwarding | Medium | P1 |
| 5 | FPS/latency display | Low | P2 |
| 6 | Metal rendering (VSync, proper scale) | Medium | P2 |
| 7 | Connection dialog | Medium | P2 |
| 8 | Keychain storage | Low | P3 |
| 9 | TOFU cert pinning | Medium | P3 |

---

## 9. Зависимости от VM стороны

Для input forwarding нужна реализация на FreeBSD:
- `wayland-tunnel` подписывается на `bsdos/input/*`
- Инжектирует через Wayland protocol в активный клиент (foot, firefox, etc.)
- Или через `uinput` kernel module (если доступен на FreeBSD)

Zenoh keys summary:
```
bsdos/global/wayland/stream  ← frames (VM → Mac)
bsdos/input/keyboard         → key events (Mac → VM)
bsdos/input/pointer          → pointer events (Mac → VM)
bsdos/telemetry              ← HAL metrics (VM → Mac)
```
