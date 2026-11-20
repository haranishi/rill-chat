import assert from 'node:assert/strict';

const post = (page, scope, id) => page.locator(`${scope} [data-mid="${id}"]`);
const savedRow = (page, id) => page.locator(`#saved-results [data-result="${id}"]`);

async function actOn(row, action) {
  const button = row.locator(`.post-tools [data-${action}]`);
  if (await row.evaluate(() => matchMedia('(pointer: coarse)').matches)) {
    const box = await button.boundingBox();
    assert.ok(box && box.width >= 44 && box.height >= 44, 'Touch actions need a 44px hit target');
    await button.tap();
    return;
  }
  await row.hover();
  await button.click();
}

async function openSaved(page, openNavigation) {
  await openNavigation(page);
  const [loaded] = await Promise.all([
    page.waitForResponse(r => new URL(r.url()).pathname === '/api/saved' && r.request().method() === 'GET'),
    page.locator('#saved-open').click()
  ]);
  assert.equal(loaded.status(), 200);
  await page.waitForFunction(() => document.querySelector('#saved-status')?.textContent !== '読み込み中…');
}

async function closeDialog(page) {
  await page.locator('#dialog [data-close]').click();
  await page.locator('#dialog').waitFor({ state: 'hidden' });
}

export async function exerciseMessageTools({ author, reader, secondReader, roomId, send, chooseRoom, openNavigation, postFromUi, capture }) {
  for (const page of [author, reader, secondReader]) await chooseRoom(page, roomId);
  const original = '初校の予定は火曜日です。';
  const corrected = '再校の予定は木曜日です。';
  const parent = await send(author, original, roomId);
  const authored = post(author, '#messages', parent.id);
  const received = post(reader, '#messages', parent.id);
  await received.waitFor();
  assert.equal(await received.locator('[data-edit]').count(), 0, 'Other members can only edit their own messages');

  await actOn(authored, 'edit');
  const editForm = author.locator('#edit-form');
  assert.equal(await editForm.locator('[name=body]').inputValue(), original);
  await editForm.locator('[name=body]').fill(corrected);
  await postFromUi(author, `/messages/${parent.id}/edit`, () => editForm.locator('[type=submit]').click());
  await received.getByText(corrected, { exact: true }).waitFor();
  assert.equal(await received.getByText(original, { exact: true }).count(), 0);
  await author.locator('#search-open').click();
  const search = author.locator('#query');
  await search.fill(corrected);
  await author.locator(`#results [data-result="${parent.id}"]`).getByText(corrected, { exact: true }).waitFor();
  await search.fill(original);
  await author.locator('#results').getByText('一致するメッセージがありません。', { exact: true }).waitFor();
  await closeDialog(author);

  for (const page of [author, reader, secondReader]) {
    await actOn(post(page, '#messages', parent.id), 'reply');
    await page.locator('#compose-thread').waitFor();
  }
  const response = '木曜日の午前で確認します。';
  await reader.locator('#compose-thread').fill(response);
  const reply = await postFromUi(reader, `/channels/${roomId}/messages`, () => reader.locator('#send-thread').tap());
  const peerReply = post(author, '#context-panel', reply.id);
  const ownReply = post(reader, '#context-panel', reply.id);
  await peerReply.getByText(response, { exact: true }).waitFor();
  for (const page of [reader, secondReader]) await actOn(post(page, '#context-panel', reply.id), 'edit');
  const revised = '木曜日の十時に確認します。';
  await reader.locator('#edit-form [name=body]').fill(revised);
  await capture(reader, 'edit-reply-390');
  await postFromUi(reader, `/messages/${reply.id}/edit`, () => reader.locator('#edit-form [type=submit]').click());
  await peerReply.getByText(revised, { exact: true }).waitFor();

  const staleDraft = '木曜日の午後で確認します。';
  const stale = secondReader.locator('#edit-form');
  await stale.locator('[name=body]').fill(staleDraft);
  await postFromUi(secondReader, `/messages/${reply.id}/edit`, () => stale.locator('[type=submit]').click(), 409);
  await secondReader.locator('#edit-conflict').getByText(revised, { exact: true }).waitFor();
  assert.equal(await stale.locator('[name=body]').inputValue(), staleDraft);
  await secondReader.locator('#review-edit').click();
  assert.equal(await stale.locator('[name=body]').inputValue(), staleDraft);
  const reconciled = '木曜日の十時に校正結果を確認します。';
  await stale.locator('[name=body]').fill(reconciled);
  await postFromUi(secondReader, `/messages/${reply.id}/edit`, () => stale.locator('[type=submit]').click());
  await peerReply.getByText(reconciled, { exact: true }).waitFor();

  await openSaved(secondReader, openNavigation);
  await postFromUi(reader, `/messages/${reply.id}/saved`, () => actOn(ownReply, 'save'));
  await savedRow(secondReader, reply.id).getByText(reconciled, { exact: true }).waitFor();
  assert.equal(await peerReply.locator('[data-save]').getAttribute('aria-pressed'), 'false');
  await reader.locator('#panel-close').click();
  await postFromUi(reader, `/messages/${parent.id}/saved`, () => actOn(received, 'save'));
  await savedRow(secondReader, parent.id).waitFor();
  await openSaved(author, openNavigation);
  assert.equal(await savedRow(author, parent.id).count(), 0);
  assert.equal(await savedRow(author, reply.id).count(), 0);
  await closeDialog(author);

  await openSaved(reader, openNavigation);
  await savedRow(reader, reply.id).waitFor();
  await capture(reader, 'saved-390');
  await postFromUi(reader, `/messages/${parent.id}/saved`, () => reader.locator(`#saved-results [data-save="${parent.id}"]`).click());
  await savedRow(secondReader, parent.id).waitFor({ state: 'detached' });
  await savedRow(reader, reply.id).click();
  await reader.locator('#dialog').waitFor({ state: 'hidden' });
  await ownReply.getByText(reconciled, { exact: true }).waitFor();
  await closeDialog(secondReader);
  return { replyId: reply.id, replyText: reconciled };
}

