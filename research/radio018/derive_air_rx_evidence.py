"""Factory-data AIR126.4 AM requests and PC4 routing; no MMIO/COM.

These are original Thumb requests, not electrical RF/audio measurements.
"""
import hashlib
import json
import struct
from pathlib import Path

from derive_oracle import HERE, PROJECT, IMAGE, setup, save, GPRS, UC_HOOK_CODE, UC_ARM_REG_PC


def main():
    cal_path = PROJECT / "crates/calibration/tests/fixtures/uv98-plus-stock-1.16.bin"
    cps_path = Path("C:/Users/User/Desktop/USB/999.880dmf")
    expected_hashes = {
        cal_path: "8906c3d9847eeeefe0ee6aad6aa4302949bd5e5984f3c64ed02edb8cf6fb0ff7",
        cps_path: "94b90ced67137d30ade3ee4b3443ee316202eaafd0008cd89f1d858d6c1912de",
        IMAGE: "eaef215cc7084dcd94281bbffb7ab6e2dd1e79ce135f01c90e3dbc691dd16bfc",
    }
    for path, digest in expected_hashes.items():
        assert hashlib.sha256(path.read_bytes()).hexdigest() == digest, str(path)
    cal, cps = cal_path.read_bytes(), cps_path.read_bytes()
    assert len(cal) == 4096 and len(cps) == 221184
    m, events, regs, block = setup()
    block[:] = cal
    m.write(0x20003044, cal[:32])
    m.write(0x200035ec, cps[:4096])
    channel = bytearray(48)
    channel[0], channel[3] = 1, 0  # AM, Wide, tone disabled.
    struct.pack_into("<I", channel, 6, 12_640_000)
    m.write(0x20005000, channel)
    regs[0x33], regs[1] = 0, 6
    for address, args in [(0x080039c0, ()), (0x08004228, (0,)),
                          (0x08004518, ()), (0x08004bf0, ())]:
        m.call(address, *args)
    m.hooks.append(m.uc.hook_add(UC_HOOK_CODE,
        lambda uc, pc, size, _: uc.reg_write(UC_ARM_REG_PC, m.STOP | 1),
        begin=0x0800518e, end=0x0800518e))
    m.call(0x080050ea)
    assert m.read(0x2000081a, 1)[0] == 3
    assert cps[0xb0:0xb2] == bytes([50, 12])
    expected_words = {0x33: 0x0010, 0x38: 0xdf00, 0x39: 0x00c0,
        0x43: 0x3028, 0x47: 0x6740, 0x48: 0xb329, 0x13: 0x03be,
        0x37: 0x1f0f, 0x30: 0xbff1, 0x28: 0x6b38, 0x29: 0xab40}
    for reg, value in expected_words.items():
        assert regs[reg] == value, (reg, regs[reg], value)
    assert [e for e in events if e[0] == "gpio"] == [["gpio", "F", 0, 0]]
    assert not any(e[0] == "write" and e[2] in [0x7e, 0x3d] for e in events)
    save("air-am-factory-stock.csv", ["step", "kind", "target", "register", "value"],
        [[index, *event] for index, event in enumerate(events)])

    pc4_cases = []
    for repeat in [0, 1]:
        for setting107 in [0, 1]:
            m, _, _, _ = setup()
            gpio, helpers = [], []
            m.write(0x200035ec + 0x77, bytes([repeat]))
            m.write(0x200035ec + 0x107, bytes([setting107]))
            for address in [0x0801fc70, 0x0801e2bc]:
                m.intercept(address, lambda e, address=address:
                    helpers.append([hex(address), e.uc.reg_read(GPRS[0])]))
            stores = {0x0800b0ee: (0x40020828, False),
                      0x0800b0fe: (0x40020818, True),
                      0x0800b13c: (0x40020828, False)}
            def capture(uc, pc, size, _):
                address, high = stores[pc]
                assert size == 2 and uc.reg_read(GPRS[0]) == 16
                assert uc.reg_read(GPRS[1]) == address
                gpio.append(high)
                uc.reg_write(UC_ARM_REG_PC, (pc + size) | 1)
            for pc in stores:
                m.hooks.append(m.uc.hook_add(UC_HOOK_CODE, capture, begin=pc, end=pc))
            for pc in [0x0800b106, 0x0800b13e]:
                m.hooks.append(m.uc.hook_add(UC_HOOK_CODE,
                    lambda uc, pc, size, _: uc.reg_write(UC_ARM_REG_PC, m.STOP | 1),
                    begin=pc, end=pc))
            m.call(0x0800b0da)
            high = bool(repeat and not setting107)
            assert gpio == [high]
            assert helpers == ([] if not repeat else
                [["0x801e2bc" if high else "0x801fc70", 4]])
            pc4_cases.append(dict(cfg77=repeat, cfg107=setting107, pc4_high=high,
                helpers=helpers))
    assert cps[0x77] == 0 and cps[0x107] == 0
    for path, digest in expected_hashes.items():
        assert hashlib.sha256(path.read_bytes()).hexdigest() == digest, str(path)
    summary = dict(requirement="R-02", hardware_accessed=False, mmio_mapped=False,
        calibration_sha256=expected_hashes[cal_path], cps_sha256=expected_hashes[cps_path],
        reference_sha256=expected_hashes[IMAGE], frequency_10hz=12_640_000,
        corrected_frequency_code=(regs[0x39] << 16) | regs[0x38], calibration_band=3,
        modulation="AM", width="Wide", reg01_input=6, am_volume_cfg=[50, 12],
        expected_write_words={f"{reg:02X}": f"{value:04X}" for reg, value in expected_words.items()},
        trace_events=len(events), csv_sha256=hashlib.sha256((HERE / "air-am-factory-stock.csv").read_bytes()).hexdigest(),
        pc4_cases=pc4_cases, actual_cfg77=0, actual_cfg107=0,
        limits="Original Thumb requests only. AIR record constructed in emulator RAM; actual preserved factory/CFG input unchanged. No MMIO, COM, signal, audio or observed register readback. A01 seed6 affects A48; A31 and unrelated reads use setup's55A0. PC4 fragment stops before unrelated application logic and intercepts named GPIO/helper calls. No full stock boot emulation.")
    (HERE / "air-am-evidence.json").write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
    print(f"PASS AIR AM: {len(events)} Thumb events; {len(pc4_cases)} PC4 branch cases; inputs unchanged")


if __name__ == "__main__":
    main()
