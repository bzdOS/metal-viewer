# bsdOS Metal Viewer — Вопросы к серверной стороне

## 1. mTLS: нужен ли клиентский сертификат?

**ОТВЕТ: НЕ нужен.** mTLS отключён на сервере.
- Сервер использует только server-side TLS (CA + server.pem + server.key)
- `enable_mtls = false`
- `certs/client.pem` и `certs/client.key` в репозитории не используются
- Аутентификация только через usrpwd токен

---

## 2. Актуальный токен

**ОТВЕТ: Актуальный токен:**
```
975fc7cc6ef6a8f0fc69be8851e2e8eec9507957c4034542449e2ebc06ee9113
```

Токен теперь **постоянный** — не меняется при перезапуске pipeline.
Меняется только при `make zenoh-rotate-token` (явная ротация).
Всегда актуальный: `make zenoh-token` на сервере или смотреть CONNECT.md.

---

## 3. Текущий статус pipeline

**ОТВЕТ: Pipeline запущен.** События идут в Zenoh:

- `bsdos/global/wayland/stream` → POOL_DATA (0x03) + SURFACE_COMMIT (0x04)
- `bsdos/health` → `{"ts":N,"ok":true}` каждую секунду
- `bsdos/logs/core` → логи bsdos-core в реальном времени
- `bsdos/logs/tunnel` → логи wayland-tunnel в реальном времени

Формат протокола — новый (Wayland Protocol Events, не pixel frames).
Подробно: `github.com/bzdOS/WLStream (spec)` и `CONNECT.md`.

**Важно**: добавь `lz4_flex = "0.11"` в Cargo.toml — LZ4 сжатие активно.
Без этого POOL_DATA содержит сжатые данные, viewer получит мусор вместо пикселей.

---

## 4. Режим Zenoh

**ОТВЕТ: Правильно.** Не меняй.

- bsdos-core = `peer` mode (у него есть `listen/endpoints`)
- viewer = `client` mode (подключается к `connect/endpoints`)

Client mode означает: "я подключаюсь к известному peer, не слушаю сам".
Peer mode означает: "я принимаю входящие соединения".

Это стандартная topology для Zenoh: один publisher (peer) + N клиентов (client).

---

## Checklist для первого запуска

- [ ] Добавить `lz4_flex = "0.11"` в Cargo.toml
- [ ] Реализовать LZ4 decompression в handle_pool_data (см. STATUS.md)
- [ ] Убедиться что `connect/endpoints` = `["tls/203.0.113.11:7447"]`
- [ ] Не использовать `mDNS/multicast` — только explicit endpoint
- [ ] Токен: `975fc7cc6ef6a8f0fc69be8851e2e8eec9507957c4034542449e2ebc06ee9113`
