import assert from 'node:assert/strict';
import { mkdir } from 'node:fs/promises';
import { join } from 'node:path';
import { chromium } from 'playwright';
import { account, newMessage, session, startLab, subscribe, summary } from './lab.mjs';

const [, , binary, screenshots] = process.argv;
if (!binary) {
  console.error('Usage: node tests/browser.mjs <rill-chat-binary> [screenshot-directory]');
  process.exit(2);
}

const lab = await startLab(binary);
let browser;
let outsiderStream;
const redact = new Set();
const pageErrors = [];
const owner = { ...account('owner'), name: '森 あおい', workspace: '風のスタジオ' };
const peer = { ...account('peer'), name: '高木 陸' };
redact.add(owner.password);
redact.add(peer.password);
let stage = 'launch';

async function checked(response, action) {
  assert.ok(response.ok(), `${action}: HTTP ${response.status()}`);
  const text = await response.text();
  return text ? JSON.parse(text) : undefined;
}

async function postFromUi(page, path, act) {
  const pending = page.waitForResponse(response =>
    new URL(response.url()).pathname === '/api' + path && response.request().method() === 'POST'
  );
  const [response] = await Promise.all([pending, act()]);
  return checked(response, path);
}

async function openNavigation(page) {
  if (await page.locator('#nav-open').isVisible() && !await page.locator('#nav-shade').isVisible()) {
    await page.locator('#nav-open').click();
  }
}

async function chooseRoom(page, id) {
  await openNavigation(page);
  await page.locator(`[data-room="${id}"]`).click();
}

async function authenticate(page, form, credentials) {
  const open = form === 'create' ? '#team-action' : '#account';
  await openNavigation(page);
  await page.locator(open).click();
  await page.locator(`[data-auth="${form}"]`).click();
  const fields = page.locator('#auth-form');
  for (const [key, value] of Object.entries(credentials)) {
    await fields.locator(`[name="${key}"]`).fill(value);
  }
  await postFromUi(page, '/auth/' + form, () => fields.locator('[type=submit]').click());
  await page.locator('#dialog').waitFor({ state: 'hidden' });
}

async function bookPage(viewport = { width: 1440, height: 900 }) {
  const context = await browser.newContext({ viewport, locale: 'ja-JP' });
  const page = await context.newPage();
  page.setDefaultTimeout(8000);
  page.on('pageerror', error => pageErrors.push(error.message));
  await page.goto(lab.origin);
  await page.locator('#messages [data-mid]').first().waitFor();
  return page;
}

async function capture(page, name) {
  if (screenshots) await page.screenshot({ path: join(screenshots, name + '.png'), animations: 'disabled' });
}

async function exerciseAppearance(page) {
  const control = page.getByRole('combobox', { name: '画面の配色' });
  const waitForTheme = theme =>
    page.waitForFunction(theme => document.documentElement.dataset.theme === theme, theme);
  await page.emulateMedia({ colorScheme: 'dark' });
  await page.reload();
  await waitForTheme('light');
  assert.equal(await control.inputValue(), 'light', 'A new browser starts with the light palette');

  await page.locator('#search-open').focus();
  await page.keyboard.press('Tab');
  assert.ok(await control.evaluate(el => el.matches(':focus-visible')), 'Appearance must be reachable by keyboard');
  await control.press('Space');
  await page.waitForFunction(() => document.getElementById('appearance').matches(':open'));
  // Playwright drives the native picker through selectOption; macOS owns its popup keyboard events.
  await control.selectOption('dim');
  await waitForTheme('dim');
  assert.equal(await control.inputValue(), 'dim');
  await page.emulateMedia({ colorScheme: 'light' });

  // A returning reader's palette must be applied even while the first stylesheet is in flight.
  let releaseStylesheet;
  const delayed = new Promise(resolve => { releaseStylesheet = resolve; });
  const hold = async route => { await delayed; await route.continue(); };
  await page.route('**/style.css', hold);
  try {
    await Promise.all([page.waitForRequest('**/style.css'), page.reload({ waitUntil: 'commit' })]);
    await waitForTheme('dim');
  } finally {
    releaseStylesheet();
    await page.waitForLoadState('load');
    await page.unroute('**/style.css', hold);
  }
  assert.equal(await control.inputValue(), 'dim', 'Manual choice survives a reload and OS change');
  await page.locator('#messages [data-mid]').first().waitFor();
  for (const [width, height] of [[390, 844], [768, 1024], [1440, 900]]) {
    await page.setViewportSize({ width, height });
    await assertFits(page);
    const box = await control.boundingBox();
    assert.ok(box && (width > 768 || (box.width >= 44 && box.height >= 44)), 'Touch target size');
    await capture(page, 'preview-dim-' + width);
  }

  await control.selectOption('device');
  await waitForTheme('light');
  await page.emulateMedia({ colorScheme: 'dark' });
  await waitForTheme('dim');
  await page.reload();
  assert.equal(await control.inputValue(), 'device');
  await page.emulateMedia({ colorScheme: 'light' });
  await waitForTheme('light');
  await control.selectOption('light');
}

