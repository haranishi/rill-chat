// This read-only workspace contains fictional names and conversations.
export const people = [
  { id: 1, name: '青木 みな', handle: 'mina' },
  { id: 2, name: '佐伯 蓮', handle: 'ren' },
  { id: 3, name: '小野 すず', handle: 'suzu' },
  { id: 4, name: '森 海斗', handle: 'kaito' }
];
export const preview = {
  workspace: { id: 0, name: 'Fieldwork Studio', owner_id: 1 }, me: null, members: people,
  channels: [
    { id: 1, name: 'general', topic: 'チーム全体のお知らせと、日々の共有。', kind: 'channel' },
    { id: 2, name: 'design', topic: 'デザインの相談や制作中のアイデアを持ち寄る場所。', kind: 'channel' },
    { id: 3, name: 'field-notes', topic: 'フィールドで見つけたものを記録しましょう。', kind: 'channel' },
    { id: 4, name: 'random', topic: 'ちょっとした発見や雑談をどうぞ。', kind: 'channel' },
    { id: 8, kind: 'dm', name: '', topic: '', peer_id: 2 },
    { id: 9, kind: 'dm', name: '', topic: '', peer_id: 3 }
  ]
};
const at = (hour, minute) => new Date(2026, 5, 16, hour, minute).getTime();
const post = (id, channel_id, author_id, body, hour, minute, extras = {}) => ({
  id, channel_id, author_id, body, parent_id: null, created_at: at(hour, minute), reply_count: 0, reactions: [], ...extras
});
export const examples = [
  post(11, 1, 1, 'おはようございます。次のフィールドノートは「森と水辺」がテーマです。今週は表紙と取材先を決めていきましょう。', 9, 5, { reactions: [{emoji:'👍',count:3},{emoji:'🎉',count:2}] }),
  post(12, 1, 2, '表紙の配色を考えました。川沿いを歩いたときの、霧が晴れていく感じをイメージしています。\n> 深い森 #294F43　 朝霧 #A8B7A0　 朝日 #E4A16F', 9, 18, { reply_count: 2, reactions: [{emoji:'❤️',count:3}] }),
  post(13, 1, 3, 'やわらかい色でいいですね。扉のページにも同じ緑を使いたいです。', 9, 24),
  post(14, 1, 4, '川沿いの下見は木曜に行ってきます。撮ってきてほしい風景があれば、#field-notes に書いてください！', 9, 41, {reactions:[{emoji:'👍',count:2}]}),
  post(15, 1, 1, '中央の淡い緑、朝の光に近くて好きです。', 9, 22, {parent_id:12}),
  post(16, 1, 2, 'ありがとう。紙にのせたときの見え方も確かめてみます。', 9, 32, {parent_id:12}),
  post(81, 8, 2, '表紙の紙見本、打ち合わせに持っていきますね。', 9, 28),
  post(91, 9, 3, '校正用の原稿がまとまったら声をかけます。', 10, 14),
  post(21, 2, 2, '余白を少し広げた表紙のラフができました。小さな判型でもタイトルが読みやすくなりそうです。', 10, 12, {reactions:[{emoji:'👀',count:2}]}),
  post(31, 3, 4, '今回の下見では、橋の上と水辺の遊歩道から同じ稜線を撮ってみます。', 10, 28),
  post(41, 4, 3, '帰り道の喫茶店で、新しい豆の試飲が始まっていました。ひと息つきたい方におすすめです。', 11, 3)
];
