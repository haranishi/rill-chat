use wasm_bindgen::{JsCast, closure::Closure};
use web_sys::{Document, Element, Event, EventTarget, HtmlElement, Window};

pub fn window() -> Window {
    web_sys::window().expect("Rill runs in a browser window")
}

pub fn document() -> Document {
    window().document().expect("browser document")
}

pub fn get(id: &str) -> Option<Element> {
    document().get_element_by_id(id)
}

pub fn set_html(id: &str, html: &str) {
    if let Some(el) = get(id) {
        el.set_inner_html(html);
    }
}

pub fn set_text(id: &str, text: &str) {
    if let Some(el) = get(id) {
        el.set_text_content(Some(text));
    }
}

pub fn hidden(id: &str, hide: bool) {
    if let Some(el) = get(id).and_then(|v| v.dyn_into::<HtmlElement>().ok()) {
        el.set_hidden(hide);
    }
}

pub fn focus(id: &str) {
    if let Some(el) = get(id).and_then(|v| v.dyn_into::<HtmlElement>().ok()) {
        let _ = el.focus();
    }
}

pub struct Listener {
    target: EventTarget,
    kind: String,
    callback: Closure<dyn FnMut(Event)>,
}

pub fn listen(target: &EventTarget, kind: &str, f: impl FnMut(Event) + 'static) -> Listener {
    let callback = Closure::wrap(Box::new(f) as Box<dyn FnMut(Event)>);
    target
        .add_event_listener_with_callback(kind, callback.as_ref().unchecked_ref())
        .expect("register DOM listener");
    Listener {
        target: target.clone(),
        kind: kind.to_owned(),
        callback,
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        let _ = self.target.remove_event_listener_with_callback(
            &self.kind,
            self.callback.as_ref().unchecked_ref(),
        );
    }
}

/// For fixed page elements that live as long as the Wasm instance.
pub fn on(target: &EventTarget, kind: &str, f: impl FnMut(Event) + 'static) {
    std::mem::forget(listen(target, kind, f));
}

pub fn timeout(ms: i32, f: impl FnOnce() + 'static) {
    let cb = Closure::once_into_js(f);
    let _ = window().set_timeout_with_callback_and_timeout_and_arguments_0(cb.unchecked_ref(), ms);
}
