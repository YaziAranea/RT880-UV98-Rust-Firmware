"""Bounded original Thumb GPS/Si47 evidence. No hardware or MMIO mappings.

Only listed SDK/I2C callees and exact GPIO stores are intercepted. No full
firmware boot, calibration writes, serial port access, or real GPS data.
"""
import hashlib
import json
import struct

from derive_oracle import HERE, IMAGE, GPRS, setup, UC_HOOK_CODE, UC_ARM_REG_PC
from derive_runtime_evidence import stop_at
from stock_emu import UC_ARM_REG_SP


def args(m, count=4):
    return [m.uc.reg_read(r) for r in GPRS[:count]]


def gpio_store(m, pc, address, value, output, offset=0):
    def hook(uc, current, size, _):
        assert size == 2
        assert args(m, 2) == [value, address - offset]
        output.append(["gpio_store", hex(address), value])
        uc.reg_write(UC_ARM_REG_PC, (current + size) | 1)
    m.hooks.append(m.uc.hook_add(UC_HOOK_CODE, hook, begin=pc, end=pc))


def gps():
    baud_table = [4800, 9600, 14400, 19200, 38400, 56000, 57600,
                  115200, 128000, 256000]
    m, *_ = setup()
    for index in range(256):
        assert m.call(0x08012fd0, index)[0] == (baud_table[index] if index < 10 else 9600)
    initializations = []
    for baud in baud_table:
        m, *_ = setup()
        events = []
        for address, count in [(0x08026110, 4), (0x08026284, 2),
                               (0x0802627a, 2), (0x08026244, 3), (0x080260f6, 2)]:
            m.intercept(address, lambda e, a=address, n=count:
                        events.append([hex(a), *args(e, n)]))
        m.call(0x0802480c, baud)
        assert events == [["0x8026110", 0x40004400, baud, 0, 0],
                          ["0x8026284", 0x40004400, 0],
                          ["0x802627a", 0x40004400, 1],
                          ["0x8026244", 0x40004400, 0xc0005, 1],
                          ["0x80260f6", 0x40004400, 1]]
        initializations.append(events)
    auxiliary = []
    for enabled in [0, 1]:
        m, *_ = setup()
        events = []
        m.intercept(0x08025464, lambda e: events.append(
            ["gpio_init", hex(args(e, 2)[0]), e.read(args(e, 2)[1], 8).hex()]))
        if not enabled:
            gpio_store(m, 0x08002f40, 0x40020028, 1, events, 0x28)
            gpio_store(m, 0x08002f44, 0x40020028, 2, events, 0x28)
            gpio_store(m, 0x08002f48, 0x40021428, 2, events)
        m.call(0x08002f30, enabled)
        assert events[-1] == ["gpio_init", "0x40020000",
                              "0400000000" + ("01" if enabled else "02") + "0001"]
        auxiliary.append({"enabled": enabled, "events": events})
    m, *_ = setup()
    power = []
    gpio_store(m, 0x0800f314, 0x40020418, 0x200, power)
    m.intercept(0x08002f30, lambda e: power.append(["auxiliary", args(e, 1)[0]]))
    stop_at(m, 0x0800f31c)
    m.call(0x0800f30e)
    vector = struct.unpack_from("<I", IMAGE.read_bytes(), 54 * 4)[0]
    assert vector == 0x080244e5
    return dict(baud_cases=256, baud_table=baud_table, invalid_baud=9600,
                uart_initializations=initializations, rx_irq_vector=hex(vector),
                auxiliary_pin_setup=auxiliary, power_enable_fragment=power)


