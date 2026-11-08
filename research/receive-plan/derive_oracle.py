"""Exact bounded stock fragments, concatenated by an explicit new test harness.

No boot, device, COM or MMIO. SPI reads serve a synthetic calibration fixture.
The noise slice stops before AFC; all arithmetic and branch decisions execute
original Thumb. The concatenation is NOT claimed as a full stock RX procedure.
"""
import csv
import hashlib
import json
import os
import struct
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
PROJECT = Path(os.environ.get("UV98_RUST_PROJECT", str(HERE.parents[1])))
WORK = PROJECT.parent
HELPER = PROJECT / "research"
if not (HELPER / "deps/unicorn").exists():
    HELPER = WORK / "calibration-analysis"
sys.path.insert(0, str(HELPER))
from stock_emu import StockArm, REFERENCE_SHA256, GPRS, UC_ARM_REG_PC, UC_HOOK_CODE
IMAGE = Path(os.environ.get("UV98_STOCK_REFERENCE", str(WORK / "reference-firmware/RT-880_Upgrade_V1_16_250915.nicsure.bin")))

def synthetic_calibration(raw_offset):
    block = bytearray([0xff]*4096)
    block[0] = 0x9a
    for slot in range(15):
        record = bytearray((slot*17+i*3+5)&255 for i in range(112))
        struct.pack_into("<H", record, 0, raw_offset)
        block[32+slot*112:32+(slot+1)*112] = record
    return block

def setup(raw_offset, prior_reg33=0):
    emu = StockArm(IMAGE)
    events, spi_reads = [], []
    block = synthetic_calibration(raw_offset)
    emu.write(0x2000081a, b"\xff") # force a fresh calibration selection
    emu.write(0x200010e9, b"\0")  # primary NOR source
    emu.write(0x20000838, bytes([0xa6,0x5b])) # distinguish retained routing state
    emu.word(0x200010ec, 0x20005000)
    def spi(e):
        dest, address, length = [e.uc.reg_read(reg) for reg in GPRS[:3]]
        assert dest == 0x20003064 and length == 112
        assert address >= 32 and (address-32)%112 == 0 and address+length <= 4096
        spi_reads.append([address,length])
        e.write(dest, block[address:address+length])
        return 0
    emu.intercept(0x08025D30, spi)
    for entry, chip in [(0x0801E660,"A"),(0x0801E6D4,"B")]:
        def write(e, chip=chip):
            events.append(["write",chip,e.uc.reg_read(GPRS[0]),e.uc.reg_read(GPRS[1])&0xffff])
            return 0
        emu.intercept(entry, write)
    def read(e):
        register = e.uc.reg_read(GPRS[0])
        assert register == 0x33
        events.append(["read","A",register,prior_reg33])
        return prior_reg33
    emu.intercept(0x0801E620, read)
    def delay(e):
        events.append(["delay","caller",0,e.uc.reg_read(GPRS[0])])
        return 0
    emu.intercept(0x08008380, delay)
    def gpio(uc, pc, size, unused):
        high = pc == 0x08005086
        assert size == 2 and uc.reg_read(GPRS[0]) == 1
        assert uc.reg_read(GPRS[1]) == (0x40021418 if high else 0x40021428)
        events.append(["gpio","F",0,int(high)])
        uc.reg_write(UC_ARM_REG_PC,(pc+size)|1)
    for pc in [0x0800507c,0x08005086]:
        emu.hooks.append(emu.uc.hook_add(UC_HOOK_CODE,gpio,begin=pc,end=pc))
    emu.hooks.append(emu.uc.hook_add(UC_HOOK_CODE,
        lambda uc,pc,size,unused: uc.reg_write(UC_ARM_REG_PC,emu.STOP|1),
        begin=0x08004d5c,end=0x08004d5c))
    return emu,events,spi_reads

def save(name,header,rows):
    path = HERE/name
    with path.open("w",encoding="utf-8",newline="") as f:
        w=csv.writer(f,lineterminator="\n");w.writerow(header);w.writerows(rows)
    return hashlib.sha256(path.read_bytes()).hexdigest()

