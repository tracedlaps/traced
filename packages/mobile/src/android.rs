//! Shared JNI plumbing for the Kotlin plugins.

use manganis::jni::objects::{GlobalRef, JClass, JValue};

/// Constructs the Kotlin plugin `class` (taking the `Activity`). The `#[manganis::ffi]` type's
/// `new()` uses JNI `FindClass`, which only sees the bootstrap classloader on Rust-spawned
/// (e.g. tokio) threads. Loading through the activity's own classloader works from any thread.
pub fn load_plugin(class: &str) -> Option<GlobalRef> {
    manganis::android::with_activity(|env, activity| {
        let plugin = (|| {
            let loader = env
                .call_method(activity, "getClassLoader", "()Ljava/lang/ClassLoader;", &[])
                .ok()?
                .l()
                .ok()?;
            let name = env.new_string(class).ok()?;
            let loaded: JClass = env
                .call_method(
                    &loader,
                    "loadClass",
                    "(Ljava/lang/String;)Ljava/lang/Class;",
                    &[JValue::Object(&name)],
                )
                .ok()?
                .l()
                .ok()?
                .into();
            let instance = env
                .new_object(
                    &loaded,
                    "(Landroid/app/Activity;)V",
                    &[JValue::Object(activity)],
                )
                .ok()?;
            env.new_global_ref(&instance).ok()
        })();
        if plugin.is_none() {
            let _ = env.exception_clear(); // a pending Java exception would abort the next JNI call
        }
        plugin
    })
}
