use crate::dom;
use rill_appearance::{MEDIA_QUERY, Preference, STORAGE_KEY};
use std::cell::Cell;
use wasm_bindgen::JsCast;
use web_sys::{HtmlSelectElement, StorageEvent};

thread_local! { static CHOICE: Cell<Preference> = const { Cell::new(Preference::Light) }; }

fn apply(choice: Preference) {
    CHOICE.set(choice);
    choice.apply(&dom::window());
    if let Some(control) =
        dom::get("appearance").and_then(|el| el.dyn_into::<HtmlSelectElement>().ok())
    {
        control.set_value(choice.value());
    }
}

pub fn before_paint() {
    let win = dom::window();
    // The user may have changed their OS setting or another tab while this client downloaded.
    apply(Preference::stored(&win));
    if let Ok(Some(media)) = win.match_media(MEDIA_QUERY) {
        dom::on(media.as_ref(), "change", |_| {
            if CHOICE.get() == Preference::Device {
                apply(Preference::Device);
            }
        });
    }
    dom::on(win.as_ref(), "storage", |event| {
        let Some(event) = event.dyn_ref::<StorageEvent>() else {
            return;
        };
        if event.key().as_deref().is_none_or(|key| key == STORAGE_KEY) {
            apply(Preference::parse(event.new_value().as_deref()));
        }
    });
}

pub fn mount() {
    if let Some(select) =
        dom::get("appearance").and_then(|el| el.dyn_into::<HtmlSelectElement>().ok())
    {
        select.set_value(CHOICE.get().value());
        dom::on(select.as_ref(), "change", |event| {
            let Some(select) = event
                .target()
                .and_then(|el| el.dyn_into::<HtmlSelectElement>().ok())
            else {
                return;
            };
            let choice = Preference::parse(Some(&select.value()));
            apply(choice);
            choice.save(&dom::window());
        });
    }
    if let Some(root) = dom::document().document_element() {
        let _ = root.remove_attribute("data-loading");
    }
}
