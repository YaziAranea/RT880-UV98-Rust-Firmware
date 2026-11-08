"""Bounded original PC3 setup/mux stores and delay(10) instructions.

No MMIO, COM, board access or physical time measurement. GPIO calls/stores
are intercepted exactly; delay code itself executes without interception.
"""
import hashlib
import json

from derive_oracle import HERE, IMAGE, setup, GPRS, UC_HOOK_CODE, UC_ARM_REG_PC


def stop_at(m, address):
    m.hooks.append(m.uc.hook_add(UC_HOOK_CODE,
        lambda uc, pc, size, _: uc.reg_write(UC_ARM_REG_PC, m.STOP | 1),
        begin=address, end=address))


def main():
    m, _, _, _ = setup()
    gpio = []
    def configure(e):
        port, pointer = [e.uc.reg_read(r) for r in GPRS[:2]]
        config = list(e.read(pointer, 8))
        assert port == 0x40020800
        assert config == [0xc8, 3, 0, 0, 0, 0, 1, 2]
        gpio.append(dict(kind="configure", port=f"{port:08X}", bytes=config))
    m.intercept(0x08025464, configure)
    def clear(uc, pc, size, _):
        assert size == 2 and uc.reg_read(GPRS[0]) == 8
        assert uc.reg_read(GPRS[1]) == 0x40020828
        gpio.append(dict(kind="clear", address="40020828", mask=8))
        uc.reg_write(UC_ARM_REG_PC, (pc + size) | 1)
    m.hooks.append(m.uc.hook_add(UC_HOOK_CODE, clear, begin=0x08011c34, end=0x08011c34))
    stop_at(m, 0x08011c36)
    m.call(0x08011c08)
    assert len(gpio) == 2

    stores = []
    for start, pc, end, address, meaning in [
        (0x08010338, 0x08010340, 0x08010342, 0x40020828, "TX-stop/tail prefix LOW"),
        (0x08021858, 0x0802185c, 0x0802185e, 0x40020818, "digital backend HIGH"),
        (0x080218a2, 0x080218a8, 0x080218aa, 0x40020828, "digital backend restores LOW"),
    ]:
        m, _, _, _ = setup()
        seen = []
        def capture(uc, pc, size, _, address=address):
            assert size == 2 and uc.reg_read(GPRS[0]) == 8
            assert uc.reg_read(GPRS[1]) == address
            seen.append([address, 8])
            uc.reg_write(UC_ARM_REG_PC, (pc + size) | 1)
        m.hooks.append(m.uc.hook_add(UC_HOOK_CODE, capture, begin=pc, end=pc))
        stop_at(m, end)
        m.call(start)
        assert seen == [[address, 8]]
        stores.append(dict(start=f"{start:08X}", store_pc=f"{pc:08X}",
            address=f"{address:08X}", value=8, meaning=meaning))

    m, _, _, _ = setup()
    counts = {}
    def count(uc, pc, size, _):
        counts[f"{pc:08X}"] = counts.get(f"{pc:08X}", 0) + 1
    m.hooks.append(m.uc.hook_add(UC_HOOK_CODE, count, begin=0x080251dc, end=0x080251e8))
    m.call(0x080251dc, 10)
    assert counts == {"080251DC": 1, "080251E0": 10, "080251E2": 10,
        "080251E4": 10, "080251E6": 10, "080251DE": 9, "080251E8": 1}
    assert sum(counts.values()) == 51
    assert m.uc.reg_read(GPRS[0]) == 0
    report = dict(hardware_accessed=False, mmio_mapped=False,
        reference_sha256=hashlib.sha256(IMAGE.read_bytes()).hexdigest(),
        gpio_boot_excerpt="08011C08..08011C36", gpio_boot_events=gpio,
        pc3_normal_mode="output, push-pull, no pull, drive2, LOW",
        pc3_stores=stores, delay_function="080251DC..080251E8", delay_argument=10,
        instruction_counts=counts, instructions_executed=51,
        delay_physical_microseconds_measured=False,
        limits="Selected original Thumb fragments only; no complete boot/TX controller execution. PC3 mux function is derived separately from RT880 V2 schematic page3, not these GPIO events. No measurement of actual PC3/analog switch/microphone voltage. Instruction counts do not give physical cycles without a pipeline/flash/bus timing model. Not evidence that a new 1us bus delay is qualified.")
    (HERE / "mic-mux-timing-evidence.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print("PASS PC3 boot config + 3 exact stores; stock delay(10)=51 instructions; no MMIO/COM")


if __name__ == "__main__":
    main()
