use wasm_bindgen::prelude::wasm_bindgen;
use web_sys::Window;

pub const STORAGE_KEY: &str = "rill.appearance";
pub const MEDIA_QUERY: &str = "(prefers-color-scheme: dark)";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preference {
    Light,
    Dim,
    Device,
}

impl Preference {
    pub fn parse(value: Option<&str>) -> Self {
        match value {
            Some("dim") => Self::Dim,
            Some("device") => Self::Device,
            _ => Self::Light,
        }
    }

    pub fn value(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dim => "dim",
            Self::Device => "device",
        }
    }

    pub fn stored(window: &Window) -> Self {
        let saved = window
            .local_storage()
            .ok()
            .flatten()
            .and_then(|store| store.get_item(STORAGE_KEY).ok().flatten());
        Self::parse(saved.as_deref())
    }

    pub fn save(self, window: &Window) {
        if let Ok(Some(store)) = window.local_storage() {
            let _ = store.set_item(STORAGE_KEY, self.value());
        }
    }

    fn dim(self, window: &Window) -> bool {
        self == Self::Dim
            || (self == Self::Device
                && window
                    .match_media(MEDIA_QUERY)
                    .ok()
                    .flatten()
                    .is_some_and(|media| media.matches()))
    }

    pub fn apply(self, window: &Window) {
        let Some(document) = window.document() else {
            return;
        };
        let (theme, color) = if self.dim(window) {
            ("dim", "#27292f")
        } else {
            ("light", "#fbfbfc")
        };
        if let Some(root) = document.document_element() {
            let _ = root.set_attribute("data-theme", theme);
        }
        if let Ok(Some(meta)) = document.query_selector("meta[name=theme-color]") {
            let _ = meta.set_attribute("content", color);
        }
    }
}

/// Paint the saved palette while the chat client is still loading.
#[wasm_bindgen]
pub fn prepaint() {
    if let Some(window) = web_sys::window() {
        Preference::stored(&window).apply(&window);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_existing_choices_and_defaults_to_light() {
        for choice in [Preference::Light, Preference::Dim, Preference::Device] {
            assert_eq!(Preference::parse(Some(choice.value())), choice);
        }
        assert_eq!(Preference::parse(None), Preference::Light);
        assert_eq!(Preference::parse(Some("unknown")), Preference::Light);
    }
}
