package dev.traced.storage

import android.app.Activity
import android.net.Uri
import android.provider.OpenableColumns
import org.json.JSONObject
import java.io.File
import java.util.concurrent.CompletableFuture
import java.util.concurrent.TimeUnit

/**
 * App storage bridge for Rust (`#[manganis::ffi]`). `pickFile` and `saveFile` reply with the
 * JSON envelope `{"ok":true,"data":…}` or `{"ok":false,"error":"unavailable","detail":…}`.
 */
class StoragePlugin(
    private val activity: Activity,
) {
    /** App-private, needs no permission, and is kept until uninstall. */
    fun dataDir(): String = activity.filesDir.absolutePath

    /**
     * Shows the system file picker and copies the choice into the app's cache, since Rust can't
     * open a `content://` URI: `{name, path}`, or null if cancelled. Never call on the UI thread.
     */
    fun pickFile(): String =
        guard {
            val launch = launchFilePicker ?: error("no file picker registered")
            ok(
                await { activity.runOnUiThread(launch) }?.let { uri ->
                    val copy = File(activity.cacheDir, "picked")
                    open(uri) { input -> copy.outputStream().use { input.copyTo(it) } }
                    JSONObject().put("name", displayName(uri)).put("path", copy.absolutePath)
                },
            )
        }

    /** Asks where to save a copy of the file at `path`, offering `name`: true if saved, false if cancelled. Never call on the UI thread. */
    fun saveFile(
        path: String,
        name: String,
    ): String =
        guard {
            val launch = launchFileSaver ?: error("no file saver registered")
            val uri = await { activity.runOnUiThread { launch(name) } } ?: return@guard ok(false)
            val output = activity.contentResolver.openOutputStream(uri) ?: error("can't write $uri")
            output.use { out -> File(path).inputStream().use { it.copyTo(out) } }
            ok(true)
        }

    /** Starts a picker with `launch` and waits for its answer. */
    private fun await(launch: () -> Unit): Uri? {
        val answer = CompletableFuture<Uri?>().also { pending = it }
        launch()
        return try {
            answer.get(PICK_WAIT_MINUTES, TimeUnit.MINUTES)
        } finally {
            pending = null
        }
    }

    private fun open(
        uri: Uri,
        read: (java.io.InputStream) -> Unit,
    ) = (activity.contentResolver.openInputStream(uri) ?: error("can't open $uri")).use(read)

    private fun displayName(uri: Uri): String =
        activity.contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use {
            if (it.moveToFirst()) it.getString(0) else null
        } ?: uri.lastPathSegment.orEmpty()

    private inline fun guard(block: () -> String): String =
        try {
            block()
        } catch (e: Exception) {
            JSONObject().put("ok", false).put("error", "unavailable").put("detail", e.message ?: e.javaClass.simpleName).toString()
        }

    private fun ok(data: Any?): String = JSONObject().put("ok", true).put("data", data ?: JSONObject.NULL).toString()

    companion object {
        private const val PICK_WAIT_MINUTES = 10L

        // Wired by MainActivity, like GpsPlugin's permission launcher: `registerForActivityResult`
        // must run before the activity starts.
        @Volatile
        var launchFilePicker: (() -> Unit)? = null

        @Volatile
        var launchFileSaver: ((String) -> Unit)? = null

        // Only one picker can be on screen at a time, so one static slot is enough.
        @Volatile
        private var pending: CompletableFuture<Uri?>? = null

        /** Invoked by MainActivity's ActivityResultCallbacks; null when cancelled. */
        fun onFileChosen(uri: Uri?) {
            pending?.complete(uri)
        }
    }
}
