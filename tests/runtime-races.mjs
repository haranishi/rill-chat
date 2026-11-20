import assert from 'node:assert/strict';
import { request } from 'playwright';
import { account, deadline, newMessage } from './lab.mjs';

// Commit an actual request, then let the test choose when its response reaches the tab.
async function holdResponse(page, url, fetchResponse = route => route.fetch()) {
  const captured = Promise.withResolvers();
  const release = Promise.withResolvers();
  let seen = false;
  const intercept = async route => {
    if (seen) return route.continue();
    seen = true;
    try {
      const response = await fetchResponse(route);
      captured.resolve(response.status());
      await release.promise;
      await route.fulfill({ response });
    } catch (error) {
      captured.reject(error);
      throw error;
    }
  };
  await page.route(url, intercept);
  return {
    committed: () => deadline(captured.promise, 'committed test request'),
    release: () => release.resolve(),
    close: async () => {
      release.resolve();
      await page.unroute(url, intercept);
    }
  };
}

async function openTab(context, origin, errors) {
  const page = await context.newPage();
  page.setDefaultTimeout(10000);
  page.on('pageerror', error => errors.push(error.message));
  await page.addInitScript(() => {
    // Coordinate the delayed response with an observed native SSE invalidation.
    window.observedMessageRefreshes = 0;
    const NativeSource = window.EventSource;
    window.EventSource = class extends NativeSource {
      constructor(...args) {
        super(...args);
        this.addEventListener('refresh', event => {
          if (JSON.parse(event.data).message_id) window.observedMessageRefreshes++;
        });
      }
    };
  });
  await page.goto(origin);
  await page.locator('#self-detail.connected').waitFor();
  return page;
}

async function observeStalePaint(page, selector, attribute, value) {
  return page.evaluate(({ selector, attribute, value }) => {
    window.supersededMessagePainted = false;
    new MutationObserver(() => {
      const node = document.querySelector(selector);
      if ((attribute ? node?.getAttribute(attribute) : node?.textContent) === value) {
        window.supersededMessagePainted = true;
      }
    }).observe(document.querySelector('#messages'), { childList: true, subtree: true, attributes: true });
  }, { selector, attribute, value });
}

async function delayedSave(page, write, origin, post) {
  const url = origin + '/api/messages/' + post.id + '/saved';
  const article = page.locator('#messages [data-mid="' + post.id + '"]');
  const selector = '#messages [data-save="' + post.id + '"]';
  const held = await holdResponse(page, url);
  try {
    await article.hover();
    await page.locator(selector).click();
    assert.equal(await held.committed(), 200);
    await page.locator(selector + '[aria-pressed=true]').waitFor();
    await write('/messages/' + post.id + '/saved', { active: false });
    await page.locator(selector + '[aria-pressed=false]').waitFor();
    await observeStalePaint(page, selector, 'aria-pressed', 'true');
    const reply = page.waitForResponse(url);
    held.release();
    await reply;
    // The notification is downstream of decoding and handling the delayed POST.
    await page.waitForFunction(() => document.querySelector('#toast')?.textContent === '「あとで読む」に保存しました。');
    assert.equal(await page.evaluate(() => window.supersededMessagePainted), false);
    assert.equal(await page.locator(selector).getAttribute('aria-pressed'), 'false');
  } finally {
    await held.close();
  }
}

async function editDuringHistory(page, write, origin, post, alternate) {
  await page.locator('[data-room="' + alternate + '"]').click();
  const held = await holdResponse(page, origin + '/api/channels/' + post.channel_id + '/messages');
  try {
    const marker = await page.evaluate(() => window.observedMessageRefreshes);
    await page.locator('[data-room="' + post.channel_id + '"]').click();
    assert.equal(await held.committed(), 200);
    const body = '履歴の読み込み中に編集した本文';
    await write('/messages/' + post.id + '/edit', { body, expected_edited_at: null });
    await page.waitForFunction(before => window.observedMessageRefreshes > before, marker);
    const selector = '#messages [data-mid="' + post.id + '"] .post-body';
    await observeStalePaint(page, selector, '', post.body);
    held.release();
    await page.locator(selector).getByText(body, { exact: true }).waitFor();
    assert.equal(await page.evaluate(() => window.supersededMessagePainted), false);
  } finally {
    await held.close();
  }
}

