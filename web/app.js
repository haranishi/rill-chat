import { api } from './api.js';
import { preview, examples } from './demo.js';
import { createDialogs } from './dialogs.js';
import { icon } from './icons.js';
import { avatar, composerMarkup, dayDivider, dayLabel, emptyRoom, esc, palette, postMarkup, renderSwatches, roomLabel, userById } from './view.js';

const $ = s => document.querySelector(s);
const wide = matchMedia('(min-width: 1180px)');
const mobile = matchMedia('(max-width: 700px)');
const timeline = $('#timeline');
const postsBox = $('#messages');
const panel = $('#context-panel');
const seenEvents = new Set();
const unread = new Set();
const pending = new Set();
const drafts = new Map();
let state = preview, sample = true, room = null, posts = [], thread = null, replies = [];
let more = false, moreReplies = false, anchored = false, panelMode = wide.matches ? 'info' : null;
let roomSerial = 0, threadSerial = 0, authSerial = 0, metaSerial = 0;
let stream = null, live = false, toastTimer, emojiMenu, refreshTimer;
let composerKey = '', readingOlder = false;

const dialogs = createDialogs({state:() => state, sample:() => sample, enter, addRoom, jump, signedOut, notify});
document.querySelectorAll('[data-icon]').forEach(el => el.innerHTML = icon(el.dataset.icon));

