Fixture: uv98-plus-stock-1.16.bin
Length: 4096 bytes
SHA-256: 8906c3d9847eeeefe0ee6aad6aa4302949bd5e5984f3c64ed02edb8cf6fb0ff7

Copied without modification from:
outputs/uv98-backup-2026-11-08/calibration-read-01/raw.bin
in the parent Codex workspace on 2026-11-08.

This is one user's backed-up iRadio UV98 Plus stock 1.16 tuning block,
not a universal calibration image or firmware. Do not write it to another radio.
The fixture is local test evidence, not a proposed redistributable release asset.

Golden values were checked against the separate offline Python decode:
marker=0x9A; slot0 frequency-offset stored code=27; slot1=65;
ADC correction raw=128; wide TX-deviation raw=0x34E2, masked code=1250.
These codes do not establish physical frequency, power, or RF calibration behavior.

SHA-256 is provenance documentation, not a runtime hashing dependency.
The Rust parser checks only size and marker; it cannot detect all corruptions.
