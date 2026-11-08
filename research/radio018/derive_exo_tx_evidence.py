"""Original stock TX body with the preserved EXO Rep record/calibration.

Offline only: bounded Unicorn, RAM and RF/delay test doubles. No COM/MMIO,
no device writes, no full firmware boot, no mutation of preserved inputs.
"""
import hashlib
import json
import struct
from pathlib import Path

from derive_oracle import HERE, PROJECT, IMAGE, setup, save


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    cal_path = PROJECT / "crates/calibration/tests/fixtures/uv98-plus-stock-1.16.bin"
    cps_path = Path("C:/Users/User/Desktop/USB/999.880dmf")
    expected = {
        cal_path: "8906c3d9847eeeefe0ee6aad6aa4302949bd5e5984f3c64ed02edb8cf6fb0ff7",
        cps_path: "94b90ced67137d30ade3ee4b3443ee316202eaafd0008cd89f1d858d6c1912de",
        IMAGE: "eaef215cc7084dcd94281bbffb7ab6e2dd1e79ce135f01c90e3dbc691dd16bfc",
    }
    for path, digest in expected.items():
        assert sha(path) == digest, str(path)
    cal, cps = cal_path.read_bytes(), cps_path.read_bytes()
    assert len(cal) == 4096 and len(cps) == 221184
    channel = cps[0x1000 + 48:0x1000 + 96]
    frequency = struct.unpack_from("<I", channel, 10)[0]
    tone = struct.unpack_from("<H", channel, 14)[0]
    assert (channel[0], channel[1], channel[3], frequency, tone,
            channel[16], channel[18] >> 5, channel[19] & 15) == (
                0, 0, 0, 43_297_500, 0x1302, 0, 0, 0)
    assert struct.unpack_from("<I", channel, 6)[0] == frequency
    assert struct.unpack_from("<H", channel, 4)[0] == 0
    assert cps[0xad] == 31
    m, events, regs, block = setup()
    # setup's NOR callback captures this bytearray; replace its synthetic data.
    block[:] = cal
    m.write(0x20003044, cal[:32])
    m.write(0x200035ec, cps[:4096])
    m.write(0x20005000, channel)
    regs[0x33], regs[0x31] = 0, 0x55a0
    m.call(0x080053f0)
    band = m.read(0x2000081a, 1)[0]
    assert band == 1
    assert events == [
        ["read", "A", 0x33, 0], ["write", "A", 0x33, 0],
        ["write", "A", 0x47, 0x6140],
        ["write", "A", 0x38, 0xab1d], ["write", "A", 0x39, 0x0294],
        ["gpio", "F", 0, 0],
        ["write", "A", 0x51, 0x9041], ["write", "A", 0x07, 0x0636],
        ["write", "A", 0x71, 0x68dc],
        ["read", "A", 0x31, 0x55a0], ["write", "A", 0x31, 0x55a0],
        ["write", "A", 0x74, 0xe61c], ["write", "A", 0x7d, 0xe95f],
        ["write", "A", 0x40, 0x34e2], ["write", "A", 0x37, 0x1d0f],
        ["delay", "ms", 0, 10], ["write", "A", 0x52, 0x028f],
        ["write", "A", 0x30, 0x0200], ["write", "A", 0x30, 0xc1fe],
        ["gpio", "C", 15, 0], ["gpio", "B", 11, 0],
        ["read", "A", 0x33, 0], ["write", "A", 0x33, 4],
        ["write", "A", 0x36, 0x5cff],
    ]
    corrected = (regs[0x39] << 16) | regs[0x38]
    assert corrected == frequency + 65
    save("exo-tx-stock.csv", ["step", "kind", "target", "register", "value"],
         [[i, *event] for i, event in enumerate(events)])
    source_files = ["firmware-diagnostic/src/main.rs", "crates/radio-runtime/src/lib.rs",
                    "crates/radio-runtime/src/ptt.rs", "crates/radio-runtime/src/policy.rs",
                    "crates/rf-sequence/src/lib.rs", "crates/diag-startup/src/pre_init.s"]
    report = dict(requirement="R-09", reference_function="080053F0..080055C2",
                  hardware_accessed=False, mmio_mapped=False, events=len(events),
                  channel_index=1, label="EXO Rep", relay_identity_confirmed=False,
                  frequency_10hz=frequency, corrected_frequency_code=corrected,
                  calibration_band=band, power_mode=0, pa_code=23, pa_register="5CFF",
                  mic_cfg=31, tx_tone_raw="1302", tx_ctcss_hz=77.0,
                  calibration_sha256=expected[cal_path], cps_sha256=expected[cps_path],
                  reference_sha256=expected[IMAGE],
                  csv_sha256=sha(HERE / "exo-tx-stock.csv"),
                  source_snapshot_sha256={name: sha(PROJECT / name) for name in source_files},
                  limits="Original Thumb ordinary TX branch with preserved factory/CFG/channel bytes. RF/delay/GPIO are test doubles; input A33=0,A31=55A0. No RF voltage, watts, modulation, relay identity or physical shutdown proof. Rust's additional off-prefix/width/readback must remain separately tested; this CSV does not execute main-loop/PTT/watchdog.")
    for path, digest in expected.items():
        assert sha(path) == digest, "preserved input changed: " + str(path)
    (HERE / "exo-tx-evidence.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(dict(status="PASS", events=24, band=band, pa_code=23,
                          register36="5CFF", inputs_unchanged=True, hardware_accessed=False)))


if __name__ == "__main__":
    main()
