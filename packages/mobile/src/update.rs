//! The app's own updates: the newer releases on the public repo, with what's new in each from the
//! site's changelog, installed by the system installer.

use dioxus::prelude::*;
use serde::Deserialize;

#[cfg(target_os = "android")]
use crate::storage::plugin;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
const RELEASES: &str = "https://api.github.com/repos/tracedlaps/tracedlaps/releases?per_page=30";
const CHANGELOG: &str = "https://tracedlaps.com/changelog";

#[derive(Debug, Clone, PartialEq)]
pub struct Release {
    pub version: String,
    /// What's new, from the changelog; empty if it couldn't be fetched.
    pub changes: Vec<String>,
    url: String,
}

/// `check`, run on launch and again if the language changes.
pub type Latest = Resource<Result<Vec<Release>, String>>;

/// Why a check or install failed, for the user.
pub fn describe(error: String) -> String {
    #[cfg(target_os = "android")]
    if error == plugin::OFFLINE {
        return dioxus_i18n::t!("no-internet");
    }
    error
}

/// The releases newer than this build, newest first, with their changes in `language` (or English).
pub async fn check(language: String) -> Result<Vec<Release>, String> {
    if cfg!(feature = "play") {
        return Ok(vec![]);
    }
    #[derive(Deserialize)]
    struct GitHubRelease {
        tag_name: String,
        prerelease: bool,
        assets: Vec<Asset>,
    }
    #[derive(Deserialize)]
    struct Asset {
        name: String,
        browser_download_url: String,
    }
    let releases: Vec<GitHubRelease> = serde_json::from_str(&fetch(RELEASES.into()).await?).map_err(|e| e.to_string())?;
    // Only a nicety: the update is offered without it.
    let changelog = match fetch(format!("{CHANGELOG}/{language}.md")).await {
        Ok(md) => md,
        Err(_) => fetch(format!("{CHANGELOG}/en.md")).await.unwrap_or_default(),
    };
    let entries = changelog::parse(&changelog);
    let mut newer: Vec<Release> = releases
        .into_iter()
        .filter(|r| !r.prerelease)
        .filter_map(|r| {
            let version = r.tag_name.trim_start_matches('v').to_string();
            let url = r.assets.into_iter().find(|a| a.name.ends_with(".apk"))?.browser_download_url;
            let changes = entries.iter().find(|e| e.version == version).map_or(vec![], |e| e.changes.iter().map(|c| c.to_string()).collect());
            (parse(&version) > parse(VERSION)).then_some(Release { version, changes, url })
        })
        .collect();
    newer.sort_by_key(|r| std::cmp::Reverse(parse(&r.version)));
    Ok(newer)
}

/// Downloads `release` and opens the system installer on it; returns once it's up.
pub async fn install(release: Release) -> Result<(), String> {
    #[cfg(target_os = "android")]
    return plugin::call(move |p| plugin::install_update(p, &release.url)).await;

    #[cfg(not(target_os = "android"))]
    {
        let _ = release;
        Err("updates are only on Android".into())
    }
}

async fn fetch(url: String) -> Result<String, String> {
    #[cfg(target_os = "android")]
    return plugin::call(move |p| plugin::fetch(p, &url)).await;

    #[cfg(not(target_os = "android"))]
    {
        let _ = url;
        Err("updates are only on Android".into())
    }
}

/// `1.2.3` as numbers, to compare; a part that isn't one counts as 0.
fn parse(version: &str) -> Vec<u32> {
    version.split('.').map(|n| n.parse().unwrap_or(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn versions_compare_as_numbers() {
        assert!(parse("0.10.0") > parse("0.9.1"));
        assert!(parse("1.0") > parse("0.99.99"));
        assert!(parse("0.4.0") == parse("0.4.0"));
    }
}