def main():
    hashes = {}
    rows, case_meta = [], []
    frequencies = [1_800_000,3_200_000,4_500_000,6_400_000,6_400_001,
                   14_550_000,25_999_999,26_000_000,43_350_000,66_000_000,
                   84_000_000,130_000_000]
    case=0
    for f in frequencies:
        for width in [0,1]:
            for level in [0,1,10]:
                for offset in [0,0x8000,0xffff]:
                    for prior in [0,0xa5a5]:
                        e,events,spi=setup(offset,prior)
                        channel=bytearray(48);channel[3]=width
                        e.write(0x20005000,channel)
                        e.write(0x200035ec+0xa4,bytes([level]))
                        e.write(0x200011f9,b"\0") # ordinary calibration path
                        # New harness composition of original fragments.
                        e.call(0x08004228,width,count=2000)
                        e.call(0x08004b88,f,count=5000)
                        selected=e.read(0x2000081d,4)
                        e.call(0x08004cca,count=3000)
                        e.call(0x08004ff0,count=2000)
                        assert len(spi)==1 and len(events)==8
                        for step,event in enumerate(events):
                            rows.append([case,f,width,level,offset,prior,step,*event])
                        case_meta.append({"case":case,"spi_read":spi[0],
                                          "selected_noise_wide":selected[0],"selected_noise_narrow":selected[1],
                                          "selected_squelch_wide":selected[2],"selected_squelch_narrow":selected[3]})
                        case+=1
    hashes["composition.csv"]=save("composition.csv",
        ["case","frequency_10hz","width","level","raw_offset","prior_reg33","step","kind","target","address","value"],rows)
    freq_rows=[];freq_case=0
    for chip in ["A","B"]:
        for f in frequencies:
            for offset in [0,0x8000,0xffff]:
                e,events,spi=setup(offset)
                e.call(0x08004b88 if chip=="A" else 0x08003ccc,f,count=5000)
                assert len(events)==2 and len(spi)==1
                for step,event in enumerate(events):
                    freq_rows.append([freq_case,chip,f,offset,step,*event])
                freq_case+=1
    hashes["frequency.csv"]=save("frequency.csv",
        ["case","chip","frequency_10hz","raw_offset","step","kind","target","address","value"],freq_rows)
    widths=[]
    for width in range(256):
        e,events,_=setup(0x8000)
        e.call(0x08004228,width,count=1000)
        assert len(events)==1
        widths.append([width,*events[0]])
    hashes["width.csv"]=save("width.csv",["raw_width","kind","target","address","value"],widths)
    starts=[]
    for chip,entry in [("A",0x08004fa0),("B",0x08003e3c)]:
        e,events,_=setup(0x8000)
        e.call(entry,count=1000)
        for step,event in enumerate(events):starts.append([chip,step,*event])
    hashes["rx-start.csv"]=save("rx-start.csv",["chip","step","kind","target","address","value"],starts)
    input_path=HERE/"composition-inputs.json"
    input_path.write_text(json.dumps(case_meta,indent=2)+"\n",encoding="utf-8")
    hashes[input_path.name]=hashlib.sha256(input_path.read_bytes()).hexdigest()
    for offset in [0,0x8000,0xffff]:
        path=HERE/f"synthetic-calibration-{offset:04x}.hex"
        path.write_text(synthetic_calibration(offset).hex()+"\n",encoding="ascii")
        hashes[path.name]=hashlib.sha256(path.read_bytes()).hexdigest()
    report={"reference_sha256":REFERENCE_SHA256,"composition_cases":case,"composition_events":len(rows),
        "frequency_cases":freq_case,"width_cases":len(widths),"rx_start_cases":2,
        "mmio_mapped":False,"hardware_accessed":False,"hashes":hashes,
        "composition":"New concatenation: 4228 -> 4B88 -> 4CCA..4D5C ->4FF0; not full stock caller.",
        "hooks":["SPI read synthetic record","RF writes capture","A33 read supplied seed","PF0 store PCs507C/5086 skipped",
                 "stop4D5C before AFC","8380 delay captured for RXstart only"],
        "ordinary_noise_path":"RAM11F9=0, channel pointer200010EC->20005000, width[3], config35EC+A4=level",
        "synthetic_calibration":"4096B FF; marker0=9A;15recordsat32+slot*112; byte[i]=(slot*17+i*3+5)&255;record[0..2]=offsetLE"}
    (HERE/"oracle-summary.json").write_text(json.dumps(report,indent=2)+"\n",encoding="utf-8")
    print(json.dumps(report,indent=2))

if __name__=="__main__":main()
