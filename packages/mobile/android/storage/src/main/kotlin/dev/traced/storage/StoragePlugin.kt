package dev.traced.storage

import android.app.Activity
import android.net.Uri
import android.provider.OpenableColumns
import org.json.JSONObject
import java.util.concurrent.CompletableFuture
import java.util.concurrent.TimeUnit

/**
 * App storage bridge for Rust (`#[manganis::ffi]`). `pickFile` replies with the JSON envelope
 * `{"ok":true,"data":…}` or `{"ok":false,"error":"unavailable","detail":…}`.
 */
class StoragePlugin(
    private val activity: Activity,
) {
    /** App-private, needs no permission, and is kept until uninstall. */
    fun dataDir(): String = activity.filesDir.absolutePath

    /** Shows the system file picker and waits for the choice: `{name, text}`, or null if cancelled. Never call on the UI thread. */
    fun pickFile(): String =
        try {
            val launch = launchFilePicker ?: error("no file picker registered")
            val picked = CompletableFuture<Uri?>().also { pending = it }
            activity.runOnUiThread(launch)
            val uri = try { picked.get(PICK_WAIT_MINUTES, TimeUnit.MINUTES) } finally { pending = null }
            ok(uri?.let { JSONObject().put("name", displayName(it)).put("text", read(it)) })
        } catch (e: Exception) {
            JSONObject().put("ok", false).put("error", "unavailable").put("detail", e.message ?: e.javaClass.simpleName).toString()
        }

    private fun read(uri: Uri): String =
        activity.contentResolver.openInputStream(uri)?.bufferedReader()?.use { it.readText() } ?: error("can't open $uri")

    private fun displayName(uri: Uri): String =
        activity.contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use {
            if (it.moveToFirst()) it.getString(0) else null
        } ?: uri.lastPathSegment.orEmpty()

    private fun ok(data: Any?): String = JSONObject().put("ok", true).put("data", data ?: JSONObject.NULL).toString()

    companion object {
        private const val PICK_WAIT_MINUTES = 10L

        // Wired by MainActivity, like GpsPlugin's permission launcher: `registerForActivityResult`
        // must run before the activity starts.
        @Volatile
        var launchFilePicker: (() -> Unit)? = null

        // Only one picker can be on screen at a time, so one static slot is enough.
        @Volatile
        private var pending: CompletableFuture<Uri?>? = null

        /** Invoked by MainActivity's ActivityResultCallback; null when cancelled. */
        fun onFilePicked(uri: Uri?) {
            pending?.complete(uri)
        }
    }
}
