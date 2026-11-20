import { icon } from './icons.js';

const escapes = { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' };
export const esc = (value) => String(value ?? '').replace(/[&<>"']/g, char => escapes[char]);
export const palette = ['👍', '🎉', '❤️', '👀'];
export const userById = (state, id) => state.members.find(person => person.id === id);
export function roomLabel(state, room) {
  return room?.kind === 'dm' ? (userById(state, room.peer_id)?.name || 'ダイレクトメッセージ') : (room?.name || '');
}
export function avatar(person, size = '') {
  const name = person?.name || 'ゲスト';
  const initial = Array.from(name.trim())[0] || '?';
  const tone = person ? (person.id + 5) % 6 : 6;
  return '<span class="avatar tone-' + tone + ' ' + size + '" aria-hidden="true">' + esc(initial) + '</span>';
}
const shortTime = new Intl.DateTimeFormat('ja-JP', {hour: '2-digit', minute: '2-digit', hour12:false});
const fullDate = new Intl.DateTimeFormat('ja-JP', {year:'numeric',month:'long',day:'numeric',weekday:'short'});
export const dayLabel = ms => fullDate.format(new Date(ms));
export function dayDivider(ms) { return '<div class="date-divider"><span>' + esc(dayLabel(ms)) + '</span></div>'; }

function inline(line) {
  // Only generate markup for this small set of tokens. User input is always escaped.
  const tokens = /(`[^`\n]+`|\*\*[^*\n]+\*\*|https?:\/\/[^\s<>"'。、]+|#[a-fA-F0-9]{6}\b)/gu;
  let cursor = 0;
  let html = '';
  for (const hit of line.matchAll(tokens)) {
    const text = hit[0];
    html += esc(line.slice(cursor, hit.index));
    if (text.startsWith('`')) html += '<code>' + esc(text.slice(1,-1)) + '</code>';
    else if (text.startsWith('**')) html += '<strong>' + esc(text.slice(2,-2)) + '</strong>';
    else if (text.startsWith('#')) html += '<span class="swatch">' + esc(text) + '</span>';
    else html += '<a href="' + esc(text) + '" target="_blank" rel="noopener noreferrer nofollow">' + esc(text) + '</a>';
    cursor = hit.index + text.length;
  }
  return html + esc(line.slice(cursor));
}
export function messageBody(body) {
  return body.split('\n').map(line => line.startsWith('> ')
    ? '<blockquote>' + inline(line.slice(2)) + '</blockquote>'
    : '<div>' + (inline(line) || '<br>') + '</div>').join('');
}

export function renderSwatches(root) {
  for (const node of root.querySelectorAll('.swatch:not([data-tinted])')) {
    // textContent was produced by the exact six-digit color matcher above.
    node.style.setProperty('--chip', node.textContent);
    node.dataset.tinted = 'true';
  }
}
const attrButton = (attr, label, glyph) =>
  '<button type="button" class="mini-icon" ' + attr + ' aria-label="' + label + '" title="' + label + '">' + icon(glyph) + '</button>';
export function postMarkup(post, state, {inThread = false, parent = false} = {}) {
  const author = userById(state, post.author_id);
  const name = author?.name || 'メンバー';
  const stamp = new Date(post.created_at);
  const pills = post.reactions.map(r =>
    '<button class="reaction' + (r.mine ? ' mine' : '') + '" data-react="' + post.id + '" data-emoji="' + esc(r.emoji) +
    '" aria-pressed="' + Boolean(r.mine) + '" aria-label="' + esc(r.emoji) + ' リアクション ' + r.count + '件"><span>' +
    esc(r.emoji) + '</span>' + r.count + '</button>').join('');
  const reply = inThread ? '' : attrButton('data-reply="' + post.id + '"','スレッドで返信','thread');
  return '<article class="post' + (parent ? ' thread-parent' : '') + '" data-mid="' + post.id + '">' + avatar(author) +
    '<div class="post-content"><div class="post-byline"><strong>' + esc(name) + '</strong><time datetime="' +
    stamp.toISOString() + '" title="' + esc(dayLabel(stamp)) + '">' + esc(shortTime.format(stamp)) + '</time></div>' +
    '<div class="post-body">' + messageBody(post.body) + '</div>' +
    (pills ? '<div class="reactions">' + pills + '</div>' : '') +
    (!inThread && post.reply_count ? '<button class="reply-link" data-reply="' + post.id + '">' + icon('thread') +
    '<span>' + post.reply_count + '件の返信</span>' + icon('right') + '</button>' : '') +
    '</div><div class="post-tools">' + attrButton('data-emoji-for="' + post.id + '"','リアクションを追加','smile') +
    reply + '</div></article>';
}

export function emptyRoom(state, room) {
  const dm = room?.kind === 'dm';
  const title = dm ? roomLabel(state,room) + 'さんへの最初のメッセージ' : '#' + (room?.name || '') + ' で会話を始めましょう';
  const text = dm ? 'この会話を読めるのは二人だけです。' : '下の入力欄から、チームに最初のひとことをどうぞ。';
  return '<div class="empty-room"><span class="empty-glyph">' + icon(dm ? 'lock' : 'hash') + '</span><h2>' +
    esc(title) + '</h2><p>' + text + '</p></div>';
}

export function composerMarkup(scope, label, sample) {
  const hint = sample ? '閲覧用デモ' : 'Enter で送信 · Shift + Enter で改行';
  const placeholder = sample ? 'ログインしてメッセージを送る' : (scope === 'thread' ? 'スレッドに返信…' : label + ' にメッセージ');
  return '<form id="form-' + scope + '" class="composer' + (sample ? ' preview-composer' : '') + '">' +
    '<textarea id="compose-' + scope + '" rows="1" maxlength="8000" aria-label="' + esc(placeholder) +
    '" placeholder="' + esc(placeholder) + '"' + (sample ? ' readonly' : '') + '></textarea>' +
    '<div class="composer-tools"><div class="composer-left">' +
    '<button type="button" data-compose-emoji="' + scope + '" class="mini-icon" aria-label="絵文字を挿入" title="絵文字を挿入">' + icon('smile') + '</button>' +
    '<span class="composer-hint">' + (sample ? icon('book') : '') + hint + '</span></div>' +
    '<button id="send-' + scope + '" class="send-button" type="submit" aria-label="' + (sample ? 'ログインして投稿' : '送信') + '">' + icon('send') + '</button></div></form>';
}
