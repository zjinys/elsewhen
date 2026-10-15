// Test-only protocol vector, independently generated with Node/OpenSSL.
// The key, seed and nonce below are public fixtures, never production material.
import { createCipheriv, createPrivateKey, createPublicKey, sign } from 'node:crypto';

const key = Buffer.from(Array.from({ length: 32 }, (_, i) => i));
const seed = Buffer.from(Array.from({ length: 32 }, (_, i) => 32 + i));
const nonce = Buffer.from(Array.from({ length: 12 }, (_, i) => 64 + i));
const header = {
  protocol_version: 1,
  dataset_id: 'vector-dataset',
  sender_device_id: 'vector-a',
  recipient_device_id: 'vector-b',
  key_version: 7,
  message_id: 'provider:vector-v1',
};
const plaintext = Buffer.from('{"api_key":"SYNTHETIC_VECTOR_ONLY","note":"跨设备"}');
const u32 = (n) => { const b = Buffer.alloc(4); b.writeUInt32BE(n); return b; };
const u64 = (n) => { const b = Buffer.alloc(8); b.writeBigUInt64BE(BigInt(n)); return b; };
const frame = (b) => Buffer.concat([u32(b.length), b]);
const aad = Buffer.concat([
  Buffer.from('elsewhen.sync.envelope.aad.v1\0'),
  u32(header.protocol_version), u64(header.key_version),
  ...[header.dataset_id, header.sender_device_id, header.recipient_device_id, header.message_id]
    .map((id) => frame(Buffer.from(id))),
]);
const cipher = createCipheriv('chacha20-poly1305', key, nonce, { authTagLength: 16 });
cipher.setAAD(aad, { plaintextLength: plaintext.length });
const ciphertext = Buffer.concat([cipher.update(plaintext), cipher.final(), cipher.getAuthTag()]);
const signed = Buffer.concat([
  Buffer.from('elsewhen.sync.envelope.signature.v1\0'), frame(aad), frame(nonce), frame(ciphertext),
]);
const pkcs8 = Buffer.concat([Buffer.from('302e020100300506032b657004220420', 'hex'), seed]);
const privateKey = createPrivateKey({ key: pkcs8, type: 'pkcs8', format: 'der' });
const publicKey = createPublicKey(privateKey).export({ type: 'spki', format: 'der' }).subarray(-32);
const signature = sign(null, signed, privateKey);
console.log(JSON.stringify({
  generator: 'Node crypto / OpenSSL; scripts/sync-envelope-vector.mjs; public test keys only',
  key: key.toString('hex'), seed: seed.toString('hex'), public_key: publicKey.toString('hex'),
  plaintext: plaintext.toString('utf8'), aad: aad.toString('hex'), signed: signed.toString('hex'),
  wire: { header, nonce: nonce.toString('hex'), ciphertext: ciphertext.toString('hex'), signature: signature.toString('hex') },
}, null, 2));
