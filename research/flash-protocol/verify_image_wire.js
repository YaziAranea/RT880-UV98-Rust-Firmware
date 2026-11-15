// Offline oracle: exact SHA-pinned upstream packet implementation.
// Never opens serial/network. argv[2] is an application-only BIN.
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const crypto = require('node:crypto');
const sha = data => crypto.createHash('sha256').update(data).digest('hex');
const source = fs.readFileSync(path.join(__dirname, 'webserial-flash880-pinned.html'));
if (sha(source) !== '4fce477ceab5d84dc18ba0552814fa2796fdfc4f9708c05ea657aee5f67d8e08') throw Error('Pinned source changed');
const text = source.toString('utf8');
const start = text.indexOf('const getModelChecksumSeed =');
const end = text.indexOf('const readByte =', start);
if (start < 0 || end < start) throw Error('Missing reviewed packet function');
const construct = vm.runInNewContext('const modelSelect = {selectedIndex:1};\n' + text.slice(start, end) + '\nconstructPacket;', {}, {timeout:1000});
const image = fs.readFileSync(process.argv[2]);
if (image.length !== 251904) throw Error('Wrong full application length');
const packets = [construct(0x39,0x3305,Uint8Array.of(0x10)),construct(0x39,0x3305,Uint8Array.of(0x10)),construct(0x39,0x3305,Uint8Array.of(0x55))];
for(let offset=0;offset<image.length;offset+=1024) packets.push(construct(0x57,offset,image.subarray(offset,offset+1024)));
console.log(JSON.stringify({image_sha256:sha(image),bytes:image.length,tx_sha256:sha(Buffer.concat(packets)),tx_bytes:Buffer.concat(packets).length,packets:packets.length,source_sha256:sha(source),device_accessed:false},null,2));
