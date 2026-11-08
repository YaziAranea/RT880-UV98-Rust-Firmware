"""Bounded stock instruction evidence for RX reset and AM scheduling/gates.

No hardware/COM. Timer callees are intercepted at their reviewed entrypoints;
no peripheral memory is mapped. Nominal timing is derived, not measured.
"""
import hashlib
import json
import struct
from derive_oracle import HERE, IMAGE, GPRS, UC_HOOK_CODE, UC_ARM_REG_PC, setup, save


def stop_at(m, address):
    m.hooks.append(m.uc.hook_add(UC_HOOK_CODE,
        lambda uc, pc, size, _: uc.reg_write(UC_ARM_REG_PC, m.STOP | 1),
        begin=address, end=address))


def main():
    m, events, regs, _ = setup()
    cases = sorted({0, 8, 0xffff, 0x55a0, 0x55a8, *[1 << bit for bit in range(16)]})
    rows = []
    for initial in cases:
        events.clear()
        regs[0x31] = initial
        m.call(0x08004518)
        assert events == [["read", "A", 0x31, initial],
            ["write", "A", 0x31, initial & 0xfff7],
            ["write", "A", 0x28, 0x6b38], ["write", "A", 0x29, 0xab40]]
        rows.extend([initial, step, *event] for step, event in enumerate(events))
    save("compander-reset.csv", ["initial31", "step", "kind", "target", "register", "value"], rows)

    m, _, _, _ = setup()
    timer = []
    for address, argc in [(0x08026094, 3), (0x080260a4, 2),
                          (0x080260d0, 3), (0x080260ae, 2)]:
        def capture(e, address=address, argc=argc):
            timer.append([hex(address), *[e.uc.reg_read(r) for r in GPRS[:argc]]])
            return 0
        m.intercept(address, capture)
    m.call(0x080211d4)
    assert timer == [["0x8026094", 0x40010000, 999, 47],
        ["0x80260a4", 0x40010000, 0],
        ["0x80260d0", 0x40010000, 1, 1], ["0x80260ae", 0x40010000, 1]]
    vectors = struct.unpack_from("<152I", IMAGE.read_bytes())
    assert vectors[41] == 0x08021205  # IRQ25, SDK TMR1_OVF_TMR10

    irq = []
    for initial in [0, 1, 2, 255]:
        m, _, _, _ = setup()
        m.intercept(0x080260be, lambda e: 1)  # TMR overflow flag
        m.intercept(0x080260b8, lambda e: 0)  # TMR flag clear
        m.intercept(0x080182bc, lambda e: 0)  # independent timer work
        m.intercept(0x0801fba0, lambda e: 0)
        stop_at(m, 0x08021252)  # immediately after target countdown
        m.write(0x200011ad, bytes([initial]))
        m.call(0x08021204)
        result = m.read(0x200011ad, 1)[0]
        assert result == max(initial - 1, 0)
        irq.append([initial, result])

    dispatch = []
    for initial in [0, 1, 2, 255]:
        m, _, _, _ = setup()
        calls = []
        for address in (0x080220ec, 0x08024140):
            m.intercept(address, lambda e, address=address: calls.append(hex(address)))
        stop_at(m, 0x0802497a)
        m.write(0x200011ad, bytes([initial]))
        m.call(0x08024966)
        result = m.read(0x200011ad, 1)[0]
        assert calls == (["0x80220ec", "0x8024140"] if initial == 0 else [])
        assert result == (1 if initial == 0 else initial)
        dispatch.append([initial, result, calls])

    gates = []
    for mode in [0, 1, 255]:
        for state in [0, 1, 2, 3, 4, 255]:
            m, events, regs, _ = setup()
            m.write(0x20000810, bytes([mode]))
            m.write(0x20001125, bytes([state]))
            m.word(0x200018d4, 90)
            regs[0x67] = 170
            stop_at(m, 0x080242ac)
            m.call(0x08024140)
            active = mode != 0 and state == 2
            assert bool(events) == active
            if active:
                assert events[0] == ["read", "A", 0x67, 170]
                assert events[1][:3] == ["write", "A", 0x13]
            gates.append([mode, state, active])

    summary = dict(reference_sha256=hashlib.sha256(IMAGE.read_bytes()).hexdigest(),
        compander_cases=len(cases), compander_sha256=hashlib.sha256((HERE / "compander-reset.csv").read_bytes()).hexdigest(),
        timer_calls=timer, vector_index=41, irq_number=25, handler="0x08021204",
        nominal_tmr_hz=48_000_000, nominal_period_us=1000,
        nominal_formula="48MHz / (47+1) / (999+1); APB2=48MHz from startup oracle",
        countdown_irq_cases=irq, dispatcher_cases=dispatch, agc_gate_cases=gates,
        gate_sources={"20000810": "0511E FM=0;0514A AM=1;05178 AFmode5=1",
            "20001125": "0509C activeRX=2;055BC TX=3;10854/B5F4 acquisition=1"},
        limits="IRQ prefix and scheduling fragment only; unrelated callees intercepted. Main-loop latency and electrical timer period not measured. AGC can run once per foreground pass at most once per nominal1ms tick; never infer exact observed period.",
        hardware_accessed=False, mmio_mapped=False)
    (HERE / "runtime-evidence.json").write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
