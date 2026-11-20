//! Fictional, read-only conversations shown before authentication.
use crate::model::{Channel, Message, Reaction, State, User, Workspace};
use js_sys::Date;

pub fn state() -> State {
    let members = [
        (1, "青木 みな", "mina"),
        (2, "佐伯 蓮", "ren"),
        (3, "小野 すず", "suzu"),
        (4, "森 海斗", "kaito"),
    ]
    .into_iter()
    .map(|(id, name, handle)| User {
        id,
        name: name.into(),
        handle: handle.into(),
    })
    .collect();
    let mut channels: Vec<_> = [
        (1, "general", "チーム全体のお知らせと、日々の共有。"),
        (
            2,
            "design",
            "デザインの相談や制作中のアイデアを持ち寄る場所。",
        ),
        (
            3,
            "field-notes",
            "フィールドで見つけたものを記録しましょう。",
        ),
        (4, "random", "ちょっとした発見や雑談をどうぞ。"),
    ]
    .into_iter()
    .map(|(id, name, topic)| Channel {
        id,
        name: name.into(),
        topic: topic.into(),
        kind: "channel".into(),
        peer_id: None,
    })
    .collect();
    channels.extend([(8, 2), (9, 3)].map(|(id, peer)| Channel {
        id,
        kind: "dm".into(),
        peer_id: Some(peer),
        ..Default::default()
    }));
    State {
        workspace: Workspace {
            id: 0,
            name: "Fieldwork Studio".into(),
            owner_id: 1,
        },
        me: None,
        members,
        channels,
    }
}

fn post(id: i64, channel_id: i64, author_id: i64, at: (i32, i32), body: &str) -> Message {
    Message {
        id,
        channel_id,
        author_id,
        body: body.into(),
        created_at: Date::new_with_year_month_day_hr_min(2026, 5, 16, at.0, at.1).get_time() as i64,
        ..Default::default()
    }
}

fn reaction(emoji: &str, count: i64) -> Reaction {
    Reaction {
        emoji: emoji.into(),
        count,
        mine: false,
    }
}

pub fn messages() -> Vec<Message> {
    vec![
        Message {
            reactions: vec![reaction("👍", 3), reaction("🎉", 2)],
            ..post(
                11,
                1,
                1,
                (9, 5),
                "おはようございます。次のフィールドノートは「森と水辺」がテーマです。今週は表紙と取材先を決めていきましょう。",
            )
        },
        Message {
            reply_count: 2,
            reactions: vec![reaction("❤️", 3)],
            ..post(
                12,
                1,
                2,
                (9, 18),
                "表紙の配色を考えました。川沿いを歩いたときの、霧が晴れていく感じをイメージしています。\n> 深い森 #294F43　 朝霧 #A8B7A0　 朝日 #E4A16F",
            )
        },
        post(
            13,
            1,
            3,
            (9, 24),
            "やわらかい色でいいですね。扉のページにも同じ緑を使いたいです。",
        ),
        Message {
            reactions: vec![reaction("👍", 2)],
            ..post(
                14,
                1,
                4,
                (9, 41),
                "川沿いの下見は木曜に行ってきます。撮ってきてほしい風景があれば、#field-notes に書いてください！",
            )
        },
        Message {
            parent_id: Some(12),
            ..post(15, 1, 1, (9, 22), "中央の淡い緑、朝の光に近くて好きです。")
        },
        Message {
            parent_id: Some(12),
            ..post(
                16,
                1,
                2,
                (9, 32),
                "ありがとう。紙にのせたときの見え方も確かめてみます。",
            )
        },
        post(
            81,
            8,
            2,
            (9, 28),
            "表紙の紙見本、打ち合わせに持っていきますね。",
        ),
        post(
            91,
            9,
            3,
            (10, 14),
            "校正用の原稿がまとまったら声をかけます。",
        ),
        Message {
            reactions: vec![reaction("👀", 2)],
            ..post(
                21,
                2,
                2,
                (10, 12),
                "余白を少し広げた表紙のラフができました。小さな判型でもタイトルが読みやすくなりそうです。",
            )
        },
        post(
            31,
            3,
            4,
            (10, 28),
            "今回の下見では、橋の上と水辺の遊歩道から同じ稜線を撮ってみます。",
        ),
        post(
            41,
            4,
            3,
            (11, 3),
            "帰り道の喫茶店で、新しい豆の試飲が始まっていました。ひと息つきたい方におすすめです。",
        ),
    ]
}
