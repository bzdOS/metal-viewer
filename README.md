# bsdOS Metal Viewer

macOS Metal рендерер для bsdOS Wayland display stream.

**Status:** Ready for development (MVP complete)

- ✅ Zenoh subscriber for WaylandPacket stream
- ✅ Metal GPU texture creation
- ✅ NSWindow + MTKView UI
- ✅ RGBA32 pixel format support
- ✅ Frame buffering & synchronization

## Архитектура

```
bsdOS VM (FreeBSD)
  → cage (Wayland compositor)
    → wayland-tunnel (proxy to WaylandPacket)
      → bsdos-core (Zenoh bridge)
        → Zenoh bsdos/global/wayland/stream (WaylandPacket)

Zenoh peer (localhost:7447 forward via SSH tunnel)
  → bsdos-metal-viewer (Rust + Objective-C)
    → Decode WaylandPacket
      → Replay on local Wayland compositor
        → macOS display
```

## Требования

- macOS 11.0+ (Big Sur)
- Rust 1.70+
- Xcode Command Line Tools (для objc2)
- SSH tunnel до bsdOS VM:
  ```bash
  ssh -L 7447:127.0.0.1:7447 user@bsdos-host
  ```

## Сборка

```bash
cd /root/bsdOS/mac-companion/metal-viewer
cargo build --release
```

Бинарь: `target/release/bsdos-metal-viewer`

## Запуск

### 1. Запустить SSH tunnel (в отдельном терминале)

```bash
# Forward Zenoh peer от VM на локальный 7447
ssh -L 7447:127.0.0.1:7447 freebsd@bsdos-host
```

### 2. Запустить Wayland pipeline на VM

```bash
make vm-start-wayland
```

Это запустит:
- `seatd` для управления устройствами ввода
- `cage` (Wayland compositor)
- `wayland-tunnel` (proxy для WaylandPacket)
- `bsdos-core` (Zenoh bridge)

### 3. Запустить Metal viewer на Mac

```bash
ZENOH_PEER=tcp/localhost:7447 ./target/release/bsdos-metal-viewer
```

Окно откроется автоматически и будет показывать WaylandPacket поток с VM.

## Интеграция с Makefile

На хосте (Linux):

```bash
# Запустить Wayland pipeline на VM
make vm-start-wayland

# Проверить логи
make vm-ssh
tail -f /tmp/wayland-tunnel.log
```

На Mac (отдельная сборка):

```bash
# Из mac-companion/metal-viewer/
cargo build --release
ZENOH_PEER=tcp/localhost:7447 ./target/release/bsdos-metal-viewer
```

## Формат пакетов

Zenoh topic: `bsdos/global/wayland/stream`

WaylandPacket (binary):
```
[0..4]    : msgId (u32, little-endian)
[4..8]    : objId (u32, little-endian)
[8..10]   : opCode (u16, little-endian)
[10..12]  : padding
[12..16]  : payloadLen (u32, little-endian)
[16..]    : Wayland wire protocol args
```

Пример (wl_surface.commit):
- Header size: 16 bytes
- Payload: variable (0 for commit)
- Total: 16+ bytes

## Troubleshooting

### "Subscribed to bsdos/global/wayland/stream" но пакетов нет

1. Проверить, запущены ли wayland-tunnel и bsdos-core:
   ```bash
   make vm-ssh
   ps aux | grep "wayland-tunnel\|bsdos-core"
   ```

2. Проверить, работает ли cage/Wayland:
   ```bash
   echo $WAYLAND_DISPLAY
   # Should print: ghost-0
   ```

3. Проверить логи:
   ```bash
   tail -50 /tmp/wayland-tunnel.log
   tail -50 /tmp/bsdos-core.log
   ```

4. Проверить, доступен ли Zenoh:
   ```bash
   ZENOH_PEER=tcp/localhost:7447 cargo run --release -- --help
   # Should not error about connection
   ```

### Медленное обновление или зависания

- Вероятно, недостаточно пропускной способности SSH tunnel
- Попробовать compression: `ssh -C -L ...`
- Или использовать более быстрый транспорт (Wireguard VPN)

### Metal device error

- Убедитесь, что запуск на macOS с Metal support
- Проверить: `system_profiler SPDisplaysDataType | grep Metal`

## Статус реализации

- ✓ Zenoh subscriber для `bsdos/global/wayland/stream`
- ✓ MTKView + NSWindow инициализация
- ⏳ WaylandPacket декодирование
- ⏳ Replay на локальный Wayland compositor
- ⏳ Input echo (Mac → VM через Zenoh)
- ⏳ Оптимизация (packet buffering, latency tuning)

## Будущее

- WaylandPacket latency statistics in title bar
- Window resizing → scale WaylandPacket viewport
- Input forwarding optimization (deduplicate pointer motion)
- Multi-monitor support (wl_output protocol)
- Wayland subsurface composition