async function send(page, text, channelId, thread = false) {
  const input = page.locator(thread ? '#compose-thread' : '#compose-main');
  await input.fill(text);
  return postFromUi(page, `/channels/${channelId}/messages`, () => input.press('Enter'));
}

const timelineHas = (page, id) => page.locator(`#messages [data-mid="${id}"]`);

async function assertFits(page) {
  const metrics = await page.evaluate(() => ({
    width: window.innerWidth, document: document.documentElement.scrollWidth,
    timeline: document.querySelector('#timeline').getBoundingClientRect().width
  }));
  assert.ok(metrics.document <= metrics.width + 1, 'Page overflows horizontally');
  assert.ok(metrics.timeline > 260, 'Too little space for the conversation');
}

async function measureBrowser(sender, viewer, channelId) {
  const prefix = '受信計測 ';
  await sender.evaluate(prefix => {
    window.rillKeys = new Map();
    window.addEventListener('keydown', event => {
      const body = event.target.value;
      if (event.key === 'Enter' && body?.startsWith(prefix)) {
        window.rillKeys.set(body, performance.timeOrigin + performance.now());
      }
    }, true);
  }, prefix);
  await viewer.evaluate(prefix => {
    window.rillArrivals = new Map();
    new MutationObserver(() => {
      const now = performance.timeOrigin + performance.now();
      for (const item of document.querySelectorAll('#messages [data-mid]')) {
        const number = item.textContent.match(/受信計測 (\d+) 終/);
        if (!number) continue;
        const body = prefix + number[1] + ' 終';
        if (!window.rillArrivals.has(body)) window.rillArrivals.set(body, now);
      }
    }).observe(document.querySelector('#messages'), { subtree: true, childList: true, characterData: true });
  }, prefix);
  const timings = [];
  for (let n = 0; n < 28; n++) {
    const body = prefix + n + ' 終';
    const event = await send(sender, body, channelId);
    await timelineHas(viewer, event.id).waitFor();
    const start = await sender.evaluate(body => window.rillKeys.get(body), body);
    const arrival = await viewer.evaluate(body => window.rillArrivals.get(body), body);
    assert.ok(Number.isFinite(start) && Number.isFinite(arrival) && arrival >= start, 'Browser timing observation missing');
    if (n >= 4) timings.push(arrival - start);
  }
  return {
    metric: 'Sender Enter keydown to receiver timeline DOM insertion; two browser contexts on loopback',
    browser: 'Chromium ' + browser.version(), warmups: 4, ...summary(timings)
  };
}

