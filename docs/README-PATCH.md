# Zenoh TLS Patch: skip_certificate_verification

This directory contains patched versions of `zenoh-link-tls` and `zenoh-link-commons` that add the `skip_certificate_verification` configuration option.

## What This Patch Does

The patch adds a new configuration option `skip_certificate_verification` to the Zenoh TLS transport. When enabled, the client will accept **any** TLS certificate, including:
- Self-signed certificates
- Expired certificates
- Certificates with invalid names
- Certificates from unknown CAs

## Security Warning

**⚠️ NEVER USE THIS IN PRODUCTION!**

This option completely disables TLS certificate verification, leaving you vulnerable to man-in-the-middle attacks. It should ONLY be used for:
- Development/testing environments
- Isolated networks under your control
- Situations where you have other means of verifying the endpoint

## How to Use

### Environment Variable
```bash
export SKIP_CERT_VERIFICATION=1
./your-zenoh-app
```

### Programmatic Configuration
```rust
let mut cfg = zenoh::Config::default();
cfg.insert_json5(
    "transport/link/tls/skip_certificate_verification",
    "true"
)?;
```

### JSON5 Configuration File
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

## Files Structure

```
mac-companion/
├── zenoh-link-commons-patched/    # Patched commons crate
│   └── src/tls.rs                 # Added NoCertVerifier struct
├── zenoh-link-tls-patched/        # Patched TLS link crate
│   ├── src/utils.rs               # Integration of NoCertVerifier
│   └── Cargo.toml                 # Local commons dependency
└── metal-viewer/                  # Example usage
    └── src/main.rs                # build_zenoh_config()
```

## Upstream Contribution

This patch is designed to be contributed back to the Eclipse Zenoh project. See PATCH-PROGRESS.md for details on the proposed upstream pull request.

## Verification Priority

The certificate verification logic follows this priority:

1. **skip_certificate_verification=true** → Accept any certificate (NoCertVerifier)
2. **verify_name_on_connect=false** → Verify CA but ignore server name (WebPkiVerifierAnyServerName)
3. **Default** → Full verification (CA + name + expiration)

## License

This patch maintains the same license as the original Zenoh project: EPL-2.0 OR Apache-2.0
