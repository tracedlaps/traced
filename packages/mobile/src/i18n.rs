//! Translations: one Fluent file per language in `i18n/`, the device's unless another is chosen.

use chrono::{DateTime, Local, Utc};
use dioxus_i18n::prelude::*;
use unic_langid::{langid, LanguageIdentifier};

use crate::prefs::{Choice, Language};

pub fn device_language() -> LanguageIdentifier {
    sys_locale::get_locale().and_then(|l| l.parse().ok()).unwrap_or(langid!("en"))
}

/// The chosen language, falling back to English for any language or key not translated.
pub fn use_translations() {
    use_init_i18n(|| {
        I18nConfig::new(Language::load().id())
            .with_fallback(langid!("en"))
            .with_locale((langid!("en"), include_str!("../i18n/en.ftl")))
            .with_locale((langid!("pt"), include_str!("../i18n/pt.ftl")))
    });
}

/// `at` in local time, month and day names in the chosen language.
pub fn local_date(at: DateTime<Utc>, fmt: &str) -> String {
    let locale = chrono::Locale::try_from(&*i18n().language().to_string().replace('-', "_"));
    at.with_timezone(&Local)
        .format_localized(fmt, locale.unwrap_or(chrono::Locale::en_US))
        .to_string()
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeSet, fs};

    use dioxus_i18n::fluent::FluentResource;

    /// Message ids: top-level `id = ...` lines.
    fn keys(ftl: &str) -> BTreeSet<String> {
        FluentResource::try_new(ftl.into()).expect("parses");
        ftl.lines()
            .filter(|l| l.starts_with(|c: char| c.is_ascii_lowercase()))
            .filter_map(|l| l.split_once(" =").map(|(k, _)| k.to_string()))
            .collect()
    }

    #[test]
    fn every_key_is_in_english() {
        let en = keys(include_str!("../i18n/en.ftl"));
        let pt = keys(include_str!("../i18n/pt.ftl"));
        assert!(pt.is_subset(&en), "not in en.ftl: {:?}", pt.difference(&en));

        let mut used = BTreeSet::new();
        for file in fs::read_dir("src/views").unwrap() {
            let src = fs::read_to_string(file.unwrap().path()).unwrap();
            // `t!("`, but not `format!("`.
            let calls = src.match_indices("t!(\"").filter(|&(i, _)| !src[..i].ends_with(char::is_alphanumeric));
            used.extend(calls.filter_map(|(i, _)| src[i + 4..].split_once('"')).map(|(k, _)| k.to_string()));
        }
        assert!(used.is_subset(&en), "used but not in en.ftl: {:?}", used.difference(&en));
    }
}
