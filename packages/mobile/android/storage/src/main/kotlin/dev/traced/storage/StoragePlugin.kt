package dev.traced.storage

import android.app.Activity

/** App storage bridge for Rust (`#[manganis::ffi]`). */
class StoragePlugin(
    private val activity: Activity,
) {
    /** App-private, needs no permission, and is kept until uninstall. */
    fun dataDir(): String = activity.filesDir.absolutePath
}
