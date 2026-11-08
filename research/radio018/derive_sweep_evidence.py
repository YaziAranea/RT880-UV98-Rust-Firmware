"""Execute the original fast sweep retune fragment, without hardware/MMIO."""
import hashlib
import json
import struct
from derive_oracle import HERE, IMAGE, GPRS, setup, save
from derive_runtime_evidence import stop_at


def main():
    frequencies = [1_800_000, 3_199_999, 3_200_000, 4_500_000,
        6_400_000, 6_400_001, 12_640_000, 13_600_000, 14_550_000,
        26_000_000, 43_350_000, 66_000_000, 84_000_000, 130_000_000]
    rows, cases = [], 0
    for frequency in frequencies:
        for initial in [0, 0x5540, 0xa500, 0xffff]:
            m, events, regs, block = setup()
            regs[0x33] = initial
            m.write(0x20005006, struct.pack("<I", frequency))
            stop_at(m, 0x08023cf8)
            m.call(0x08023c58)
            band = m.read(0x2000081a, 1)[0]
            route = m.read(0x20000839, 1)[0]
            correction = struct.unpack_from("<H", block, 32 + band * 112)[0] - 32768
            corrected = (frequency + correction) & 0xffffffff
            mask = {1: 1, 2: 1, 3: 0x10, 4: 0x20, 5: 2}[route]
            assert events == [["write", "A", 0x38, corrected & 65535],
                ["write", "A", 0x39, corrected >> 16],
                ["write", "A", 0x37, 0x1f0f], ["write", "A", 0x30, 0x0200],
                ["write", "A", 0x30, 0xbff1], ["read", "A", 0x33, initial],
                ["write", "A", 0x33, (initial & 0xffcc) | mask]]
            rows.extend([cases, frequency, initial, band, route, step, *e]
                for step, e in enumerate(events))
            cases += 1
    save("fast-sweep.csv", ["case", "frequency", "initial33", "band", "route", "step",
        "kind", "target", "register", "value"], rows)
    # Only the dispatcher fragment: replace unrelated callees, observe cadence.
    dispatch = []
    for initial in [0, 1, 10, 255]:
        m, _, _, _ = setup()
        calls = []
        for address in [0x08023048, 0x08023658, 0x08023a00, 0x08023304, 0x080227c4]:
            m.intercept(address, lambda e, address=address: calls.append(hex(address)))
        stop_at(m, 0x080249dc)
        m.write(0x200011b0, bytes([initial]))
        m.call(0x080249bc)
        result = m.read(0x200011b0, 1)[0]
        assert result == (10 if initial == 0 else initial)
        assert len(calls) == (5 if initial == 0 else 0)
        dispatch.append([initial, result, calls])
    summary = dict(reference_sha256=hashlib.sha256(IMAGE.read_bytes()).hexdigest(),
        fragment="08023C58..08023CF8, full4B88/12A30 calls execute",
        cases=cases, events_per_case=7, dispatcher_cases=dispatch,
        trace_sha256=hashlib.sha256((HERE / "fast-sweep.csv").read_bytes()).hexdigest(),
        hardware_accessed=False, mmio_mapped=False,
        nominal_cadence_ms=10,
        limits="No mute, PA-off, relay change, settling wait or PLL-lock poll in fragment. A33 preserves TX bits: execution requires independent TX-off preconditions. 10ms is stock foreground scheduling, not measured electrical settling. FFFF seed intentionally demonstrates preserved TX bits and must never authorize live TX.")
    (HERE / "fast-sweep-evidence.json").write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
