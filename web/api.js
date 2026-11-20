export async function api(path, body) {
  const options = {credentials: 'same-origin', cache: 'no-store', headers: {'Accept':'application/json'}};
  if (body !== undefined) {
    options.method = 'POST';
    options.headers['Content-Type'] = 'application/json';
    options.body = JSON.stringify(body);
  }
  let response;
  try { response = await fetch('/api' + path, options); }
  catch { throw new Error('接続できません。通信が戻ったら、もう一度お試しください。'); }
  const data = await response.json().catch(() => ({}));
  if (!response.ok) {
    const error = new Error(typeof data.error === 'string' ? data.error : '処理に失敗しました。もう一度お試しください。');
    error.status = response.status;
    throw error;
  }
  return data;
}
