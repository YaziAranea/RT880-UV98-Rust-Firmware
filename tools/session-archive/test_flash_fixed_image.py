import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import flash_fixed_image as f


class FakePort:
    timeout = 5
    def __init__(self, bad_ack=None, short_write=None, no_final=False, unsolicited=b''):
        self.sent = []
        self.rx = bytearray(unsolicited)
        self.bad_ack, self.short_write, self.no_final = bad_ack, short_write, no_final
        self.closed = False
    def write(self, data):
        self.sent.append(bytes(data))
        n = len(self.sent)
        self.rx.append(0xff if n == self.bad_ack else 6)
        if n == 249 and not self.no_final:
            self.rx.append(6)
        return len(data) - 1 if n == self.short_write else len(data)
    def read(self, count):
        result = bytes(self.rx[:count]); del self.rx[:count]
        return result
    def close(self):
        self.closed = True


class Tests(unittest.TestCase):
    def test_radio018r2_exact_oracle_and_fake_transfer(self):
        image = f.checked_image('diagnostic018r2')
        expected = [frame for _, frame in f.packets(image)]
        self.assertEqual(f.sha(b''.join(expected)), '85b3489e33c284096a086cd4c942a4f90ea34917f5dc5f5a7615be8e85fd3512')
        self.assertEqual(b''.join(frame[3:-1] for frame in expected[3:]), image)
        fake = FakePort()
        with tempfile.TemporaryDirectory() as temp:
            result = f.flash('diagnostic018r2', 'FAKE', Path(temp)/'new', factory=lambda _:fake, progress=lambda *a,**k:None)
            self.assertEqual(fake.sent,expected)
            self.assertEqual((result['acked_blocks'],result['ack_count']),(246,250))
            self.assertTrue(result['final_ack_received'] and result['port_closed'] and fake.closed)
        with patch.dict(f.TX_SHA256, {'diagnostic018r2':'0'*64}), tempfile.TemporaryDirectory() as temp:
            with self.assertRaises(ValueError):
                f.flash('diagnostic018r2','FAKE',Path(temp)/'new',factory=lambda _:self.fail('port opened'))

    def test_radio018_exact_oracle_and_fake_transfer(self):
        image = f.checked_image('diagnostic018')
        expected = [frame for _, frame in f.packets(image)]
        self.assertEqual(f.sha(b''.join(expected)), 'a7be3c04378ee8434d8797d5650fe8aed0703636977158ff02fbc7f460d4c667')
        self.assertEqual(b''.join(frame[3:-1] for frame in expected[3:]), image)
        fake = FakePort()
        with tempfile.TemporaryDirectory() as temp:
            result = f.flash('diagnostic018', 'FAKE', Path(temp)/'new', factory=lambda _:fake, progress=lambda *a,**k:None)
            self.assertEqual(fake.sent,expected)
            self.assertEqual((result['acked_blocks'],result['ack_count']),(246,250))
            self.assertTrue(result['final_ack_received'] and result['port_closed'] and fake.closed)
        with patch.dict(f.TX_SHA256, {'diagnostic018':'0'*64}), tempfile.TemporaryDirectory() as temp:
            with self.assertRaises(ValueError):
                f.flash('diagnostic018','FAKE',Path(temp)/'new',factory=lambda _:self.fail('port opened'))

    def test_uart_probe012_fixed_hash_and_fake_transfer(self):
        image = f.checked_image('uartprobe012')
        expected = [frame for _,frame in f.packets(image)]
        self.assertEqual(f.sha(b''.join(expected)), 'dafc8d71166b87215c8d8c3cb6ed2d1d90fa8dbb04272b71accfa95ff2f5c267')
        self.assertEqual(b''.join(frame[3:-1] for frame in expected[3:]), image)
        fake = FakePort()
        with tempfile.TemporaryDirectory() as temp:
            result = f.flash('uartprobe012','FAKE',Path(temp)/'new',factory=lambda _: fake,
                             progress=lambda *a,**k: None)
            self.assertEqual(fake.sent,expected)
            self.assertEqual(result['ack_count'],250)
            self.assertTrue(result['final_ack_received'] and fake.closed)
        with patch.dict(f.IMAGES, {'uartprobe012':(f.IMAGES['uartprobe012'][0],'0'*64)}), tempfile.TemporaryDirectory() as temp:
            with self.assertRaises(ValueError):
                f.flash('uartprobe012','FAKE',Path(temp)/'new',factory=lambda _: self.fail('port opened'))

    def test_diagnostic011_pinned_oracle_and_fake_full_transfer(self):
        image = f.checked_image('diagnostic011')
        planned = tuple(f.packets(image))
        self.assertEqual(f.sha(b''.join(frame for _, frame in planned)),
                         '7f698eba1b6b7814db5d8e17c8b112098bd26e8dac7f8876eb44578e8e68aa25')
        self.assertEqual(b''.join(frame[3:-1] for _, frame in planned[3:]), image)
        fake = FakePort()
        with tempfile.TemporaryDirectory() as temp:
            report = f.flash('diagnostic011', 'FAKE', Path(temp)/'new',
                             factory=lambda _: fake, progress=lambda *a, **k: None)
            self.assertEqual(fake.sent, [frame for _,frame in planned])
            self.assertEqual((report['acked_blocks'], report['ack_count']), (246,250))
            self.assertTrue(report['port_closed'] and report['final_ack_received'])
        with patch.dict(f.TX_SHA256, {'diagnostic011':'0'*64}), tempfile.TemporaryDirectory() as temp:
            with self.assertRaises(ValueError):
                f.flash('diagnostic011', 'FAKE', Path(temp)/'new',
                        factory=lambda _: self.fail('port opened before oracle check'))

    def run_flash(self, fake, directory):
        return f.flash('diagnostic', 'FAKE', directory, factory=lambda _: fake,
                       progress=lambda *args, **kwargs: None)

    def test_exact_iradio_wire_sequence_and_full_image(self):
        image = f.checked_image('diagnostic')
        expected = list(f.packets(image))
        self.assertEqual([p.hex() for _,p in expected[:3]],
                         ['3933051081', '3933051081', '39330555c6'])
        self.assertEqual(len(expected), 249)
        for index,(_,frame) in enumerate(expected[3:]):
            self.assertEqual(frame[:3], bytes((0x57, ((index*1024)>>8)&255, 0)))
            self.assertEqual(frame[-1], sum(frame[:-1]) & 255)
            self.assertEqual(len(frame), 1028)
        self.assertEqual(b''.join(p[3:-1] for _,p in expected[3:]), image)
        fake = FakePort()
        with tempfile.TemporaryDirectory() as temp:
            out=Path(temp)/'new'; result=self.run_flash(fake,out)
            self.assertEqual(fake.sent, [p for _,p in expected])
            self.assertEqual(result['ack_count'],250)
            self.assertEqual(result['acked_blocks'],246)
            self.assertTrue(result['final_ack_received'] and fake.closed)
            self.assertFalse(result['hardware_boot_verified'])
            self.assertEqual((out/'rx.bin').read_bytes(),b'\x06'*250)

    def test_faults_stop_without_retry_and_close(self):
        for fake, expected_tx in [(FakePort(bad_ack=1),1),(FakePort(bad_ack=70),70),
                                  (FakePort(short_write=8),8),(FakePort(no_final=True),249),
                                  (FakePort(unsolicited=b'\x06'),0)]:
            with self.subTest(expected_tx=expected_tx), tempfile.TemporaryDirectory() as temp:
                out=Path(temp)/'new'
                with self.assertRaises(RuntimeError): self.run_flash(fake,out)
                self.assertEqual(len(fake.sent),expected_tx)
                self.assertTrue(fake.closed)
                report=json.loads((out/'manifest.json').read_text())
                self.assertEqual(report['status'],'failed')
                self.assertFalse(report['final_ack_received'])

    def test_reject_wrong_image_before_open_and_existing_output(self):
        with patch.dict(f.IMAGES, {'diagnostic': (f.IMAGES['diagnostic'][0], '0'*64)}):
            with self.assertRaises(ValueError): f.checked_image('diagnostic')
        for size in (0,1024,f.BYTES-1,f.BYTES+1):
            with self.assertRaises(ValueError): list(f.packets(bytes(size)))
        with tempfile.TemporaryDirectory() as temp:
            with self.assertRaises(FileExistsError):
                f.flash('diagnostic','FAKE',Path(temp),factory=lambda _: self.fail('port opened'))


if __name__ == '__main__': unittest.main()
