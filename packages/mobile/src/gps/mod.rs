//! Native GPS bridges. Each platform plugin speaks the JSON envelope in [`wire`].

#[cfg(target_os = "android")]
pub mod android;
// Only the Android bridge uses it; elsewhere it is just unit-tested.
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
pub mod wire;
