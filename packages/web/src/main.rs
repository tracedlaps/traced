use dioxus::prelude::*;

mod demo;

const ICON: Asset = asset!("/assets/icon.svg");
const FONTS: &str = "https://fonts.googleapis.com/css2?family=Archivo:wdth,wght@62..125,400..900&family=IBM+Plex+Mono:wght@400;500&display=swap";
const ISSUES: &str = "https://github.com/tracedlaps/tracedlaps/issues";
const SUPPORT: &str = "mailto:support@tracedlaps.com";
const APK: &str = "https://github.com/tracedlaps/tracedlaps/releases/latest/download/traced.apk";

const FEATURES: [(&str, &str, &str); 6] = [
    ("accent", "Live delta", "Your gap to the all-time best lap, updated as you drive, green when you're ahead and red when you're not."),
    ("s0", "Sector times", "Split any track into sectors. The map lights up the one you're in, with a running sector clock beside it."),
    ("s2", "Circuits and runs", "Closed loops are timed in laps; one-way tracks like a mountain descent are timed start to finish."),
    ("s1", "Draw or record tracks", "Tap waypoints on a street or satellite map, drive the route once to record it, or import a GPX, KML or CSV file."),
    ("muted", "Speedometer", "No track needed: current, top and average speed plus distance, in km/h."),
    ("fg", "History and splits", "Every recording is kept with its laps, splits and personal bests, even when the app closes mid-session."),
];

const STEPS: [(&str, &str); 4] = [
    ("Download the APK on your phone", "Open this page on the phone itself and tap the button."),
    ("Allow the install", "Android prompts to let your browser install apps. Allow it for this one install."),
    ("Open Traced and pick a track", "Draw one in the Editor tab, or use the Speedometer to try it straight away."),
    ("Allow location on Start", "Traced asks for precise location the first time you record. It's only used while recording."),
];

#[derive(Routable, Clone, PartialEq)]
enum Route {
    #[layout(Layout)]
        #[route("/")]
        Home {},
        #[route("/changelog")]
        Changelog {},
        #[route("/privacy")]
        Privacy {},
}

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    rsx! {
        document::Link { rel: "icon", href: ICON }
        document::Link { rel: "stylesheet", href: FONTS }
        // Release inlines it: a linked sheet loads after first paint and flashes unstyled SVGs.
        // Debug links it so `dx serve` hot-reloads CSS edits.
        if cfg!(debug_assertions) {
            document::Stylesheet { href: asset!("/assets/main.css") }
        } else {
            style { {include_str!("../assets/main.css")} }
        }

        Router::<Route> {}
    }
}

#[component]
fn Layout() -> Element {
    rsx! {
        div { class: "wrap",
            header {
                Link { class: "brand", to: Route::Home {}, Logo {} "TRACED" }
                nav {
                    a { href: "/#features", "Features" }
                    a { href: "/#install", "Install" }
                    Link { to: Route::Changelog {}, "Changelog" }
                }
            }
            Outlet::<Route> {}
            footer {
                span { "Traced · made with Rust and Dioxus" }
                span { class: "links",
                    Link { to: Route::Changelog {}, "Changelog" }
                    Link { to: Route::Privacy {}, "Privacy" }
                    a { href: SUPPORT, "Support" }
                }
            }
        }
    }
}

#[component]
fn Home() -> Element {
    // Arriving from another page at /#features: the browser looked for the anchor before it rendered.
    use_effect(|| _ = document::eval("location.hash && document.querySelector(location.hash)?.scrollIntoView()"));
    rsx! {
        document::Title { "Traced · GPS lap timer for Android" }
        div { class: "hero", id: "top",
            div {
                p { class: "label", "GPS lap timer for Android · Open beta" }
                h1 { "Race your " em { "best lap." } " Every lap." }
                p { class: "lede",
                    "Draw a circuit or a downhill run on the map, set your sectors, and go. Traced times every lap from your phone's GPS and shows "
                    b { "live how far ahead or behind your best" }
                    " you are."
                }
                div { class: "get",
                    Download {}
                    dl { class: "meta",
                        dt { "Status" } dd { "Open beta, free to test" }
                        dt { "Later" } dd { "One-time Pro unlock" }
                        dt { "Languages" } dd { "English, Português" }
                    }
                }
            }
            div { class: "device",
                demo::Phone {}
                p { class: "caption label", "The Live screen, lapping Mondercange" }
            }
        }

        section { id: "features",
            p { class: "label", "What it does" }
            h2 { "Built for the stopwatch in your head" }
            p { class: "intro", "Everything you'd want from a timing transponder, on the phone already in your pocket." }
            dl { class: "specs",
                for (color, title, text) in FEATURES {
                    div { class: "spec",
                        dt { i { background: "var(--{color})" } "{title}" }
                        dd { "{text}" }
                    }
                }
            }
        }

        section { id: "install",
            div { class: "install",
                div {
                    p { class: "label", "Installing" }
                    h2 { "Not on the Play Store, yet" }
                    p { class: "intro",
                        "Traced ships as an APK. Android asks once before installing an app from outside the store. It's in beta, so expect rough edges: found a bug or missing something? "
                        a { href: ISSUES, "Open an issue" } " or "
                        a { href: SUPPORT, "email us" } "."
                    }
                    Download {}
                }
                ol { class: "steps",
                    for (title, text) in STEPS {
                        li { div { b { "{title}" } span { "{text}" } } }
                    }
                }
            }
        }

        section {
            div { class: "local",
                div { class: "big", "Offline." br {} "Yours." }
                div {
                    p { "Tracks and recordings live only on your phone. No account, no cloud, no tracking." }
                    p { "Export a backup file whenever you want a copy, and restore it on a new phone." }
                }
            }
        }
    }
}