try {
  browser = await chromium.launch();
  if (screenshots) await mkdir(screenshots, { recursive: true });
  stage = 'public preview';
  const a = await bookPage();
  for (const [width, height] of [[390, 844], [768, 1024], [1440, 900]]) {
    await a.setViewportSize({ width, height });
    await assertFits(a);
    if (width === 390) await a.locator('#preview-tag').waitFor();
    const anonymousButtons = a.getByRole('button', { name: '', exact: true });
    assert.equal(await anonymousButtons.count(), 0, 'Visible buttons need an accessible name');
    await capture(a, 'preview-' + width);
  }
  stage = 'appearance by keyboard, persistence and device changes';
  await exerciseAppearance(a);
  const b = await bookPage();
  const unsigned = await b.request.get(lab.origin + '/api/state');
  assert.equal(unsigned.status(), 401);

  stage = 'create workspace and invite a second account';
  await authenticate(a, 'create', owner);
  await a.locator('#team-action').click();
  const invite = await postFromUi(a, '/invites', () => a.locator('#make-invite').click());
  redact.add(invite.code);
  const shownCode = await a.locator('#invite-code').evaluate(element => element.value ?? element.textContent);
  assert.ok(shownCode.includes(invite.code), 'Invitation code was not visible to its owner');
  await a.keyboard.press('Escape');
  await authenticate(b, 'join', { ...peer, code: invite.code });
  const state = await checked(await b.request.get(lab.origin + '/api/state'), 'member state');
  const originalChannel = state.channels.find(channel => channel.kind === 'channel');
  assert.ok(originalChannel);
  assert.ok(state.members.some(user => user.name === owner.name));
  assert.equal((await b.request.post(lab.origin + '/api/invites', { data: {} })).status(), 403);

  stage = 'create a channel and send using Japanese input';
  await a.locator('#new-channel').click();
  await a.locator('#channel-form [name=name]').fill('校正室');
  await a.locator('#channel-form [name=topic]').fill('初稿を読んで、細かな表現を整えます。');
  const room = await postFromUi(a, '/channels', () => a.locator('#channel-form [type=submit]').click());
  await chooseRoom(a, room.id);
  await chooseRoom(b, room.id);

  const firstBody = '原稿の見出しを確認しました。';
  const compose = a.locator('#compose-main');
  await compose.fill(firstBody);
  await compose.evaluate(el => {
    el.dispatchEvent(new CompositionEvent('compositionstart', { bubbles: true }));
    el.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', code: 'Enter', isComposing: true, keyCode: 229, bubbles: true, cancelable: true }));
    el.dispatchEvent(new CompositionEvent('compositionend', { bubbles: true }));
  });
  assert.equal(await compose.inputValue(), firstBody);
  assert.equal(await a.locator('#messages [data-mid]').count(), 0);
  const first = await postFromUi(a, `/channels/${room.id}/messages`, () => compose.press('Enter'));
  await timelineHas(b, first.id).waitFor();
  assert.equal(await timelineHas(b, first.id).count(), 1);
  await compose.fill('一行目');
  await compose.press('Shift+Enter');
  await compose.pressSequentially('2');
  assert.equal(await compose.inputValue(), '一行目\n2');
  await compose.fill('');

  stage = 'delivery measurement';
  const browserTiming = await measureBrowser(a, b, room.id);
  console.log(JSON.stringify(browserTiming));

  stage = 'reaction and thread conversation';
  await timelineHas(a, first.id).hover();
  await a.locator(`[data-emoji-for="${first.id}"]`).click();
  await postFromUi(a, `/messages/${first.id}/reactions`, () => a.locator(`[data-react="${first.id}"][data-emoji="👍"]`).last().click());
  const peerReaction = b.locator(`[data-react="${first.id}"][data-emoji="👍"]`);
  await peerReaction.waitFor();
  assert.ok((await peerReaction.innerText()).includes('1'));

  for (const page of [a, b]) {
    await timelineHas(page, first.id).hover();
    await page.locator(`[data-reply="${first.id}"]`).click();
    await page.locator('#compose-thread').waitFor();
  }
  const replyText = 'タイトルの修正をお願いします。';
  const reply = await send(b, replyText, room.id, true);
  await a.locator(`#context-panel [data-mid="${reply.id}"]`).waitFor();
  assert.equal(await timelineHas(a, reply.id).count(), 0, 'Thread reply leaked into main timeline');
  await b.setViewportSize({ width: 390, height: 844 });
  await capture(b, 'thread-390');
  await b.setViewportSize({ width: 1440, height: 900 });

  stage = 'failed send, preserved drafts and retry';
  await chooseRoom(a, originalChannel.id);
  await chooseRoom(a, room.id);
  const saved = '次回の打ち合わせは火曜でどうでしょう。';
  await compose.fill(saved);
  const postUrl = lab.origin + `/api/channels/${room.id}/messages`;
  await a.route(postUrl, route => route.request().method() === 'POST' ? route.abort() : route.continue());
  await Promise.all([a.waitForEvent('requestfailed', req => req.url() === postUrl && req.method() === 'POST'), compose.press('Enter')]);
  await a.unroute(postUrl);
  await chooseRoom(a, originalChannel.id);
  await chooseRoom(a, room.id);
  assert.equal(await compose.inputValue(), saved);
  const retried = await postFromUi(a, `/channels/${room.id}/messages`, () => compose.press('Enter'));
  await timelineHas(b, retried.id).waitFor();
  assert.equal(await b.getByText(saved, { exact: true }).count(), 1);

  stage = 'history, lookup and persistence';
  for (let i = 0; i < 26; i++) {
    await checked(await a.request.post(postUrl, { data: newMessage('校正記録 ' + i) }), 'history setup');
  }
  await a.reload();
  await chooseRoom(a, room.id);
  await a.locator('#load-older').click();
  await timelineHas(a, first.id).waitFor();
  await a.reload();
  await chooseRoom(a, room.id);
  const search = a.locator('#query');
  await a.keyboard.press('Control+k');
  await search.waitFor();
  await search.fill(firstBody);
  await a.locator(`[data-result="${first.id}"]`).click();
  await timelineHas(a, first.id).waitFor();
  await capture(a, 'conversation-1440');
  const fromHistory = await send(a, '過去の原稿も確認しました。', room.id);
  await timelineHas(a, fromHistory.id).waitFor();
  await a.locator('#to-latest').waitFor({ state: 'hidden' });
  await b.context().setOffline(true);
  // Restarting closes the established stream, even on browsers that keep existing
  // TCP connections alive after network emulation switches offline.
  await lab.restart();
  await b.waitForFunction(() => document.querySelector('#self-detail')?.textContent.includes('再接続'));
  let missed, away;
  for (let i = 0; i < 58; i++) {
    away = await checked(await a.request.post(postUrl, { data: newMessage('切断中の連絡 ' + i) }), 'offline message');
    if (i === 0) missed = away;
  }
  assert.equal(await timelineHas(b, away.id).count(), 0);
  await b.context().setOffline(false);
  // EventSource reconnects and the new stream's handshake reloads authoritative history.
  await timelineHas(b, away.id).waitFor({ timeout: 15000 });
  assert.equal(await timelineHas(b, away.id).count(), 1);
  if (!await timelineHas(b, missed.id).count()) await b.locator('#load-older').click();
  await timelineHas(b, missed.id).waitFor();
  await b.reload();
  await chooseRoom(b, room.id);
  const persisted = await checked(await b.request.get(postUrl + `?around=${first.id}`), 'persistent history');
  assert.ok(persisted.some(message => message.id === first.id && message.reply_count === 1));

  stage = 'private DM and search';
  const newcomer = session(lab.origin);
  const privateAccount = account('observer');
  redact.add(privateAccount.password);
  await newcomer.json('/auth/join', { ...privateAccount, code: invite.code });
  outsiderStream = await subscribe(lab.origin, newcomer);
  await a.locator('#new-dm').click();
  const direct = await postFromUi(a, '/dms', () => a.locator(`[data-person="${state.me.id}"]`).click());
  await a.locator('#dialog').waitFor({ state: 'hidden' });
  await a.locator(`[aria-current="page"][data-room="${direct.id}"]`).waitFor();
  const privateBody = '二人で確認する校正前のメモ';
  const dm = await send(a, privateBody, direct.id);
  await chooseRoom(b, direct.id);
  await timelineHas(b, dm.id).waitFor();
  const fence = '校正室の記録を共有しました。';
  const checkedStream = outsiderStream.wait('message', event => {
    assert.notEqual(event.body, privateBody, 'A DM was streamed to another member');
    return event.body === fence;
  });
  await Promise.all([
    checkedStream,
    b.request.post(postUrl, { data: newMessage(fence) }).then(response => checked(response, 'public marker'))
  ]);
  const denied = await newcomer.json('/search?q=' + encodeURIComponent(privateBody));
  assert.equal(denied.length, 0);
  const allowed = await checked(await b.request.get(lab.origin + '/api/search?q=' + encodeURIComponent(privateBody)), 'DM search');
  assert.ok(allowed.some(message => message.id === dm.id));
  assert.ok(!(await newcomer.json('/state')).channels.some(channel => channel.id === direct.id));

  stage = 'mobile navigation and login';
  await b.setViewportSize({ width: 390, height: 844 });
  await b.locator('#nav-open').click();
  await b.locator(`[data-room="${room.id}"]`).click();
  const mobileInput = b.locator('#compose-main');
  await mobileInput.fill('スマートフォンから確認しました。');
  await mobileInput.press('Enter');
  assert.equal(await mobileInput.inputValue(), 'スマートフォンから確認しました。\n');
  const mobilePost = await postFromUi(b, `/channels/${room.id}/messages`, () => b.locator('#send-main').click());
  await timelineHas(b, mobilePost.id).waitFor();
  await assertFits(b);
  await capture(b, 'conversation-390');
  const c = await bookPage();
  await authenticate(c, 'login', { handle: peer.handle, password: peer.password });
  await chooseRoom(c, direct.id);
  await timelineHas(c, dm.id).waitFor();
  await c.locator('#account').click();
  await postFromUi(c, '/auth/logout', () => c.locator('#logout').click());
  assert.equal((await c.request.get(lab.origin + '/api/state')).status(), 401);
  await c.reload();
  await c.locator('#messages [data-mid]').first().waitFor();
  assert.equal(await c.getByText(privateBody, { exact: true }).count(), 0);
  assert.deepEqual(pageErrors, []);
  console.log('Browser acceptance passed: preview, appearance, invitation, channels, IME, reactions, threads, retry, pagination, search, reconnect, restart, DM privacy, mobile and login/logout.');
} catch (error) {
  let report = error.stack || String(error);
  for (const value of redact) if (value) report = report.replaceAll(value, '<redacted>');
  console.error(`Browser acceptance failed during ${stage}\n${report}`);
  process.exitCode = 1;
} finally {
  outsiderStream?.close();
  await browser?.close();
  await lab.dispose();
}
