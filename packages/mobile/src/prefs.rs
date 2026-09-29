//! User preferences: each a choice among fixed options, saved in the store (so it goes with a backup).

use dioxus::prelude::*;
use unic_langid::{langid, LanguageIdentifier};

use crate::{i18n, storage, units::Units};

/// A preference saved under `KEY`. Each option's value is also its label's key, `{KEY}-{value}`.
pub trait Choice: Copy + PartialEq + 'static {
    const KEY: &'static str;
    /// Every option and its stored value, the default first.
    const OPTIONS: &'static [(Self, &'static str)];

    fn load() -> Self {
        let saved = storage::setting(Self::KEY);
        Self::OPTIONS.iter().find(|(_, v)| saved.as_deref() == Some(v)).unwrap_or(&Self::OPTIONS[0]).0
    }

    fn value(self) -> &'static str {
        Self::OPTIONS.iter().find(|(c, _)| *c == self).map_or("", |(_, v)| v)
    }

    fn save(self) {
        storage::put_setting(Self::KEY, self.value());
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Theme {
    System,
    Light,
    Dark,
}

impl Choice for Theme {
    const KEY: &'static str = "theme";
    const OPTIONS: &'static [(Self, &'static str)] = &[(Self::System, "system"), (Self::Light, "light"), (Self::Dark, "dark")];
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Language {
    System,
    English,
    Portuguese,
}

impl Choice for Language {
    const KEY: &'static str = "language";
    const OPTIONS: &'static [(Self, &'static str)] = &[(Self::System, "system"), (Self::English, "en"), (Self::Portuguese, "pt")];
}

impl Language {
    pub fn id(self) -> LanguageIdentifier {
        match self {
            Self::System => i18n::device_language(),
            Self::English => langid!("en"),
            Self::Portuguese => langid!("pt"),
        }
    }
}

/// The live map: following the dot, or the whole track.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MapView {
    Follow,
    Overview,
}

impl Choice for MapView {
    const KEY: &'static str = "live-map";
    const OPTIONS: &'static [(Self, &'static str)] = &[(Self::Follow, "follow"), (Self::Overview, "overview")];
}

/// Provides every preference to the app as a signal, and applies theme and language as they change.
pub fn use_prefs() {
    use_context_provider(|| Signal::new(Units::load()));
    use_context_provider(|| Signal::new(MapView::load()));
    let theme = use_context_provider(|| Signal::new(Theme::load()));
    let language = use_context_provider(|| Signal::new(Language::load()));
    // `main.css` reads it; "system" leaves it to `prefers-color-scheme`.
    use_effect(move || _ = document::eval(&format!("document.documentElement.dataset.theme = '{}'", theme().value())));
    use_effect(move || dioxus_i18n::prelude::i18n().set_language(language().id()));
}

/// Rereads every preference, after a restore replaced them.
pub fn reload() {
    consume_context::<Signal<Units>>().set(Units::load());
    consume_context::<Signal<MapView>>().set(MapView::load());
    consume_context::<Signal<Theme>>().set(Theme::load());
    consume_context::<Signal<Language>>().set(Language::load());
}
