//! What's new in each release, one markdown file per language (`en.md`, `pt.md`): a `## <version> - <date>`
//! heading per release, newest first, then a `- ` line per change. The website shows the English one and
//! publishes them all to tracedlaps.com/changelog/, where the app fetches its language's before an update.

pub const EN: &str = include_str!("../en.md");

pub struct Entry<'a> {
    pub version: &'a str,
    pub date: &'a str,
    pub changes: Vec<&'a str>,
}

/// A changelog's releases, newest first.
pub fn parse(md: &str) -> Vec<Entry<'_>> {
    let mut entries: Vec<Entry> = Vec::new();
    for line in md.lines().map(str::trim) {
        if let Some(head) = line.strip_prefix("## ") {
            let (version, date) = head.split_once(" - ").unwrap_or((head, ""));
            entries.push(Entry { version, date, changes: Vec::new() });
        } else if let (Some(change), Some(entry)) = (line.strip_prefix("- "), entries.last_mut()) {
            entry.changes.push(change);
        }
    }
    entries
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn languages_list_the_same_releases() {
        let releases = |md| parse(md).iter().map(|e| (e.version, e.date, e.changes.len())).collect::<Vec<_>>();
        assert_eq!(releases(EN), releases(include_str!("../pt.md")));
        assert!(parse(EN).iter().all(|e| !e.changes.is_empty()));
    }
}
