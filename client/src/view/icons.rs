pub fn icon(name: &str) -> String {
    let path = match name {
        "plus" => "M12 5v14M5 12h14",
        "x" => "m6 6 12 12M18 6 6 18",
        "search" => "M20 20l-5-5M17 10.5a6.5 6.5 0 1 1-13 0 6.5 6.5 0 0 1 13 0",
        "menu" => "M3 6h18M3 12h18M3 18h18",
        "panel" => "M4 4h16v16H4zM14 4v16",
        "layout" => "M3 4.5h18v15H3zM8.5 4.5v15",
        "layers" => "m3 8 9-5 9 5-9 5zM3.5 12.5l8.5 5 8.5-5M3.5 17l8.5 5 8.5-5",
        "info" => "M22 12a10 10 0 1 1-20 0 10 10 0 0 1 20 0M12 11v6M12 7v.1",
        "user-plus" => {
            "M16 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2M14 7a4 4 0 1 1-8 0 4 4 0 0 1 8 0M19 7v6M16 10h6"
        }
        "user" => "M20 21v-2a5 5 0 0 0-5-5H9a5 5 0 0 0-5 5v2M16 7a4 4 0 1 1-8 0 4 4 0 0 1 8 0",
        "users" => {
            "M16 20v-1a5 5 0 0 0-5-5H9a5 5 0 0 0-5 5v1M14 7a4 4 0 1 1-8 0 4 4 0 0 1 8 0M17 4a3.7 3.7 0 0 1 0 7M18 14c2 .6 3 2 3 4v2"
        }
        "chevrons" => "m8 9 4-4 4 4M8 15l4 4 4-4",
        "down" => "m6 9 6 6 6-6",
        "right" => "m9 5 7 7-7 7",
        "arrow" => "M4 12h15m-6-6 6 6-6 6",
        "thread" => "M4 4.5h15v11H9l-5 4zM8 9h7",
        "smile" => "M21 12a9 9 0 1 1-18 0 9 9 0 1 1 18 0M8 14c2 3 6 3 8 0M9 9h.1M15 9h.1",
        "send" => "m3 4 18 8-18 8 3-7 8-1-8-1z",
        "lock" => "M5 10h14v11H5zM8 10V7a4 4 0 0 1 8 0v3",
        "copy" => "M8 8h12v13H8zM5 16H3V3h12v2",
        "check" => "m4 12 5 5L20 6",
        "exit" => "M10 3H4v18h6M10 12h11m-5-5 5 5-5 5",
        "external" => "M13 4h7v7M20 4l-10 10M10 5H4v15h15v-6",
        "book" => "M3 5c4-1 6 0 9 2 3-2 5-3 9-2v14c-4-1-6 0-9 2-3-2-5-3-9-2zM12 7v14",
        "compass" => "M22 12a10 10 0 1 1-20 0 10 10 0 1 1 20 0M15.5 8.5 13 13l-4.5 2.5L11 11z",
        "link" => {
            "m10 14 4-4m-1-3 2-2a4 4 0 0 1 6 6l-4 4a4 4 0 0 1-5 .4M11 17l-2 2a4 4 0 1 1-6-6l4-4a4 4 0 0 1 5-.4"
        }
        "inbox" => "M4 5h16l2 13H2zM3 13h5l2 3h4l2-3h5",
        "key" => "M15 3a6 6 0 1 1-5.7 8L3 17v4h4v-3h3v-3l2-2M17 7h.1",
        "globe" => "M21 12a9 9 0 1 1-18 0 9 9 0 1 1 18 0M3 12h18M12 3c4 4 4 14 0 18-4-4-4-14 0-18",
        "bookmark" => "M6 4h12v17l-6-4-6 4z",
        "edit" => "m14 6 4 4M4 20l4-1 12-12a2.8 2.8 0 0 0-4-4L4 15z",
        _ => "M10 3 7 21M17 3l-3 18M4 9h16M3 15h16",
    };
    format!(
        r#"<svg class="icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.65" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" focusable="false"><path d="{path}"/></svg>"#
    )
}
