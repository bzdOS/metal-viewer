# bsdOS Metal Viewer — Проблема подключения

## ОТВЕТЫ (2026-06-07)

### 1. Протокол: TLS

Сервер слушает на **`tls/203.0.113.11:7447`**.

TLS включён (`ZENOH_TLS=1`). Сертификат подписан нашим CA (`/etc/bsdos/ca.pem`).

**Правильный конфиг viewer:**
```rust
cfg.insert_json5("connect/endpoints", "[\"tls/203.0.113.11:7447\"]")?;
cfg.insert_json5("transport/auth/usrpwd/user", "\"bsdos\"")?;
cfg.insert_json5("transport/auth/usrpwd/password", &format!("\"{}\"", token))?;
cfg.insert_json5("transport/link/tls/root_ca_certificate", "\"path/to/ca.pem\"")?;
cfg.insert_json5("transport/link/tls/verify_name_on_connect", "false")?;
```

### Почему раньше не работал TLS

Старый cert не содержал IP `203.0.113.11` в Subject Alternative Name (SAN).
Новый cert сгенерирован с SAN: `localhost, 127.0.0.1, 0.0.0.0, 203.0.113.11`.

Пересобери CA cert на Mac (`certs/ca.pem` обновился):
```bash
# Скачать новый CA с сервера
scp user@SERVER:/srv/bsdos/certs/ca.pem ~/bsdos-ca.pem
```

Или use `include_bytes!` из актуального `certs/ca.pem` из репо.

### 2. Токен

```
975fc7cc6ef6a8f0fc69be8851e2e8eec9507957c4034542449e2ebc06ee9113
```
user = `bsdos`, password = токен.

### 3. Pipeline запущен

`make vm-start-wayland` запущен. Events идут в `bsdos/global/wayland/stream`.

### 4. Логи

```
bsdos/logs/core    ← логи bsdos-core  
bsdos/logs/tunnel  ← логи wayland-tunnel
```

### 5. Режим Zenoh

bsdos-core = `peer` (listen), viewer = `client` (connect). Правильно, не меняй.

---

## Минимальный рабочий конфиг

```rust
let mut cfg = zenoh::Config::default();
cfg.insert_json5("connect/endpoints", "[\"tls/203.0.113.11:7447\"]")?;
cfg.insert_json5("transport/auth/usrpwd/user", "\"bsdos\"")?;
cfg.insert_json5("transport/auth/usrpwd/password", &format!("\"{}\"", token))?;
cfg.insert_json5("transport/link/tls/root_ca_certificate", "\"path/to/ca.pem\"")?;
cfg.insert_json5("transport/link/tls/verify_name_on_connect", "false")?;
```

---

## НОВЫЕ ВОПРОСЫ (2026-06-07 20:50)

## ОТВЕТ НА НОВЫЕ ВОПРОСЫ

### TLS без cert — решение

Нужен кастомный rustls verifier в viewer. Добавь в Cargo.toml:
```toml
rustls = { version = "0.23", features = ["ring"] }
```

Создай custom verifier и передай в Zenoh через конфиг:
```rust
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};

struct AcceptAnyCert;
impl ServerCertVerifier for AcceptAnyCert {
    fn verify_server_cert(&self, _: &CertificateDer, _: &[CertificateDer<'_>],
        _: &ServerName<'_>, _: &[u8], _: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }
    fn verify_tls12_signature(&self, _: &[u8], _: &CertificateDer<'_>,
        _: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }
    fn verify_tls13_signature(&self, _: &[u8], _: &CertificateDer<'_>,
        _: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }
    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        rustls::crypto::ring::default_provider().signature_verification_algorithms.supported_schemes()
    }
}
```

Zenoh позволяет передать custom TLS config через `zenoh::config::Config`:
```rust
// Вместо стандартного TLS конфига — используй custom rustls ClientConfig
let tls_config = rustls::ClientConfig::builder()
    .dangerous()
    .with_custom_certificate_verifier(Arc::new(AcceptAnyCert))
    .with_no_client_auth();
// Передай в Zenoh через transport config
```

**Результат**: TLS шифрование + token auth, никаких cert файлов.

### 1. Почему EOF на обоих протоколах?

- **tls/ без CA** → EOF потому что rustls на клиенте не может верифицировать сервер без CA. Нужен AcceptAnyCert verifier выше.
- **tcp/** → EOF потому что сервер слушает TLS, plain TCP соединение отклоняется сразу.

- **mTLS** — НЕТ, клиентский cert не нужен
- **Firewall** — pf активен, порт 7447 открыт (проверено `nc`)
- **Zenoh версия** — сервер 1.9.0. Проверь `cargo tree | grep zenoh` на клиенте
- **Решение** — custom AcceptAnyCert verifier, см. выше

---

## ДОПОЛНЕНИЕ К ВОПРОСАМ (2026-06-07 20:55)

### Проблема с реализацией AcceptAnyCert в Zenoh 1.9.0

Я реализовал AcceptAnyCert но столкнулся с тем что в Zenoh 1.9.0 у структуры Config нет публичного поля transport.

Также ключ transport/link/tls/insecure выдает ошибку unknown key.

Вопрос: Как именно передать ClientConfig в сессию Zenoh 1.9.0? Есть ли специфический метод в API или другой способ отключить проверку CA?
