// Offline oracle from the exact previously reviewed, SHA-pinned packet function.
const fs = require('node:fs');
const crypto = require('node:crypto');
const vm = require('node:vm');
const path = require('node:path');
const root = path.resolve(__dirname, '../..');
const sha = data => crypto.createHash('sha256').update(data).digest('hex');
const source = fs.readFileSync(path.join(root, 'work/board-identification/webserial-flash880-pinned.html'));
if (sha(source) !== '4fce477ceab5d84dc18ba0552814fa2796fdfc4f9708c05ea657aee5f67d8e08') throw Error('Pinned source mismatch');
const text = source.toString('utf8');
const start = text.indexOf('const getModelChecksumSeed =');
const end = text.indexOf('const readByte =', start);
if (start < 0 || end < start) throw Error('Missing packet implementation');
const construct = vm.runInNewContext('const modelSelect = {selectedIndex:1};\n' + text.slice(start, end) + '\nconstructPacket;', {}, {timeout:1000});
const image = fs.readFileSync(path.join(root, 'outputs/UV98-Rust-diagnostic-0.1.8-2026-11-09-r5-build2/image/UV98-diag-0.1.8.bin'));
if (image.length !== 251904 || sha(image) !== 'bde124a26b800352b9c75b073548f114186241f7e57024dcb5f22c3786ed5307') throw Error('Image mismatch');
const packets = [construct(0x39, 0x3305, Uint8Array.of(0x10)), construct(0x39, 0x3305, Uint8Array.of(0x10)), construct(0x39, 0x3305, Uint8Array.of(0x55))];
for (let offset = 0; offset < image.length; offset += 1024) packets.push(construct(0x57, offset, image.subarray(offset, offset + 1024)));
console.log(JSON.stringify({image_sha256:sha(image), source_sha256:sha(source), packets:packets.length, tx_bytes:Buffer.concat(packets).length, tx_sha256:sha(Buffer.concat(packets)), device_accessed:false}, null, 2));
