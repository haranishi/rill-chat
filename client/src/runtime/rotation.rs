// Tabs on the same origin share their cookie. Coordinate just the short gap
// between server-side revocation and the replacement Set-Cookie reaching them.
use super::*;
use js_sys::Date;
use wasm_bindgen::JsValue;
use web_sys::{BroadcastChannel, MessageEvent, StorageEvent};

const CHANNEL: &str = "rill.password-rotation";
const MARKER: &str = "rill.rotation.";
const LIFETIME_MS: i32 = 30_000;

thread_local! {
    static BUS: RefCell<Option<BroadcastChannel>> = const { RefCell::new(None) };
    static PENDING: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
}

pub struct Rotation(String);

fn valid_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|v| v.is_ascii_hexdigit())
}

fn local_storage() -> Option<Storage> {
    dom::window().local_storage().ok().flatten()
}

fn track(id: &str, expires: f64) {
    let remaining = expires - Date::now();
    if !valid_id(id) || !remaining.is_finite() || remaining <= 0.0 || sample() {
        return;
    }
    if !PENDING.with_borrow_mut(|pending| pending.insert(id.to_owned())) {
        return;
    }
    let suspended = change(|app| std::mem::replace(&mut app.password_pending, true));
    if !suspended {
        events::stop_stream();
    }
    let id = id.to_owned();
    let delay = remaining.ceil().min(f64::from(LIFETIME_MS)) as i32;
    dom::timeout(delay, move || {
        if PENDING.with_borrow(|pending| pending.contains(&id)) {
            release(&id);
            if let Some(storage) = local_storage() {
                let _ = storage.remove_item(&format!("{MARKER}{id}"));
            }
        }
    });
}

fn release(id: &str) {
    let resume = PENDING.with_borrow_mut(|pending| pending.remove(id) && pending.is_empty());
    if resume {
        let session = change(|app| {
            app.password_pending = false;
            (!app.sample).then_some(app.auth_epoch)
        });
        if let Some(epoch) = session {
            events::connect(epoch);
        }
    }
}

fn broadcast(value: &str) {
    BUS.with_borrow(|bus| {
        if let Some(channel) = bus {
            let _ = channel.post_message(&JsValue::from_str(value));
        }
    });
}

pub(super) fn start() -> Rotation {
    let id = composer::random_id().unwrap_or_else(|| format!("{:032x}", Date::now() as u64));
    let expires = Date::now() + f64::from(LIFETIME_MS);
    track(&id, expires);
    if let Some(storage) = local_storage() {
        let _ = storage.set_item(&format!("{MARKER}{id}"), &expires.to_string());
    }
    broadcast(&format!("begin/{id}/{expires}"));
    Rotation(id)
}

pub(super) fn finish(rotation: Rotation) {
    let id = rotation.0;
    if let Some(storage) = local_storage() {
        let _ = storage.remove_item(&format!("{MARKER}{id}"));
    }
    broadcast(&format!("end/{id}"));
    release(&id);
}

// A backgrounded tab may see the stream close before its queued storage event.
pub(super) fn recover_pending() -> bool {
    if let Some(storage) = local_storage() {
        let mut expired = Vec::new();
        for index in 0..storage.length().unwrap_or_default() {
            let Some(key) = storage.key(index).ok().flatten() else {
                continue;
            };
            let Some(id) = key.strip_prefix(MARKER).filter(|id| valid_id(id)) else {
                continue;
            };
            let expires = storage
                .get_item(&key)
                .ok()
                .flatten()
                .and_then(|v| v.parse::<f64>().ok());
            if let Some(expires) = expires
                && expires > Date::now()
            {
                track(id, expires);
            } else {
                expired.push(key);
            }
        }
        for key in expired {
            let _ = storage.remove_item(&key);
        }
    }
    read(|app| app.password_pending)
}

pub(super) fn reset() {
    PENDING.with_borrow_mut(HashSet::clear);
}

pub(super) fn install() {
    if let Ok(bus) = BroadcastChannel::new(CHANNEL) {
        dom::on(bus.as_ref(), "message", |event| {
            let Some(value) = event
                .dyn_into::<MessageEvent>()
                .ok()
                .and_then(|e| e.data().as_string())
            else {
                return;
            };
            let mut parts = value.split('/');
            match (parts.next(), parts.next(), parts.next()) {
                (Some("begin"), Some(id), Some(end)) if valid_id(id) => {
                    if let Ok(expires) = end.parse() {
                        track(id, expires);
                    }
                }
                (Some("end"), Some(id), None) if valid_id(id) => release(id),
                _ => {}
            }
        });
        BUS.with_borrow_mut(|slot| *slot = Some(bus));
    }
    dom::on(dom::window().as_ref(), "storage", |event| {
        let Ok(event) = event.dyn_into::<StorageEvent>() else {
            return;
        };
        let Some(key) = event.key() else { return };
        let Some(id) = key.strip_prefix(MARKER).filter(|id| valid_id(id)) else {
            return;
        };
        match event.new_value() {
            Some(value) => {
                if let Ok(expires) = value.parse() {
                    track(id, expires);
                }
            }
            None => release(id),
        }
    });
}
