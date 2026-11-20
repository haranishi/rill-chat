import { spawn } from 'node:child_process';
import { randomBytes, randomUUID } from 'node:crypto';
import { once } from 'node:events';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { createInterface } from 'node:readline';
import { Readable } from 'node:stream';

export async function deadline(promise, label, ms = 15000) {
  let timer;
  try {
    return await Promise.race([
      promise,
      new Promise((_, reject) => {
        timer = setTimeout(() => reject(new Error(`Timed out waiting for ${label}`)), ms);
      })
    ]);
  } finally {
    clearTimeout(timer);
  }
}

// Tests own the service and its database, including across intentional restarts.
export async function startLab(binary) {
  const dir = await mkdtemp(join(tmpdir(), 'rill-test-'));
  const exe = resolve(binary);
  let child;
  let port = '0';
  let origin;
  const emergencyStop = () => child?.kill('SIGKILL');
  process.once('exit', emergencyStop);

  async function start() {
    child = spawn(exe, [], {
      cwd: dir,
      env: { ...process.env, RILL_ENV: 'development', RILL_BIND: `127.0.0.1:${port}`, RILL_DB: join(dir, 'chat.sqlite3') },
      stdio: ['ignore', 'pipe', 'inherit']
    });
    const lines = createInterface({ input: child.stdout });
    const address = new Promise((accept, reject) => {
      lines.on('line', line => {
        const match = line.match(/^Rill listening on (http:\/\/127\.0\.0\.1:(\d+))\s*$/);
        if (match) accept(match);
      });
      child.once('error', reject);
      child.once('exit', (code, signal) => reject(new Error(`Rill exited (${code ?? signal})`)));
    });
    const [, url, actualPort] = await deadline(address, 'Rill startup');
    port = actualPort;
    origin = url;
    const health = await fetch(`${origin}/healthz`, { signal: AbortSignal.timeout(5000) });
    if (!health.ok) throw new Error(`Health check returned ${health.status}`);
    await health.text();
  }

  async function stop() {
    if (!child || !child.pid || child.exitCode !== null || child.signalCode !== null) return;
    const exited = once(child, 'exit');
    child.kill('SIGTERM');
    const force = setTimeout(() => child.kill('SIGKILL'), 3000);
    try { await exited; } finally { clearTimeout(force); }
  }
  async function dispose() {
    try { await stop(); } finally {
      process.removeListener('exit', emergencyStop);
      await rm(dir, { recursive: true, force: true });
    }
  }

  try { await start(); } catch (error) { await dispose(); throw error; }
  return {
    origin,
    restart: async () => { await stop(); await start(); },
    dispose
  };
}

export function account(role) {
  return {
    handle: role + '_' + randomBytes(6).toString('hex'),
    name: role,
    password: randomBytes(18).toString('base64url')
  };
}

export function session(origin) {
  let cookie = '';
  return {
    get cookie() { return cookie; },
    async json(path, body) {
      const headers = { Cookie: cookie };
      const init = { headers, signal: AbortSignal.timeout(10000) };
      if (body !== undefined) {
        Object.assign(headers, { 'Content-Type': 'application/json', Origin: origin });
        Object.assign(init, { method: 'POST', body: JSON.stringify(body) });
      }
      const response = await fetch(origin + '/api' + path, init);
      const auth = response.headers.getSetCookie()[0];
      if (auth) cookie = auth.split(';')[0];
      if (!response.ok) throw new Error(`HTTP ${response.status} on ${path.split('?')[0]}`);
      const payload = await response.text();
      return payload ? JSON.parse(payload) : undefined;
    }
  };
}

async function* frames(response) {
  const lines = createInterface({ input: Readable.fromWeb(response.body), crlfDelay: Infinity });
  let name = 'message';
  let data = [];
  try {
    for await (const line of lines) {
      if (line === '') {
        if (data.length) yield { name, payload: JSON.parse(data.join('\n')), at: performance.now() };
        name = 'message';
        data = [];
      } else if (line.startsWith('event:')) {
        name = line.slice(6).trimStart();
      } else if (line.startsWith('data:')) {
        data.push(line.slice(5).replace(/^ /, ''));
      }
    }
  } finally {
    lines.close();
  }
}

export async function subscribe(origin, client) {
  const controller = new AbortController();
  try {
    const response = await deadline(fetch(origin + '/api/events', {
      headers: { Cookie: client.cookie, Accept: 'text/event-stream' },
      signal: controller.signal
    }), 'event stream');
    if (!response.ok) throw new Error(`Event stream returned HTTP ${response.status}`);
    const stream = frames(response);
    async function wait(name, match = () => true) {
      for (;;) {
        const item = await deadline(stream.next(), name + ' event', 10000);
        if (item.done) throw new Error('Event stream closed');
        if (item.value.name === name && match(item.value.payload)) return item.value;
      }
    }
    await wait('ready');
    return { wait, close: () => controller.abort() };
  } catch (error) {
    controller.abort();
    throw error;
  }
}

export const newMessage = (body, parent_id = null) => ({ body, parent_id, client_id: randomUUID() });

export function summary(values) {
  const sorted = [...values].sort((a, b) => a - b);
  const percentile = fraction => Number(sorted[Math.ceil(sorted.length * fraction) - 1].toFixed(2));
  return { samples: sorted.length, p50_ms: percentile(0.5), p95_ms: percentile(0.95), max_ms: percentile(1) };
}
