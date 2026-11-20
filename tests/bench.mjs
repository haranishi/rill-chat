import assert from 'node:assert/strict';
import { account, deadline, newMessage, session, startLab, subscribe, summary } from './lab.mjs';

const [, , binary, count = '200'] = process.argv;
const samples = Number(count);
if (!binary || !Number.isInteger(samples) || samples < 1 || samples > 10000) {
  console.error('Usage: node tests/bench.mjs <rill-chat-binary> [1..10000 samples]');
  process.exit(2);
}

const lab = await startLab(binary);
let events;
try {
  const sender = session(lab.origin);
  const peer = session(lab.origin);
  await sender.json('/auth/create', { ...account('sender'), workspace: 'Bench' });
  const { code } = await sender.json('/invites', {});
  await peer.json('/auth/join', { ...account('reader'), code });
  const { channels } = await sender.json('/state');
  const channel = channels.find(item => item.kind === 'channel');
  assert.ok(channel);
  events = await subscribe(lab.origin, peer);

  const timings = [];
  const warmups = 10;
  for (let n = 0; n < warmups + samples; n++) {
    const message = newMessage('チームへの連絡 ' + n);
    const next = events.wait('message', data => data.body === message.body);
    const begin = performance.now();
    const [received, committed] = await deadline(Promise.all([
      next,
      sender.json(`/channels/${channel.id}/messages`, message)
    ]), 'message delivery');
    assert.equal(received.payload.id, committed.id);
    if (n >= warmups) timings.push(received.at - begin);
  }
  console.log(JSON.stringify({
    metric: 'Start of sender HTTP POST to decoded message on a second authenticated SSE connection',
    transport: 'loopback TCP', clients: 2, in_flight: 1, warmups, node: process.version,
    platform: process.platform + '/' + process.arch, ...summary(timings)
  }, null, 2));
} finally {
  events?.close();
  await lab.dispose();
}
