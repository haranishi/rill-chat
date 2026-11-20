import { api } from './api.js';
import { examples } from './demo.js';
import { icon } from './icons.js';
import { avatar, dayLabel, esc, roomLabel, userById } from './view.js';

const sourceUrl = 'https://github.com/haranishi/rill-chat';
const $ = selector => document.querySelector(selector);
const field = (label,name,attrs) =>
  '<label class="field"><span>' + label + '</span><input name="' + name + '" ' + attrs + ' required></label>';
const primary = label => '<button class="primary" type="submit">' + label + icon('arrow') + '</button>';
const feedback = '<p class="form-error" role="alert" hidden></p>';

export function createDialogs(app) {
  const dialog = $('#dialog');
  const body = $('#dialog-body');
  let viewId = 0;
  let resultList = [];
  let timer;
  function close() { dialog.close(); }
  function show(title, html, className = '') {
    viewId++;
    clearTimeout(timer);
    dialog.className = className;
    body.innerHTML = '<div class="dialog-heading"><h2 id="dialog-title">' + esc(title) +
      '</h2><button type="button" class="icon-button" data-close aria-label="閉じる">' + icon('x') + '</button></div>' + html;
    dialog.setAttribute('aria-labelledby', 'dialog-title');
    if (!dialog.open) dialog.showModal();
  }
  dialog.addEventListener('close', () => { viewId++; clearTimeout(timer); resultList = []; body.replaceChildren(); });
  dialog.addEventListener('click', event => {
    if (event.target === dialog || event.target.closest('[data-close]')) close();
  });
  function formHandler(selector, run) {
    const form = $(selector);
    const submit = form.querySelector('[type=submit]');
    form.addEventListener('submit', async event => {
      event.preventDefault();
      if (submit.disabled) return;
      const errorBox = form.querySelector('.form-error');
      submit.disabled = true; errorBox.hidden = true;
      try { await run(Object.fromEntries(new FormData(form)), form); }
      catch (error) { errorBox.textContent = error.message; errorBox.hidden = false; }
      finally { submit.disabled = false; }
    });
  }
  function auth(mode = 'create') {
    const entries = {create:'新規作成',join:'招待で参加',login:'ログイン'};
    const title = {create:'ワークスペースを作る',join:'チームの招待に参加',login:'おかえりなさい'}[mode];
    const lead = {create:'チームの名前と、あなたのログイン情報を登録してください。',join:'チームから届いた招待コードで、アカウントを作成します。',login:'登録したユーザー名とパスワードを入力してください。'}[mode];
    let fields = '';
    if (mode === 'create') fields += field('ワークスペース名','workspace','maxlength="48" placeholder="例）デザイン部" autocomplete="organization"');
    if (mode === 'join') fields += field('招待コード','code','maxlength="256" autocomplete="off" spellcheck="false" autocapitalize="off"');
    if (mode !== 'login') fields += field('表示名','name','maxlength="48" autocomplete="nickname" placeholder="会話で使う名前"');
    fields += field('ユーザー名','handle','minlength="3" maxlength="32" pattern="[a-zA-Z0-9_\\-]{3,32}" autocomplete="username" autocapitalize="none" spellcheck="false" placeholder="英数字・ハイフン・下線で3〜32字"');
    fields += field('パスワード','password','type="password" ' + (mode === 'login' ? 'autocomplete="current-password"' : 'minlength="10" autocomplete="new-password" placeholder="10文字以上"'));
    show(title, '<div class="auth-tabs" role="group" aria-label="アカウント操作">' +
      Object.entries(entries).map(([key,label]) => '<button data-auth="' + key + '" aria-pressed="' + (key === mode) + '">' + label + '</button>').join('') +
      '</div><p class="dialog-lead">' + lead + '</p><form id="auth-form">' + fields + feedback +
      primary({create:'作成して始める',join:'チームに参加',login:'ログイン'}[mode]) + '</form>');
    body.querySelectorAll('[data-auth]').forEach(button => button.onclick = () => auth(button.dataset.auth));
    formHandler('#auth-form', async values => {
      await api('/auth/' + mode, {...values, handle:values.handle.trim().toLowerCase()});
      await app.enter(); close();
    });
    body.querySelector('input')?.focus();
  }
  function channel() {
    if (app.sample()) return auth();
    show('チャンネルを作成', '<form id="channel-form">' + field('チャンネル名','name','maxlength="48" placeholder="例）design"') +
      '<label class="field"><span>トピック <small>任意</small></span><input name="topic" maxlength="180" placeholder="何について話す場所ですか？"></label>' +
      feedback + primary('作成する') + '</form>');
    formHandler('#channel-form', async values => { const room = await api('/channels',values); await app.addRoom(room); close(); });
    body.querySelector('input')?.focus();
  }
  function people(forDm = false) {
    const state = app.state();
    const list = state.members.filter(p => !forDm || p.id !== state.me?.id);
    const rows = list.map(p => {
      const canDm = forDm && p.id !== state.me?.id;
      return '<' + (canDm ? 'button data-person="' + p.id + '"' : 'div') + ' class="person-row">' + avatar(p) +
        '<span class="person-text"><b>' + esc(p.name) + '</b><small>@' + esc(p.handle) + '</small></span>' +
        (canDm ? icon('arrow') : p.id === state.workspace.owner_id ? '<span class="role">オーナー</span>' : '') + '</' + (canDm ? 'button' : 'div') + '>';
    }).join('');
    show(forDm ? 'DM の相手を選ぶ' : 'ワークスペースのメンバー', '<div class="people-list">' +
      (rows || '<p class="muted">メンバーを招待すると、ここから DM を始められます。</p>') + '</div>' + feedback);
    body.querySelectorAll('[data-person]').forEach(button => button.onclick = async () => {
      if (app.sample()) return auth('join');
      button.disabled = true;
      try { const room = await api('/dms',{user_id:Number(button.dataset.person)}); await app.addRoom(room); close(); }
      catch (error) { const el = body.querySelector('.form-error'); if (el) { el.textContent = error.message; el.hidden = false; } }
      finally { button.disabled = false; }
    });
  }
  async function copy(text) {
    try { await navigator.clipboard.writeText(text); app.notify('コピーしました'); }
    catch {
      show('テキストをコピー', '<label class="field"><span>選択したテキストをコピーしてください。</span><textarea id="copy-value" readonly></textarea></label>');
      const area = $('#copy-value'); area.value = text; area.select();
    }
  }
  function invite() {
    if (app.sample()) return auth();
    show('メンバーを招待', '<p class="dialog-lead">招待コードを発行し、参加してほしい人にこのサイトのアドレスと一緒に渡してください。有効期間は24時間です。</p>' +
      '<form id="invite-form">' + feedback + '<button type="submit" id="make-invite" class="primary">' + icon('key') + '招待コードを発行</button></form><div id="invite-result"></div>');
    const id = viewId;
    formHandler('#invite-form', async () => {
      const {code, expires_at} = await api('/invites',{});
      if (id !== viewId) return;
      $('#invite-form').hidden = true;
      $('#invite-result').innerHTML = '<label class="field"><span>招待コード</span><input id="invite-code" readonly></label><p class="muted">有効期限 ' +
        esc(new Date(expires_at).toLocaleString('ja-JP')) + '</p><button class="primary" id="copy-invite">' + icon('copy') + 'コードをコピー</button>';
      $('#invite-code').value = code;
      $('#copy-invite').onclick = () => copy(code);
    });
  }
  function profile() {
    if (app.sample()) return auth('login');
    const {me,workspace} = app.state();
    show('アカウント', '<div class="person-row profile-card">' + avatar(me) + '<span class="person-text"><b>' +
      esc(me.name) + '</b><small>@' + esc(me.handle) + '</small></span></div><dl class="profile-list"><dt>ワークスペース</dt><dd>' +
      esc(workspace.name) + '</dd><dt>役割</dt><dd>' + (workspace.owner_id === me.id ? 'オーナー' : 'メンバー') +
      '</dd></dl><form id="logout-form">' + feedback + '<button class="secondary" id="logout" type="submit">' + icon('exit') + 'ログアウト</button></form>');
    formHandler('#logout-form', async () => { await api('/auth/logout',{}); app.signedOut(); close(); });
  }
  function search() {
    show('ワークスペースを検索', '<label class="search-box">' + icon('search') +
      '<input id="query" type="search" aria-label="メッセージを検索" placeholder="メッセージを検索" autocomplete="off" maxlength="128"></label><div id="results" aria-live="polite"></div>', 'search-dialog');
    const query = $('#query'); const resultBox = $('#results'); const id = viewId;
    let serial = 0;
    const paint = html => { resultBox.innerHTML = html; };
    const prompt = '<p class="search-empty">投稿の言葉を入力して、会話を探せます。</p>';
    paint(prompt);
    query.focus();
    query.addEventListener('input', () => {
      clearTimeout(timer);
      const q = query.value.trim(); const seq = ++serial;
      resultList = [];
      if (!q) return paint(prompt);
      paint('<p class="search-empty">検索中…</p>');
      timer = setTimeout(async () => {
        try {
          const hits = app.sample() ? examples.filter(p => p.body.toLowerCase().includes(q.toLowerCase())).sort((a,b)=>b.id-a.id) :
            await api('/search?q=' + encodeURIComponent(q));
          if (seq !== serial || id !== viewId) return;
          resultList = hits;
          const state = app.state();
          paint(hits.length ? hits.map(post => {
            const room = state.channels.find(c => c.id === post.channel_id);
            const sender = userById(state,post.author_id);
            return '<button class="search-result" data-result="' + post.id + '"><span class="result-meta">' +
              icon(room?.kind === 'dm' ? 'lock' : 'hash') + esc(roomLabel(state,room)) + ' · ' + esc(sender?.name) +
              ' · ' + esc(dayLabel(post.created_at)) + '</span><span>' + esc(post.body) + '</span></button>';
          }).join('') : '<p class="search-empty">一致するメッセージがありません。</p>');
        } catch (error) { if (seq === serial && id === viewId) paint('<p class="form-error">' + esc(error.message) + '</p>'); }
      }, 140);
    });
    resultBox.onclick = event => {
      const button = event.target.closest('[data-result]');
      if (!button) return;
      const post = resultList.find(p => p.id === Number(button.dataset.result));
      if (post) { close(); app.jump(post).catch(error => app.notify(error.message)); }
    };
  }
  function product() {
    const text = 'Rill — Rust 製のオープンソース・チームチャット';
    const encoded = encodeURIComponent(sourceUrl);
    show('Rill について', '<div class="product-intro"><img src="/mark.svg" alt="" width="48" height="48"><p>' + text +
      '</p></div><a class="secondary" href="' + sourceUrl + '" target="_blank" rel="noopener noreferrer">GitHub でソースを見る' + icon('external') +
      '</a><h3>プロジェクトを共有</h3><div class="share-list"><a href="https://x.com/intent/post?text=' + encodeURIComponent(text) + '&amp;url=' + encoded +
      '" target="_blank" rel="noopener noreferrer">X</a><a href="https://social-plugins.line.me/lineit/share?url=' + encoded +
      '" target="_blank" rel="noopener noreferrer">LINE</a><button id="copy-project">リンクをコピー</button>' +
      (navigator.share ? '<button id="native-share">端末で共有</button>' : '') + '</div>');
    $('#copy-project').onclick = () => copy(sourceUrl);
    if ($('#native-share')) $('#native-share').onclick = async () => { try { await navigator.share({title:text,url:sourceUrl}); } catch (error) { if (error.name !== 'AbortError') app.notify('共有できませんでした。リンクをコピーしてください。'); } };
  }
  return {auth,channel,people,invite,profile,search,product};
}