#[component]
fn Changelog() -> Element {
    rsx! {
        document::Title { "Changelog · Traced" }
        section {
            p { class: "label", "Changelog" }
            h2 { "What's new" }
            for entry in changelog::parse(changelog::EN) {
                div { class: "release",
                    h3 { "v{entry.version}" small { "{entry.date}" } }
                    ul {
                        for change in entry.changes {
                            li { "{change}" }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn Privacy() -> Element {
    rsx! {
        document::Title { "Privacy policy · Traced" }
        section { class: "policy",
            p { class: "label", "Privacy policy" }
            h2 { "Your laps stay on your phone" }
            p { class: "intro", "Effective 7 October 2026. Traced has no account, no analytics, no ads and no server of its own. This page covers the Traced app and tracedlaps.com." }

            h3 { "Who we are" }
            p {
                "Traced is made by Pedro Soares, who is responsible for the data described here. Questions or requests: "
                a { href: "mailto:privacy@tracedlaps.com", "privacy@tracedlaps.com" } "."
            }

            h3 { "What the app uses on your phone" }
            ul {
                li { b { "Precise location" } ", to measure speed and time laps and sectors, and to show where you are on the map. It's used while you record, including with the screen off (Android shows a notification while it does), and when you ask the map to find you." }
                li { b { "Motion sensors" } ", for the G-meter." }
                li { b { "Notifications" } ", to show that a recording is running." }
                li { b { "Files you choose" } ", when you import a track, export or restore a backup, or share a track with another app." }
            }
            p { "Your tracks, recordings, laps and settings are stored only in Traced's private storage on your phone. Uninstalling Traced deletes them. Backups you export are files you keep and control. If you've turned on Android's own backup, Android may include Traced's data in your Google account's backup; Traced can't see it." }

            h3 { "What leaves your phone" }
            p { "Traced never sends your recordings, lap times or location history anywhere. A few features fetch data from other services, which then see your IP address and the request itself:" }
            ul {
                li { b { "Map images" } " from Esri (satellite) and OpenStreetMap (streets). They see which area of the map you're looking at." }
                li { b { "Place search and street names" } " from OpenStreetMap's Nominatim. It receives what you search for, or the coordinates of the point a street name is looked up for." }
                li { b { "Update checks" } ", only in the version downloaded from this website: the list of releases from GitHub and what's new from tracedlaps.com. The Google Play version leaves updates to Google Play." }
            }
            p {
                "Their privacy policies: "
                a { href: "https://www.esri.com/en-us/privacy/overview", "Esri" } ", "
                a { href: "https://osmfoundation.org/wiki/Privacy_Policy", "OpenStreetMap" } ", "
                a { href: "https://docs.github.com/en/site-policy/privacy-policies/github-general-privacy-statement", "GitHub" } "."
            }
            p { "If you install Traced from Google Play, Google may collect crash reports and usage statistics under your device's settings and Google's privacy policy. We see only the reports Google aggregates from them." }

            h3 { "The website" }
            p {
                "tracedlaps.com uses no cookies and no analytics. It's hosted by GitHub Pages, which logs visitors' IP addresses for security, and loads its fonts from Google Fonts, which sees your IP address. See "
                a { href: "https://policies.google.com/privacy", "Google's privacy policy" } "."
            }

            h3 { "Your rights" }
            p { "We hold no personal data about you: everything Traced keeps is on your phone, where you can delete it. You still have the right to ask what we hold, and to complain to your data protection authority. Write to privacy@tracedlaps.com." }

            h3 { "Children" }
            p { "Traced isn't aimed at children under 13." }

            h3 { "Changes" }
            p { "If this policy changes, the new version will be posted here with a new effective date." }
        }
    }
}

#[component]
fn Download() -> Element {
    rsx! {
        a { class: "apk", href: APK,
            svg { view_box: "0 0 24 24", path { d: "M12 3v12M7 10l5 5 5-5M5 20h14" } }
            span { "Download the APK" small { "v0.8.2 beta · Android 8.0+" } }
        }
    }
}

#[component]
fn Logo() -> Element {
    rsx! {
        img { src: ICON, alt: "" }
    }
}