function notify(text) {
  const toast = $('#toast'); toast.textContent = text; toast.hidden = false;
  clearTimeout(toastTimer); toastTimer = setTimeout(() => toast.hidden = true, 4200);
}
function stopStream() { stream?.close(); stream = null; live = false; clearTimeout(refreshTimer); }
function saveKey(scope, roomId = room?.id, parentId = thread?.id) {
  return 'rill.draft.' + state.workspace.id + '.' + (state.me?.id || 0) + '.' + roomId + '.' + (scope === 'thread' ? parentId : 0);
}
function readDraft(key) {
  if (drafts.has(key)) return drafts.get(key);
  try { const raw = sessionStorage.getItem(key); if (raw) return JSON.parse(raw); } catch { /* storage is optional */ }
  return {body:'',client_id:null};
}
function writeDraft(key, value) {
  if (value.body) drafts.set(key,value); else drafts.delete(key);
  try { if (value.body) sessionStorage.setItem(key, JSON.stringify(value)); else sessionStorage.removeItem(key); } catch { /* memory fallback */ }
}
function idempotencyKey() {
  const bytes = crypto.getRandomValues(new Uint8Array(16));
  return Array.from(bytes, n => n.toString(16).padStart(2,'0')).join('');
}
function roomName(r = room) { return roomLabel(state,r); }
function previewPosts(id, parent = null) { return examples.filter(p => p.channel_id === id && p.parent_id === parent).sort((a,b)=>a.id-b.id); }
function merge(old, newer) {
  const items = new Map(old.map(p=>[p.id,p]));
  for (const p of newer) items.set(p.id,p);
  return [...items.values()].sort((a,b)=>a.id-b.id);
}
function hasNewPost(list,p) { return !list.some(item => item.id === p.id); }
function visiblePeople() {
  return room?.kind === 'dm' ? state.members.filter(p=>p.id === room.peer_id || p.id === (state.me?.id || 1)) : state.members;
}
function setNavigation(open) {
  document.body.classList.toggle('nav-open', open);
  $('#nav-shade').hidden = !open;
  $('#sidebar').inert = mobile.matches && !open;
  $('#main').inert = mobile.matches && open;
  $('#nav-open').setAttribute('aria-expanded',String(open));
  if (open) $('#nav-close').focus();
}
function setConnection(isLive) {
  live = isLive;
  const detail = $('#self-detail');
  detail.textContent = sample ? 'ログイン' : live ? '接続済み' : '再接続中…';
  detail.className = sample ? '' : live ? 'connected' : 'reconnecting';
}
function navButton(r) {
  const dm = r.kind === 'dm', active = room?.id === r.id;
  const glyph = dm ? avatar(userById(state,r.peer_id),'tiny') : '<span class="nav-hash">#</span>';
  return '<button data-room="' + r.id + '" class="room-link' + (active?' selected':'') + '" aria-current="' + (active?'page':'false') + '">' +
    glyph + '<span class="room-link-text">' + esc(roomName(r)) + '</span>' +
    (unread.has(r.id) ? '<span class="unread-dot" aria-label="未読"></span>':'') + '</button>';
}
function paintNav() {
  $('#workspace-name').textContent = state.workspace.name;
  $('#workspace-caption').textContent = sample ? 'デモワークスペース' : 'ワークスペース';
  $('#crumb-workspace').textContent = state.workspace.name;
  $('#preview-tag').hidden = !sample;
  $('#channels').innerHTML = state.channels.filter(c=>c.kind==='channel').map(navButton).join('');
  const list = state.channels.filter(c=>c.kind==='dm');
  $('#dms').innerHTML = list.length ? list.map(navButton).join('') : '<button class="first-dm" data-first-dm>会話を始める</button>';
  $('#self-avatar').innerHTML = avatar(state.me);
  $('#self-name').textContent = state.me?.name || 'ゲスト';
  $('#team-action').hidden = !sample && state.me.id !== state.workspace.owner_id;
  $('#team-action').lastElementChild.textContent = sample ? 'チームで使う' : 'メンバーを招待';
  setConnection(live);
}
function paintHeader() {
  if (!room) return;
  const dm = room.kind === 'dm';
  $('#room-kind').innerHTML = dm ? icon('lock') : '#';
  $('#room-name').textContent = roomName();
  $('#room-topic').textContent = dm ? '二人だけの会話' : room.topic || 'ワークスペースのチャンネル';
  $('#crumb-section').textContent = dm ? 'ダイレクトメッセージ' : 'チャンネル';
  $('#panel-toggle').setAttribute('aria-label',dm ? '会話情報' : 'チャンネル情報');
  document.title = (dm?'':'# ') + roomName() + ' · ' + state.workspace.name + ' | Rill';
}
function historyHtml(items, inThread = false) {
  let lastDay = '';
  return items.map(p => {
    const day = dayLabel(p.created_at);
    const divider = day !== lastDay && !inThread ? dayDivider(p.created_at) : '';
    lastDay = day;
    return divider + postMarkup(p,state,{inThread});
  }).join('');
}
function drawTimeline({bottom = false} = {}) {
  postsBox.innerHTML = posts.length ? historyHtml(posts) : emptyRoom(state,room);
  renderSwatches(postsBox);
  const older = $('#load-older'); older.hidden = !more;
  let latest = $('#to-latest');
  if (anchored && !latest) {
    latest = document.createElement('button'); latest.id = 'to-latest'; latest.className = 'latest-button';
    latest.textContent = '最新の会話へ'; latest.onclick = () => openRoom(room.id,{force:true});
    $('#conversation').append(latest);
  } else if (!anchored) latest?.remove();
  if (bottom) timeline.scrollTop = timeline.scrollHeight;
}
function appendPost(p, list, box, inThread) {
  if (!hasNewPost(list,p)) return;
  const previous = list.at(-1);
  if (previous && p.id < previous.id) return false;
  let html = '';
  if (!inThread && (!previous || dayLabel(previous.created_at)!==dayLabel(p.created_at))) html += dayDivider(p.created_at);
  if (!list.length) box.replaceChildren();
  list.push(p);
  box.insertAdjacentHTML('beforeend', html + postMarkup(p,state,{inThread}));
  renderSwatches(box.lastElementChild);
  return true;
}
function updateMessage(p) {
  for (const list of [posts,replies]) {
    const n = list.findIndex(item=>item.id===p.id);
    if (n >= 0) list[n] = p;
  }
  if (thread?.id === p.id) thread = p;
  document.querySelectorAll('[data-mid="' + p.id + '"]').forEach(el => {
    const template = document.createElement('template');
    template.innerHTML = postMarkup(p,state,{inThread:panel.contains(el),parent:el.classList.contains('thread-parent')});
    const node = template.content.firstElementChild; el.replaceWith(node); renderSwatches(node);
  });
}
function receive(p) {
  if (!room || seenEvents.has(p.id)) return;
  seenEvents.add(p.id); if (seenEvents.size > 1200) seenEvents.delete(seenEvents.values().next().value);
  if (!state.members.some(m=>m.id===p.author_id) || !state.channels.some(r=>r.id===p.channel_id)) scheduleMeta();
  if (p.channel_id !== room.id) {
    if (p.author_id !== state.me?.id) { unread.add(p.channel_id); paintNav(); }
    return;
  }
  if (anchored && !p.parent_id) {
    if (p.author_id===state.me?.id) openRoom(room.id,{force:true});
    return;
  }
  if (p.parent_id) {
    const parent = posts.find(row=>row.id===p.parent_id) || (thread?.id===p.parent_id ? thread : null);
    if (parent) refreshOne(parent.id).catch(() => {});
    if (thread?.id === p.parent_id && hasNewPost(replies,p)) {
      const box = $('#thread-posts');
      if (box) {
        const scroller = $('#thread-scroll');
        const atBottom = scroller.scrollHeight - scroller.clientHeight - scroller.scrollTop < 100;
        if (appendPost(p,replies,box,true) === false) { replies = merge(replies,[p]); drawReplies(); }
        if (atBottom || p.author_id===state.me?.id) scroller.scrollTop = scroller.scrollHeight;
      }
    }
  } else {
    const atBottom = timeline.scrollHeight - timeline.clientHeight - timeline.scrollTop < 100;
    if (appendPost(p,posts,postsBox,false) === false) { posts = merge(posts,[p]); drawTimeline(); }
    if (atBottom || p.author_id===state.me?.id) timeline.scrollTop = timeline.scrollHeight;
  }
}
function roomMessages(id, query={}) {
  const search = new URLSearchParams(Object.entries(query).filter(([,v])=>v!=null));
  return api('/channels/' + id + '/messages' + (search.size ? '?' + search : ''));
}
async function refreshOne(id) {
  const old = posts.find(p=>p.id===id) || replies.find(p=>p.id===id) || (thread?.id===id ? thread : null);
  if (!old) return;
  const serial = roomSerial;
  const list = await roomMessages(old.channel_id,{around:id,parent_id:old.parent_id});
  if (serial !== roomSerial) return;
  const current = list.find(p=>p.id===id); if (current) updateMessage(current);
  if (panelMode === 'info') drawInfo();
}
function showFailure(box, error, retry) {
  box.replaceChildren();
  const text = document.createElement('p'); text.className = 'inline-error'; text.textContent = error.message;
  const btn = document.createElement('button'); btn.className = 'text-button'; btn.textContent = 'もう一度読み込む';
  btn.onclick = retry; box.append(text,btn);
}
async function loadRoom({around=null, initial=false, reset=false} = {}) {
  if (!room) return;
  const serial = roomSerial, id = room.id, initialTop = timeline.scrollTop;
  const lastBeforeFetch = posts.at(-1)?.id || 0;
  const nearBottom = timeline.scrollHeight - timeline.scrollTop - timeline.clientHeight < 100;
  try {
    const page = sample ? previewPosts(id) : await roomMessages(id,{around});
    if (serial !== roomSerial) return;
    // A fresh page is contiguous even when a disconnect spanned several pages.
    // Keep events received while the HTTP snapshot was in flight.
    posts = reset ? merge(page, posts.filter(p=>p.id>lastBeforeFetch && !around)) : merge(posts,page);
    if (reset || initial || !posts.length || page[0]?.id === posts[0]?.id) more = !sample && (page.length === 50 || Boolean(around && page.length));
    drawTimeline({bottom:!sample && !around && (initial || nearBottom)});
    if (!initial && !nearBottom) timeline.scrollTop = initialTop;
    if (panelMode==='info') drawInfo();
  } catch (error) {
    if (serial !== roomSerial) return;
    if (!posts.length) showFailure(postsBox,error,()=>loadRoom({around,initial}));
    else notify(error.message);
  }
}
async function olderMessages() {
  if (!room || !posts.length || readingOlder) return;
  const serial = roomSerial, before = posts[0].id;
  readingOlder = true;
  try {
    const page = await roomMessages(room.id,{before});
    if (serial !== roomSerial) return;
    const height = timeline.scrollHeight, top = timeline.scrollTop;
    posts = merge(page,posts); more = page.length===50; drawTimeline();
    timeline.scrollTop = top + timeline.scrollHeight - height;
  } catch (error) { notify(error.message); }
  finally { readingOlder = false; }
}
async function openRoom(id,{force=false,around=null}={}) {
  const next = state.channels.find(c=>c.id === id);
  if (!next) return;
  setNavigation(false);
  if (!force && !around && room?.id===id) return;
  roomSerial++; threadSerial++; room = next; posts = []; thread = null; replies = []; more = false; anchored = Boolean(around);
  if (panelMode==='thread') panelMode = wide.matches ? 'info' : null;
  unread.delete(id); clearEmoji();
  if (!sample) {
    try { sessionStorage.setItem('rill.room.' + state.me.id,String(id)); } catch { /* optional */ }
  }
  paintNav(); paintHeader(); drawPanel(); mountComposer('main');
  $('#load-older').hidden = true; postsBox.innerHTML = '<div class="loading">読み込み中…</div>';
  $('#to-latest')?.remove();
  await loadRoom({around,initial:true});
}
async function addRoom(next) {
  if (!state.channels.some(c=>c.id===next.id)) state = {...state,channels:[...state.channels,next]};
  await openRoom(next.id);
}
function clearEmoji() { emojiMenu?.remove(); emojiMenu = null; }
function mountComposer(scope) {
  const region = scope==='main' ? $('#composer-region') : $('#thread-composer');
  if (!region || !room) return;
  const label = (room.kind==='dm'?'':'#') + roomName();
  region.innerHTML = composerMarkup(scope,label,sample);
  const form = $('#form-' + scope), area = $('#compose-' + scope), send = $('#send-' + scope);
  const key = saveKey(scope); if (scope==='main') composerKey = key;
  area.value = readDraft(key).body;
  const resize = () => { area.style.height='auto'; area.style.height = Math.min(area.scrollHeight,118) + 'px'; };
  const update = () => { send.disabled = !sample && (pending.has(key) || !area.value.trim()); resize(); };
  area.addEventListener('input', () => { writeDraft(key,{body:area.value,client_id:null}); update(); });
  area.addEventListener('keydown', event => {
    if (event.key==='Enter' && !event.shiftKey && !event.isComposing && event.keyCode!==229 && !mobile.matches) { event.preventDefault(); form.requestSubmit(); }
  });
  if (sample) area.onclick = () => dialogs.auth('login');
  form.onsubmit = async event => {
    event.preventDefault(); if (sample) return dialogs.auth('login');
    if (pending.has(key) || !area.value.trim()) return;
    const body = area.value, parent = scope==='thread' ? thread?.id : null, channel = room.id;
    if (Array.from(body).length > 4000) return notify('メッセージは4,000文字以内で入力してください。');
    const stored = readDraft(key), client_id = stored.client_id || idempotencyKey();
    writeDraft(key,{body,client_id}); pending.add(key); update();
    try {
      const p = await api('/channels/' + channel + '/messages',{body,client_id,parent_id:parent});
      if (readDraft(key).client_id===client_id) {
        writeDraft(key,{body:'',client_id:null});
        if (area.value===body) area.value = '';
      }
      receive(p);
      const currentArea = $('#compose-' + scope);
      if (currentArea !== area && saveKey(scope) === key && !readDraft(key).body && currentArea) currentArea.value = '';
    } catch (error) { notify(error.message); }
    finally {
      pending.delete(key); update();
      if (!area.isConnected && (scope==='main' ? composerKey : saveKey(scope)) === key) mountComposer(scope);
    }
  };
  update();
}
function panelHeading(title) {
  return '<div class="panel-heading"><h2>' + esc(title) + '</h2><button id="panel-close" class="icon-button" aria-label="パネルを閉じる">' + icon('x') + '</button></div>';
}
function bindClose() {
  $('#panel-close').onclick = () => { panelMode=null; thread=null; threadSerial++; drawPanel(); $('#panel-toggle').focus(); };
}
function drawInfo() {
  if (!room || panelMode !== 'info') return;
  const dm = room.kind==='dm', people = visiblePeople();
  const threads = posts.filter(p=>p.reply_count>0).slice(-3).reverse();
  const members = people.slice(0,5).map(p => '<li class="person-row">' + avatar(p,'small') +
    '<span class="person-text"><b>' + esc(p.name) + '</b><small>@' + esc(p.handle) + '</small></span>' +
    (state.workspace.owner_id===p.id?'<span class="role">オーナー</span>':'') + '</li>').join('');
  panel.innerHTML = panelHeading(dm?'会話情報':'チャンネル情報') + '<div class="details-scroll"><div class="room-portrait"><img src="/river.png" alt="" width="880" height="416"></div>' +
    '<div class="access-note">' + icon(dm?'lock':'users') + (dm?'この会話は二人だけに表示されます':'ワークスペースの全員が参加できます') + '</div>' +
    '<section class="detail-group"><div class="detail-label"><h3>メンバー <span>' + people.length + '</span></h3>' +
    (!dm?'<button class="mini-icon" data-members aria-label="メンバーをすべて表示">' + icon('right') + '</button>':'') +
    '</div><ul class="people-list">' + members + '</ul></section>' +
    '<section class="detail-group"><div class="detail-label"><h3>最近のスレッド</h3>' + icon('thread') + '</div>' +
    (threads.length ? threads.map(p => '<button class="thread-card" data-reply="' + p.id + '"><span>' +
      esc(p.body.split('\n')[0]) + '</span><small>' + p.reply_count + '件の返信 ' + icon('right') + '</small></button>').join('') :
      '<p class="subtle">メッセージに返信すると、ここに表示されます。</p>') + '</section></div>';
  bindClose();
}
function drawReplies() {
  const box = $('#thread-posts');
  if (!box) return;
  box.innerHTML = replies.length ? historyHtml(replies,true) : '<p class="no-replies">最初の返信を書きましょう。</p>';
  renderSwatches(box); $('#older-replies').hidden = !moreReplies;
}
async function loadReplies({older=false,around=null,reset=false}={}) {
  if (!thread || !room) return;
  const serial = threadSerial, scroller = $('#thread-scroll');
  const beforeHeight = scroller.scrollHeight, beforeTop = scroller.scrollTop, lastBeforeFetch = replies.at(-1)?.id || 0;
  const page = sample ? previewPosts(room.id,thread.id) : await roomMessages(room.id,{parent_id:thread.id,before:older?replies[0]?.id:null,around});
  if (serial !== threadSerial) return;
  if (reset) replies = merge(page,replies.filter(p=>p.id>lastBeforeFetch && !around));
  else replies = older ? merge(page,replies) : merge(replies,page);
  if (reset || older || page[0]?.id === replies[0]?.id) moreReplies = !sample && (page.length===50 || Boolean(around && page.length));
  drawReplies();
  if (older) scroller.scrollTop = beforeTop + scroller.scrollHeight - beforeHeight;
}
function drawPanel() {
  const show = panelMode && room;
  panel.hidden = !show; $('#panes').classList.toggle('with-panel',Boolean(show));
  $('#panel-toggle').setAttribute('aria-expanded',String(Boolean(show)));
  if (!show) { panel.replaceChildren(); return; }
  if (panelMode==='info') return drawInfo();
  if (!thread) return;
  panel.innerHTML = panelHeading('スレッド') + '<div id="thread-scroll"><div class="thread-origin">' +
    postMarkup(thread,state,{inThread:true,parent:true}) + '</div><div class="thread-divider">返信</div>' +
    '<button id="older-replies" class="text-button" hidden>以前の返信を読む</button><div id="thread-posts"></div></div><div id="thread-composer"></div>';
  bindClose(); renderSwatches(panel); drawReplies(); mountComposer('thread');
  $('#older-replies').onclick = () => loadReplies({older:true}).catch(error=>notify(error.message));
}
async function openThread(id,{around=null}={}) {
  let parent = posts.find(p=>p.id===id);
  if (!parent) {
    const serial = roomSerial;
    const page = sample ? previewPosts(room.id) : await roomMessages(room.id,{around:id});
    if (serial !== roomSerial) return;
    parent = page.find(p=>p.id===id);
  }
  if (!parent) return notify('このメッセージを開けません。');
  threadSerial++; panelMode='thread'; thread=parent; replies=[]; moreReplies=false; drawPanel();
  if (!wide.matches) $('#panel-close').focus();
  try { await loadReplies({around}); }
  catch (error) { const box=$('#thread-posts'); if (box) showFailure(box,error,()=>openThread(id,{around})); }
}
function togglePanel() {
  if (panelMode==='info') panelMode=null; else panelMode='info';
  thread=null; threadSerial++; clearEmoji(); drawPanel();
  if (panelMode && !wide.matches) $('#panel-close').focus();
}
async function jump(post) {
  if (!state.channels.some(c=>c.id===post.channel_id) && !sample) await syncMeta();
  const root = post.parent_id || post.id;
  await openRoom(post.channel_id,{force:true,around:root});
  if (post.parent_id) await openThread(root,{around:post.id});
  const target = (post.parent_id?panel:postsBox).querySelector('[data-mid="' + post.id + '"]');
  if (target) {
    target.scrollIntoView({block:'center'}); target.classList.add('highlight');
    target.tabIndex=-1; target.focus({preventScroll:true});
    setTimeout(()=>target.classList.remove('highlight'),2100);
  }
}
function openEmoji(button,{postId=null,scope=null}) {
  const old = emojiMenu?.dataset.anchor;
  clearEmoji(); const anchor = postId?'p'+postId:'c'+scope; if (old===anchor) return;
  if (sample) return dialogs.auth('login');
  const pop = document.createElement('div'); pop.className='emoji-picker'; pop.dataset.anchor=anchor;
  pop.setAttribute('role','group'); pop.setAttribute('aria-label','絵文字を選択');
  pop.innerHTML = palette.map(e => '<button type="button" aria-label="' + e + '"' +
    (postId ? ' data-react="' + postId + '" data-emoji="' + e + '"' : ' data-insert="' + e + '" data-scope="' + scope + '"') + '>' + e + '</button>').join('');
  const rect = button.getBoundingClientRect(); document.body.append(pop); emojiMenu=pop;
  pop.style.left=Math.max(8, Math.min(innerWidth-pop.offsetWidth-8,rect.left)) + 'px';
  pop.style.top=(rect.top>=58 ? rect.top-pop.offsetHeight-5 : rect.bottom+4) + 'px';
  pop.querySelector('button').focus();
}
const changingReactions = new Set();
async function react(id,emoji) {
  if (sample) return dialogs.auth('login');
  const p = [...posts,...replies,thread].find(row=>row?.id===id);
  if (!p) return;
  const key = id + emoji; if (changingReactions.has(key)) return;
  const active = !p.reactions.find(r=>r.emoji===emoji)?.mine;
  changingReactions.add(key);
  try { const next = await api('/messages/'+id+'/reactions',{emoji,active}); if (room?.id===next.channel_id) updateMessage(next); }
  catch (error) { notify(error.message); }
  finally { changingReactions.delete(key); }
}
async function syncMeta() {
  if (sample) return;
  const version = authSerial, seq = ++metaSerial;
  const fresh = await api('/state');
  if (version!==authSerial || seq!==metaSerial) return;
  const newAuthors = fresh.members.length !== state.members.length;
  state=fresh; room=state.channels.find(c=>c.id===room?.id) || room;
  paintNav(); paintHeader();
  if (newAuthors && posts.length) { drawTimeline(); if (thread) drawReplies(); }
  if (panelMode==='info') drawInfo();
}
function scheduleMeta() {
  clearTimeout(refreshTimer);
  refreshTimer=setTimeout(()=>syncMeta().catch(error=>notify(error.message)),40);
}
async function reconcile() {
  const version=authSerial;
  try {
    await syncMeta();
    if (version!==authSerial || !room) return;
    if (anchored && posts.length) await loadRoom({around:posts[Math.floor(posts.length/2)].id,reset:true});
    else await loadRoom({reset:true});
    if (thread) {
      await refreshOne(thread.id);
      await loadReplies({reset:true});
    }
  } catch (error) {
    if (version!==authSerial) return;
    if (error.status===401) { signedOut(); notify('もう一度ログインしてください。'); }
    else notify(error.message);
  }
}
function connect() {
  stopStream();
  const source = new EventSource('/api/events'); stream=source;
  const current = () => stream===source;
  source.addEventListener('ready',()=>{ if (current()) { setConnection(true); reconcile(); } });
  source.addEventListener('resync',()=>{ if (current()) reconcile(); });
  source.addEventListener('message',event=>{ if (current()) receive(JSON.parse(event.data)); });
  source.addEventListener('refresh',event=>{
    if (!current()) return;
    const hint=JSON.parse(event.data || '{}');
    if (!hint.channel_id) scheduleMeta();
    else if (hint.channel_id===room?.id) {
      if (hint.message_id) refreshOne(hint.message_id).catch(()=>{});
      else reconcile();
    }
  });
  source.onerror=async ()=>{
    if (!current()) return; setConnection(false);
    try { await api('/state'); } catch (error) { if (current() && error.status===401) { signedOut(); notify('セッションが終了しました。'); } }
  };
}
function resetView() {
  authSerial++; metaSerial++; roomSerial++; threadSerial++; stopStream(); clearEmoji();
  seenEvents.clear(); unread.clear(); room=null; thread=null; replies=[]; posts=[]; more=false; anchored=false;
  panelMode=wide.matches?'info':null; setNavigation(false);
}
async function enter() {
  const data=await api('/state');
  resetView(); state=data; sample=false;
  let last=0; try { last=Number(sessionStorage.getItem('rill.room.'+state.me.id)); } catch { /* optional */ }
  const first=state.channels.find(c=>c.id===last) || state.channels.find(c=>c.kind==='channel'&&c.name==='general') || state.channels[0];
  if (first) await openRoom(first.id,{force:true}); connect();
}
function signedOut() {
  const prefix='rill.draft.'+state.workspace.id+'.'+(state.me?.id||0)+'.';
  for (const key of drafts.keys()) if (key.startsWith(prefix)) drafts.delete(key);
  try { for (const key of Object.keys(sessionStorage)) if (key.startsWith(prefix)) sessionStorage.removeItem(key); } catch { /* optional */ }
  resetView(); state=preview; sample=true; openRoom(preview.channels[0].id,{force:true});
}
document.addEventListener('click', event => {
  const b=event.target.closest('button');
  if (!b) { if (!event.target.closest('.emoji-picker')) clearEmoji(); return; }
  const d=b.dataset;
  if (!b.closest('.emoji-picker') && !d.emojiFor && !d.composeEmoji) clearEmoji();
  if (d.room) openRoom(Number(d.room));
  else if ('firstDm' in d) dialogs.people(true);
  else if ('members' in d) dialogs.people();
  else if (d.reply) openThread(Number(d.reply)).catch(error=>notify(error.message));
  else if (d.emojiFor) openEmoji(b,{postId:Number(d.emojiFor)});
  else if (d.composeEmoji) openEmoji(b,{scope:d.composeEmoji});
  else if (d.react) { clearEmoji(); react(Number(d.react),d.emoji); }
  else if (d.insert) {
    const field=$('#compose-'+d.scope); if (!field) return;
    field.setRangeText(d.insert,field.selectionStart,field.selectionEnd,'end'); field.dispatchEvent(new Event('input')); clearEmoji(); field.focus();
  }
});
$('#search-open').onclick = dialogs.search;
$('#product').onclick = dialogs.product;
$('#team-action').onclick = () => sample ? dialogs.auth() : dialogs.invite();
$('#account').onclick = dialogs.profile;
$('#new-channel').onclick = dialogs.channel;
$('#new-dm').onclick = () => dialogs.people(true);
$('#load-older').onclick = olderMessages;
$('#panel-toggle').onclick = togglePanel;
$('#nav-open').onclick = () => setNavigation(true);
$('#nav-close').onclick = $('#nav-shade').onclick = () => {setNavigation(false);$('#nav-open').focus();};
document.addEventListener('keydown',event=>{
  if ((event.metaKey||event.ctrlKey) && event.key.toLowerCase()==='k') { event.preventDefault(); if (!$('#dialog').open) dialogs.search(); }
  if (event.key==='Escape' && !$('#dialog').open) {
    if (emojiMenu) clearEmoji();
    else if (document.body.classList.contains('nav-open')) { setNavigation(false); $('#nav-open').focus(); }
    else if (panelMode && !wide.matches) $('#panel-close')?.click();
  }
});
if (!/Mac|iPhone|iPad/.test(navigator.platform)) $('#search-open kbd').textContent='Ctrl K';
wide.addEventListener('change',()=>{ if (panelMode!=='thread') panelMode=wide.matches?'info':null; drawPanel(); });
mobile.addEventListener('change',()=>setNavigation(false));
setNavigation(false);
enter().catch(error=>{
  signedOut();
  if (error.status!==401) notify('サーバーに接続できないため、デモを表示しています。');
});