def si47():
    m, *_ = setup()
    bus = []
    m.intercept(0x08025464, lambda e: bus.append(
        ["gpio_init", hex(args(e, 2)[0]), e.read(args(e, 2)[1], 8).hex()]))
    m.intercept(0x080254f0, lambda e: bus.append(["gpio_mux", *args(e, 3)]))
    m.intercept(0x08025562, lambda e: bus.append(
        ["i2c_init", hex(args(e, 1)[0]), hex(int.from_bytes(e.read(args(e, 1)[0], 4), "little"))]))
    m.call(0x0801eadc)
    assert bus == [["gpio_init", "0x40020800", "0300000001010201"],
                   ["gpio_mux", 0x40020800, 0, 7], ["gpio_mux", 0x40020800, 1, 7],
                   ["i2c_init", "0x20008324", "0x40005400"]]
    wrappers = []
    for function, callee, wire_address in [(0x0801f05c, 0x0802569e, 0x22),
                                           (0x0801eab4, 0x080255e8, 0x23)]:
        for sdk_result in [0, 1]:
            m, *_ = setup()
            calls = []
            def sdk(e):
                timeout = int.from_bytes(e.read(e.uc.reg_read(UC_ARM_REG_SP), 4), "little")
                calls.append([*args(e), timeout])
                return sdk_result
            m.intercept(callee, sdk)
            result = m.call(function, 0x20009000, 8)[0]
            assert calls == [[0x20008324, wire_address, 0x20009000, 8, 2000]]
            assert result == (1 if sdk_result == 0 else 0)
            wrappers.append(dict(function=hex(function), sdk_result=sdk_result,
                                 calls=calls, result=result))
    queries = []
    for failures, write_result, status in [(0, 1, 0x80), (3, 1, 0x80),
                                           (0, 0, 0x80), (0, 1, 0x40), (-1, 1, 0)]:
        m, delays, *_ = setup()
        events, reads = [], [0]
        def write(e):
            pointer, length = args(e, 2)
            events.append(["write", e.read(pointer, length).hex()])
            return write_result
        def read(e):
            pointer, length = args(e, 2)
            reads[0] += 1
            success = failures >= 0 and reads[0] > failures
            if success:
                e.write(pointer, bytes([status]) + bytes(length - 1))
            if len(events) < 16:
                events.append(["read", length, success])
            return int(success)
        m.intercept(0x0801f05c, write)
        m.intercept(0x0801eab4, read)
        result, budget_exhausted = None, False
        try:
            result = m.call(0x0801ebc0, count=5000)[0]
        except RuntimeError:
            assert failures == -1
            budget_exhausted = True
        assert events[0] == ["write", "011f05"]
        if failures >= 0:
            assert result == 1 and reads[0] == failures + 3
            assert [row[3] for row in delays] == [1, 500] + [1] * (failures + 3)
        else:
            assert budget_exhausted and reads[0] > 40
        queries.append(dict(initial_read_failures=failures, write_result=write_result,
                            response_status=status, result=result, reads=reads[0],
                            events=events, emulator_budget_exhausted=budget_exhausted,
                            delays_ms=[row[3] for row in delays[:16]]))
    return dict(bus_init=bus, wire_address_wrappers=wrappers,
                address_7bit=0x11, library_id_queries=queries,
                stock_library_id_query_ignores_send_failure_and_error_status=True,
                stock_library_id_query_has_unbounded_bus_retry=True)


def denoise():
    m, *_ = setup()
    stop_at(m, 0x0801e0e6)
    for value in range(256):
        m.write(0x200035ec + 0x108, bytes([value]))
        m.call(0x0801e0d6)
        assert m.read(0x200035ec + 0x108, 1) == b"\0"
    return {"cfg_offset": 264, "cases": 256, "sanitized_value": 0,
            "fragment": "0801E0D6..0801E0E6"}


def external_modem():
    cases = []
    for setting in [0, 1, 7, 255]:
        m, events, *_ = setup()
        m.write(0x200039ec + 0xf2, bytes([setting]))
        m.intercept(0x08005e90, lambda e: events.append(["c_bus_byte", args(e, 1)[0]]))
        for pc in [0x08005c10, 0x08005c2a, 0x08005c50, 0x08005c76, 0x08005c9c]:
            gpio_store(m, pc, 0x40021428, 2, events)
        for pc in [0x08005c1e, 0x08005c44, 0x08005c6a, 0x08005c90, 0x08005cc2]:
            gpio_store(m, pc, 0x40021418, 2, events)
        m.call(0x08005c04)
        commands, current = [], []
        for event in events:
            if event[0] == "c_bus_byte":
                current.append(event[1])
            elif event == ["gpio_store", "0x40021418", 2]:
                commands.append(current)
                current = []
        assert commands == [[1], [0xe0, 0x11, 0x80], [0xe0, 0x11, 0x40],
                            [0xe1, 0x02, 0x1c], [0xe2, (0x30 | (setting << 1)) & 255, 0x3e]]
        assert [e[3] for e in events if e[0] == "delay"] == [100, 50, 25, 5, 10, 100]
        cases.append(dict(setting=setting, commands=commands, events=events))
    return dict(fragment="08005C04", cases=cases,
                interpretation="PF1 active-low select and command addresses match CMX868/868A C-BUS. Model/presence is not proven by command format. No live modem access.")


def main():
    result = dict(reference_sha256=hashlib.sha256(IMAGE.read_bytes()).hexdigest(),
                  hardware_accessed=False, mmio_mapped=False,
                  gps=gps(), si47=si47(), denoise=denoise(), external_modem=external_modem(),
                  limits="Original Thumb with synthetic RAM and mocked I/O. No proof of installed receiver, physical pin continuity, GPS sentences, ID response, audio, or ANC. Query failure cases intentionally document unsafe stock behavior; never copy its unbounded retry or ignored status into hardware code.")
    destination = HERE / "aux-evidence.json"
    destination.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(dict(result="PASS", baud_cases=256, gps_initializations=10,
                          si_wrapper_cases=4, si_query_cases=5, denoise_cases=256, modem_cases=4,
                          path=str(destination), hardware_accessed=False)))


if __name__ == "__main__":
    main()