export async function exerciseSavedDm({ reader, otherMember, dm, roomId, chooseRoom, openNavigation, postFromUi }) {
  await reader.locator('#panel-close').click();
  await chooseRoom(reader, dm.channel_id);
  const item = post(reader, '#messages', dm.id);
  await postFromUi(reader, `/messages/${dm.id}/saved`, () => actOn(item, 'save'));
  await chooseRoom(reader, roomId);
  await openSaved(reader, openNavigation);
  await savedRow(reader, dm.id).getByText(dm.body, { exact: true }).waitFor();
  await openSaved(otherMember, openNavigation);
  assert.equal(await savedRow(otherMember, dm.id).count(), 0, 'The other DM participant has a separate reading list');
  await closeDialog(otherMember);
  await savedRow(reader, dm.id).click();
  await reader.locator(`[data-room="${dm.channel_id}"][aria-current="page"]`).waitFor({ state: 'attached' });
  await item.getByText(dm.body, { exact: true }).waitFor();
  await postFromUi(reader, `/messages/${dm.id}/saved`, () => actOn(item, 'save'));
  await openSaved(reader, openNavigation);
  assert.equal(await savedRow(reader, dm.id).count(), 0);
  await closeDialog(reader);
}

// Populate the existing test history through the public API, then page through it in the UI.
export async function exerciseSavedPagination({ page, origin, roomId, openNavigation }) {
  const api = path => origin + '/api' + path;
  const latest = await (await page.request.get(api(`/channels/${roomId}/messages`))).json();
  assert.equal(latest.length, 50);
  const earlier = await (await page.request.get(api(`/channels/${roomId}/messages?before=${latest[0].id}`))).json();
  const older = earlier[0];
  assert.ok(older, 'The fixture needs more than one page of messages');
  for (const item of [older, ...latest]) {
    const response = await page.request.post(api(`/messages/${item.id}/saved`), { data: { active: true } });
    assert.equal(response.status(), 200);
  }
  await openSaved(page, openNavigation);
  await page.locator('#saved-older').waitFor();
  assert.equal(await page.locator('#saved-results [data-result]').count(), 50);
  assert.equal(await savedRow(page, older.id).count(), 0);
  await page.locator('#saved-older').click();
  await savedRow(page, older.id).waitFor();
  const ids = await page.locator('#saved-results [data-result]').evaluateAll(rows => rows.map(r => Number(r.dataset.result)));
  assert.deepEqual(ids, [...new Set(ids)].sort((a, b) => b - a));
  await savedRow(page, older.id).click();
  await post(page, '#messages', older.id).waitFor();
}