async function delayedCookie(current, sameBrowser, otherDevice, origin, credentials) {
  const draft = '別のタブに残した下書き';
  await sameBrowser.locator('#compose-main').fill(draft);
  const url = origin + '/api/auth/password';
  // A detached cookie jar keeps the new cookie off the browser until delivery.
  const upstream = await request.newContext({ storageState: await current.context().storageState() });
  const held = await holdResponse(current, url, route => upstream.post(url, { data: route.request().postDataJSON() }));
  try {
    await current.locator('#account').click();
    await current.locator('#change-password').click();
    await current.locator('[name=current_password]').fill(credentials.password);
    await current.locator('[name=new_password]').fill(account('replacement').password);
    await current.locator('#password-form [type=submit]').click();
    assert.equal(await held.committed(), 200);
    await otherDevice.waitForFunction(() => document.querySelector('#self-detail')?.textContent === 'ログイン');
    // Keep the new cookie off the wire across both devices' revocation checks.
    assert.equal((await sameBrowser.request.get(origin + '/api/state')).status(), 401);
    await new Promise(resolve => setTimeout(resolve, 600));
    assert.equal(await sameBrowser.locator('#compose-main').inputValue(), draft);
    const reply = current.waitForResponse(url);
    held.release();
    await reply;
    await current.locator('#self-detail.connected').waitFor();
    await sameBrowser.locator('#self-detail.connected').waitFor();
    assert.equal(await sameBrowser.locator('#compose-main').inputValue(), draft);
    assert.equal((await otherDevice.request.get(origin + '/api/state')).status(), 401);
    // The other tab resumes delivery with the replacement cookie.
    const proof = 'パスワード変更後の新着メッセージ';
    const state = await (await sameBrowser.request.get(origin + '/api/state')).json();
    const room = state.channels.find(r => r.name === 'general');
    const sent = await current.request.post(origin + '/api/channels/' + room.id + '/messages', { data: newMessage(proof) });
    assert.ok(sent.ok());
    await sameBrowser.locator('#messages').getByText(proof, { exact: true }).waitFor();
  } finally {
    await held.close();
    await upstream.dispose();
  }
}

export async function exerciseRuntimeRaces(browser, origin) {
  const own = await browser.newContext();
  const remote = await browser.newContext();
  const errors = [];
  async function api(context, path, data) {
    const url = origin + '/api' + path;
    const response = data ? await context.request.post(url, { data }) : await context.request.get(url);
    assert.ok(response.ok(), path + ' HTTP ' + response.status());
    const body = await response.text();
    return body ? JSON.parse(body) : null;
  }
  try {
    const member = account('order');
    await api(own, '/auth/create', { ...member, workspace: '独立した通信試験' });
    await api(remote, '/auth/login', { handle: member.handle, password: member.password });
    const state = await api(own, '/state');
    const room = state.channels.find(r => r.name === 'general').id;
    const alt = state.channels.find(r => r.id !== room).id;
    const post = await api(own, '/channels/' + room + '/messages', newMessage('変更前の本文'));
    const current = await openTab(own, origin, errors);
    const sameBrowser = await openTab(own, origin, errors);
    const otherDevice = await openTab(remote, origin, errors);
    const remoteWrite = (path, data) => api(remote, path, data);
    await delayedSave(current, remoteWrite, origin, post);
    await editDuringHistory(current, remoteWrite, origin, post, alt);
    await delayedCookie(current, sameBrowser, otherDevice, origin, member);
    assert.deepEqual(errors, []);
  } finally {
    await Promise.all([own.close(), remote.close()]);
  }
}
