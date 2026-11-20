import assert from 'node:assert/strict';
import { account, deadline, newMessage, session, startLab, subscribe, summary } from './lab.mjs';

const binary = process.argv[2];
if (!binary) {
  console.error('Usage: node tests/fanout.mjs <rill-chat-binary>');
  process.exit(2);
}

const senders = 4;
const subscribers = 16;
const warmups = 12;
const samples = 200;
const lab = await startLab(binary);
const streams = [];
try {
  const owner = session(lab.origin);
  await owner.json('/auth/create', { ...account('lead'), workspace: 'Delivery check' });
  const { code } = await owner.json('/invites', {});
  const writers = [owner];
  for (let n = 1; n < senders + subscribers; n++) {
    const member = session(lab.origin);
    await member.json('/auth/join', { ...account('member'), code });
    if (n < senders) writers.push(member);
    else streams.push(await subscribe(lab.origin, member));
  }
  const { channels } = await owner.json('/state');
  const channel = channels.find(item => item.kind === 'channel');
  const starts = new Map();
  const slowest = new Map();
  const timings = [];
  const readers = streams.map(async stream => {
    const seen = new Set();
    for (let n = 0; n < warmups + samples; n++) {
      const { payload, at } = await stream.wait('message');
      assert(!seen.has(payload.id), 'Duplicate message');
      seen.add(payload.id);
      const seq = Number(payload.body.slice(5));
      assert(starts.has(seq), 'Message did not originate from a benchmark sender');
      if (seq >= warmups) {
        const elapsed = at - starts.get(seq);
        timings.push(elapsed);
        slowest.set(seq, Math.max(slowest.get(seq) || 0, elapsed));
      }
    }
  });

  for (let batch = 0; batch < warmups + samples; batch += senders) {
    await Promise.all(writers.map((writer, index) => {
      const seq = batch + index;
      starts.set(seq, performance.now());
      return writer.json('/channels/' + channel.id + '/messages', newMessage('post-' + seq));
    }));
  }
  await deadline(Promise.all(readers), 'all subscribers to receive every post', 30000);
  assert.equal(timings.length, samples * subscribers);
  assert.equal(slowest.size, samples);
  console.log(JSON.stringify({
    transport: 'loopback TCP', senders, subscribers, posts: samples, warmups,
    all_deliveries: summary(timings), last_subscriber_per_post: summary([...slowest.values()])
  }, null, 2));
} finally {
  for (const stream of streams) stream.close();
  await lab.dispose();
}
