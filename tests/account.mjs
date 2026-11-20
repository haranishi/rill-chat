import assert from 'node:assert/strict';

async function currentCookie(context, origin) {
  return (await context.cookies(origin)).find(cookie => cookie.name === 'rill_session')?.value;
}

export async function exercisePasswordRotation({ current, other, author, origin, peer, newPassword, wrongPassword, saved, postFromUi, openNavigation, authenticate, capture }) {
  const who = page => page.request.get(origin + '/api/state');
  const cookie = await currentCookie(current.context(), origin);
  assert.ok(cookie, 'Current session is authenticated');
  await openNavigation(current);
  await current.locator('#account').click();
  await current.locator('#change-password').click();
  const form = current.locator('#password-form');
  await capture(current, 'password-390');
  const original = form.locator('[name=current_password]');
  const replacement = form.locator('[name=new_password]');
  await original.fill(wrongPassword);
  await replacement.fill(newPassword);
  await postFromUi(current, '/auth/password', () => form.locator('[type=submit]').click(), 401);
  await form.getByRole('alert').waitFor();
  assert.equal((await who(other)).status(), 200, 'Failed reauthentication keeps other sessions valid');

  await original.fill(peer.password);
  await replacement.fill(newPassword);
  await postFromUi(current, '/auth/password', () => form.locator('[type=submit]').click());
  await current.locator('#dialog').waitFor({ state: 'hidden' });
  const rotated = await currentCookie(current.context(), origin);
  assert.ok(rotated && rotated !== cookie, 'The current browser receives a fresh session token');
  assert.equal((await who(current)).status(), 200);
  assert.equal((await who(author)).status(), 200, 'Other people remain signed in');
  assert.equal((await who(other)).status(), 401, 'The previous device loses its API session');
  await other.waitForFunction(() => document.querySelector('#self-detail')?.textContent === 'ログイン');
  await current.waitForFunction(() => document.querySelector('#self-detail')?.textContent === '接続済み');
  assert.equal(await other.getByText(saved.replyText, { exact: true }).count(), 0, 'Previous device discards real conversations');

  await other.locator('#account').click();
  await other.locator('[data-auth=login]').click();
  const login = other.locator('#auth-form');
  await login.locator('[name=handle]').fill(peer.handle);
  await login.locator('[name=password]').fill(peer.password);
  await postFromUi(other, '/auth/login', () => login.locator('[type=submit]').click(), 401);
  await login.getByRole('alert').waitFor();
  await other.locator('#dialog [data-close]').click();
  await authenticate(other, 'login', { handle: peer.handle, password: newPassword });
  await other.locator('#saved-open').click();
  await other.locator(`#saved-results [data-result="${saved.replyId}"]`).getByText(saved.replyText, { exact: true }).waitFor();
  await other.locator('#dialog [data-close]').click();
}
