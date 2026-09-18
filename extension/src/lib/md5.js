// Minimal MD5 for Bilibili app parameter signing (RFC 1321).
// Verified against the standard test vectors below at load time.
function md5(str) {
  'use strict';
  function rl(x, c) { return (x << c) | (x >>> (32 - c)); }
  const S = [
    7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22,
    5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20,
    4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23,
    6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21
  ];
  const K = new Int32Array(64);
  for (let i = 0; i < 64; i++) {
    K[i] = Math.floor(Math.abs(Math.sin(i + 1)) * 4294967296) | 0;
  }
  const bytes = new TextEncoder().encode(str);
  const len = bytes.length;
  const total = (((len + 8) >> 6) + 1) << 6;
  const buf = new Uint8Array(total);
  buf.set(bytes);
  buf[len] = 0x80;
  const view = new DataView(buf.buffer);
  const bits = len * 8;
  view.setUint32(total - 8, bits >>> 0, true);
  view.setUint32(total - 4, Math.floor(bits / 4294967296), true);

  let a0 = 0x67452301, b0 = 0xefcdab89, c0 = 0x98badcfe, d0 = 0x10325476;
  const M = new Int32Array(16);
  for (let chunk = 0; chunk < total; chunk += 64) {
    for (let j = 0; j < 16; j++) M[j] = view.getInt32(chunk + j * 4, true);
    let A = a0, B = b0, C = c0, D = d0;
    for (let i = 0; i < 64; i++) {
      let F, g;
      if (i < 16) { F = (B & C) | (~B & D); g = i; }
      else if (i < 32) { F = (D & B) | (~D & C); g = (5 * i + 1) % 16; }
      else if (i < 48) { F = B ^ C ^ D; g = (3 * i + 5) % 16; }
      else { F = C ^ (B | ~D); g = (7 * i) % 16; }
      F = (F + A + K[i] + M[g]) | 0;
      A = D; D = C; C = B;
      B = (B + rl(F, S[i])) | 0;
    }
    a0 = (a0 + A) | 0; b0 = (b0 + B) | 0; c0 = (c0 + C) | 0; d0 = (d0 + D) | 0;
  }
  function hex(x) {
    let out = '';
    for (let i = 0; i < 4; i++) out += ((x >>> (i * 8)) & 255).toString(16).padStart(2, '0');
    return out;
  }
  return hex(a0) + hex(b0) + hex(c0) + hex(d0);
}

if (typeof self !== 'undefined' && typeof self.md5SelfTestDone === 'undefined') {
  self.md5SelfTestDone = true;
  if (md5('') !== 'd41d8cd98f00b204e9800998ecf8427e' || md5('abc') !== '900150983cd24fb0d6963f7d28e17f72') {
    console.error('md5 self-test FAILED');
  }
}
