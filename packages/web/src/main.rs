use dioxus::prelude::*;

mod demo;

const ICON: Asset = asset!("/assets/icon.svg");
const MAIN_CSS: Asset = asset!("/assets/main.css");
const FONTS: &str = "https://fonts.googleapis.com/css2?family=Archivo:wdth,wght@62..125,400..900&family=IBM+Plex+Mono:wght@400;500&display=swap";
const ISSUES: &str = "https://github.com/tracedlaps/tracedlaps/issues";
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

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    rsx! {
        document::Title { "Traced · GPS lap timer for Android" }
        document::Link { rel: "icon", href: ICON }
        document::Link { rel: "stylesheet", href: FONTS }
        document::Stylesheet { href: MAIN_CSS }

        div { class: "wrap",
            header {
                a { class: "brand", href: "#top", Logo {} "TRACED" }
                nav {
                    a { href: "#features", "Features" }
                    a { href: "#install", "Install" }
                    a { href: "#changelog", "Changelog" }
                }
            }

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
                            a { href: ISSUES, "Open an issue" } "."
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

            section { id: "changelog",
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

            section {
                div { class: "local",
                    div { class: "big", "Offline." br {} "Yours." }
                    div {
                        p { "Tracks and recordings live only on your phone. No account, no cloud, no tracking." }
                        p { "Export a backup file whenever you want a copy, and restore it on a new phone." }
                    }
                }
            }

            footer {
                span { "Traced · made with Rust and Dioxus" }
            }
        }
    }
}

#[component]
fn Download() -> Element {
    rsx! {
        a { class: "apk", href: APK,
            svg { view_box: "0 0 24 24", path { d: "M12 3v12M7 10l5 5 5-5M5 20h14" } }
            span { "Download the APK" small { "v0.7.1 beta · Android 8.0+" } }
        }
    }
}

#[component]
fn Logo() -> Element {
    rsx! {
        img { src: ICON, alt: "" }
    }
}
