# Patch Progress: Zenoh TLS Certificate Verification Skip Option

## Status (2026-06-08): COMPLETED

Successfully implemented `skip_certificate_verification` option for Zenoh TLS transport to support self-signed certificates in development environments.

## Changes Made

### 1. `zenoh-link-commons-patched/src/tls.rs`
- Added `TLS_SKIP_CERTIFICATE_VERIFICATION` config constant (default: `false`)
- Implemented `NoCertVerifier` struct that accepts all certificates without validation
- Proper warning logs when verification is disabled

### 2. `zenoh-link-tls-patched/src/utils.rs`
- Export `NoCertVerifier` from `zenoh_link_commons`
- Modified `TlsClientConfig::new()` to use `NoCertVerifier` when `skip_certificate_verification=true`
- Updated certificate verification logic priority:
  1. If `skip_certificate_verification=true`: use `NoCertVerifier` (accepts any cert)
  2. Else if `verify_name_on_connect=false`: use `WebPkiVerifierAnyServerName` (validates CA, ignores name)
  3. Else: standard rustls verification (validates CA + name)

### 3. `zenoh-link-tls-patched/Cargo.toml`
- Updated `zenoh-link-commons` dependency to use local patched version

### 4. `Cargo.toml` (workspace root)
- Added `[patch.crates-io]` entries for `zenoh-link-tls` and `zenoh-link-commons`

### 5. `mac-companion/metal-viewer/src/main.rs`
- Removed temporary `AcceptAnyCert` workaround code
- Updated `build_zenoh_config()` to use new `skip_certificate_verification` option
- Set via environment variable `SKIP_CERT_VERIFICATION` or config JSON5

## Usage

```bash
# Enable skip_certificate_verification (development only!)
export SKIP_CERT_VERIFICATION=1
./bsdos-metal-viewer
```

Or programmatically:
```json5
{
  "transport": {
    "link": {
      "tls": {
        "skip_certificate_verification": true,
        "verify_name_on_connect": false
      }
    }
  }
}
```

## Upstream Pull Request Proposal

### Title: Add skip_certificate_verification option for TLS transport (development/testing)

### Summary:
Adds `skip_certificate_verification` configuration option to `zenoh-link-tls` for development environments using self-signed certificates. This is a safer alternative to manually configuring custom verifiers.

### Files to Modify in Upstream:
1. `zenoh-link-commons/src/tls.rs` - Add `NoCertVerifier` struct and config constant
2. `zenoh-link-tls/src/utils.rs` - Integrate `NoCertVerifier` into client config logic
3. `zenoh-config/src/...` - Add `skip_certificate_verification: bool` field to `TLSConf`

### Security Note:
This option is **disabled by default** and logs a prominent warning when enabled. It should NEVER be used in production environments.

### Testing:
```bash
# Server with self-signed cert
zenohd --tls --listen-port 7447

# Client with skip verification
zenoh --mode client --connect tls/localhost:7447 \
  --config '{"transport":{"link":{"tls":{"skip_certificate_verification":true}}}}'
```
