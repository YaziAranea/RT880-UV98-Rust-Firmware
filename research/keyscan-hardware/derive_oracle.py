"""Bounded related-stock GPIO helpers and column transition oracle. No MMIO."""
from pathlib import Path
import csv
import hashlib
import json
import os
import struct
import sys

HERE=Path(__file__).resolve().parent
PROJECT=Path(os.environ.get("UV98_RUST_PROJECT",str(HERE.parents[1])))
WORK=PROJECT.parent
sys.path.insert(0,str(PROJECT/"research"))
sys.path.insert(0,str(WORK/"calibration-analysis/deps"))
from stock_emu import StockArm,REFERENCE_SHA256,GPRS,UC_ARM_REG_PC,UC_HOOK_CODE
IMAGE=Path(os.environ.get("UV98_STOCK_REFERENCE",str(WORK/"reference-firmware/RT-880_Upgrade_V1_16_250915.nicsure.bin")))
RAM,PARAM=0x20004000,0x20004100
OUT=PROJECT/"crates/keyscan-at32/tests/fixtures"


def gpio_trace(seed):
    ports={base:bytearray(0x40) for base in (0x40020400,0x40020800,0x40020C00)}
    for mem in ports.values():
        for offset in (0,4,8,12):struct.pack_into("<I",mem,offset,seed)
    calls,stores=[],[]
    for start,end in ((0x08011BCC,0x08011BE4),(0x08011C7E,0x08011CEC)):
        emu=StockArm(IMAGE)
        # Byte4=push-pull, byte7=drive2 were explicitly set at11B80/11B86.
        # The first selected segment changes mask/mode/pull but inherits these.
        emu.write(0x2000B000,bytes([0,0,0,0,0,0,0,2]))
        def helper(e):
            base=e.uc.reg_read(GPRS[0]);assert base in ports
            raw=e.read(e.uc.reg_read(GPRS[1]),8)
            calls.append([hex(base),raw.hex()])
            machine=StockArm(IMAGE);machine.write(RAM,ports[base]);machine.write(PARAM,raw)
            machine.call(0x08025464,RAM,PARAM)
            ports[base][:]=machine.read(RAM,0x40)
            return 0
        emu.intercept(0x08025464,helper)
        def hook(uc,pc,size,unused):
            if pc==end:
                uc.reg_write(UC_ARM_REG_PC,emu.STOP|1)
            elif pc in (0x08011CAC,0x08011CB0,0x08011CB4,0x08011CB8):
                address,value=uc.reg_read(GPRS[1]),uc.reg_read(GPRS[0])
                assert address==0x40020418 and value in (0x400,4,2,1)
                stores.append([hex(pc),hex(address),value])
                uc.reg_write(UC_ARM_REG_PC,(pc+size)|1)
        emu.hooks.append(emu.uc.hook_add(UC_HOOK_CODE,hook,begin=start,end=end))
        emu.call(start)
    assert len(stores)==4
    values=[seed]+[struct.unpack_from("<I",p,o)[0] for p in ports.values() for o in (0,4,8,12)]
    return values,{"seed":seed,"gpio_calls":calls,"skipped_column_release_stores":stores}


def column_trace(column):
    emu=StockArm(IMAGE);emu.write(0x20001132,bytes([column]))
    sampled=[];stores=[]
    emu.intercept(0x0801FA9C,lambda e:sampled.append(e.uc.reg_read(GPRS[0])))
    pcs=(0x0801FBC2,0x0801FBCA,0x0801FBD8,0x0801FBE0,0x0801FBEE,0x0801FBF6,0x0801FC06,0x0801FC0E)
    def hook(uc,pc,size,unused):
        if pc in pcs:
            address,value=uc.reg_read(GPRS[1]),uc.reg_read(GPRS[0])
            assert address in (0x40020418,0x40020428)
            assert value in (0x400,4,2,1)
            stores.append([hex(pc),hex(address),value])
            uc.reg_write(UC_ARM_REG_PC,(pc+size)|1)
    emu.hooks.append(emu.uc.hook_add(UC_HOOK_CODE,hook,begin=0x0801FBA0,end=0x0801FC38))
    emu.call(0x0801FBA0)
    assert sampled==[column] and len(stores)==2
    assert stores[0][1]=="0x40020428" and stores[1][1]=="0x40020418"
    next_column=emu.read(0x20001132,1)[0]
    return [column,next_column,stores[0][2],stores[1][2]],{"sampled":sampled,"stores":stores}


def csv_file(name,header,rows):
    path=OUT/name
    with path.open("w",newline="",encoding="ascii") as f:
        w=csv.writer(f,lineterminator="\n");w.writerow(header);w.writerows(rows)
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    OUT.mkdir(parents=True,exist_ok=True)
    g=[gpio_trace(s) for s in (0,0xFFFFFFFF,0xA5A5A5A5,0x12345678)]
    c=[column_trace(n) for n in range(4)]
    hashes={
        "stock-gpio.csv":csv_file("stock-gpio.csv",["seed"]+[p+"_"+r for p in ("b","c","d") for r in ("cfgr","omode","odrvr","pull")],[x[0] for x in g]),
        "stock-columns.csv":csv_file("stock-columns.csv",["current","next","clear_mask","set_mask"],[x[0] for x in c]),
    }
    report={"reference_sha256":REFERENCE_SHA256,"hardware_qualified":False,
        "scope":"Two explicitly bounded init slices and column function; GPIO helpers executed against RAM; no MMIO mapping; no ADC/encoder/RF code",
        "init_slice_inheritance":"11BCC slice inherits byte4=0 and byte7=2 from explicit stock11B80/11B86 stores",
        "fixture_sha256":hashes,"gpio_traces":[x[1] for x in g],"column_traces":[x[1] for x in c],
        "driver_difference":"Open-drain rather than stock push-pull; break-before-make; two complete scans; aliases/multikey rejected for actions"}
    (HERE/"oracle.json").write_text(json.dumps(report,indent=2)+"\n",encoding="utf-8")
    print("Keyscan oracle: 4 GPIO seeds + 4 column transitions; no peripheral access")

if __name__=="__main__":main()
