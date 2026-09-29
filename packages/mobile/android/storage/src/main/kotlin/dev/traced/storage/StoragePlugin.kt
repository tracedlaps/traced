package dev.traced.storage

import android.app.Activity
import android.content.ClipData
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.provider.OpenableColumns
import android.util.Log
import android.webkit.MimeTypeMap
import androidx.core.content.FileProvider
import androidx.core.content.IntentCompat
import org.json.JSONObject
import java.io.File
import java.util.concurrent.CompletableFuture
import java.util.concurrent.TimeUnit

/**
 * App storage bridge for Rust (`#[manganis::ffi]`). Calls returning JSON reply with the
 * envelope `{"ok":true,"data":…}` or `{"ok":false,"error":"unavailable","detail":…}`.
 */
class StoragePlugin(
    private val activity: Activity,
) {
    /** App-private, needs no permission, and is kept until uninstall. */
    fun dataDir(): String = activity.filesDir.absolutePath

    /** App-private scratch space the system may clear. */
    fun cacheDir(): String = activity.cacheDir.absolutePath

    /**
     * Shows the system file picker and copies the choice into the app's cache, since Rust can't
     * open a `content://` URI: `{name, path}`, or null if cancelled. Never call on the UI thread.
     */
    fun pickFile(): String =
        guard {
            val launch = launchFilePicker ?: error("no file picker registered")
            ok(await { activity.runOnUiThread(launch) }?.let { activity.copy(it, File(activity.cacheDir, "picked")) })
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

    /** The file another app handed us (see [receive]), as `pickFile` replies, then forgets it. */
    fun takeIncoming(): String = guard { ok(incoming.also { incoming = null }) }

    /**
     * Opens the share sheet for the newline-separated `paths`, which must be under `cacheDir/share`
     * (res/xml/file_paths.xml). Doesn't wait for the user's choice.
     */
    fun shareFiles(
        paths: String,
        title: String,
    ): String =
        guard {
            val files = paths.lines().map(::File)
            val uris = files.map { FileProvider.getUriForFile(activity, "${activity.packageName}.fileprovider", it) }
            val intent =
                if (uris.size == 1) {
                    Intent(Intent.ACTION_SEND).putExtra(Intent.EXTRA_STREAM, uris[0])
                } else {
                    Intent(Intent.ACTION_SEND_MULTIPLE).putParcelableArrayListExtra(Intent.EXTRA_STREAM, ArrayList(uris))
                }
            intent.type = files.map { mimeOf(it.name) }.distinct().singleOrNull() ?: "*/*"
            // The read grant covers the URIs in ClipData, not those in EXTRA_STREAM.
            intent.clipData = ClipData.newRawUri(title, uris[0]).apply { uris.drop(1).forEach { addItem(ClipData.Item(it)) } }
            intent.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
            activity.runOnUiThread { activity.startActivity(Intent.createChooser(intent, title)) }
            ok(null)
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

        @Volatile
        private var incoming: JSONObject? = null

        /** Invoked by MainActivity's ActivityResultCallbacks; null when cancelled. */
        fun onFileChosen(uri: Uri?) {
            pending?.complete(uri)
        }

        /** Invoked by MainActivity: keeps a copy of the file a VIEW or SEND `intent` carries, for [takeIncoming]. */
        fun receive(
            context: Context,
            intent: Intent?,
        ) {
            val uri =
                when (intent?.action) {
                    Intent.ACTION_VIEW -> intent.data
                    Intent.ACTION_SEND -> IntentCompat.getParcelableExtra(intent, Intent.EXTRA_STREAM, Uri::class.java)
                    else -> null
                } ?: return
            try {
                val dir = File(context.cacheDir, "incoming").apply { deleteRecursively(); mkdirs() }
                incoming = context.copy(uri, File(dir, File(context.displayName(uri)).name.ifEmpty { "shared" }))
            } catch (e: Exception) {
                Log.w("StoragePlugin", "can't receive $uri", e)
            }
        }

        /** The MIME type of a file called `name`, or the wildcard if unknown. */
        fun mimeOf(name: String): String =
            when (val ext = name.substringAfterLast('.', "").lowercase()) {
                "gpx" -> "application/gpx+xml"
                "db" -> "application/vnd.sqlite3"
                else -> MimeTypeMap.getSingleton().getMimeTypeFromExtension(ext) ?: "*/*"
            }
    }
}

/** Copies `uri` to `dest`, since Rust can't open a `content://` URI: `{name, path}`. */
private fun Context.copy(
    uri: Uri,
    dest: File,
): JSONObject {
    (contentResolver.openInputStream(uri) ?: error("can't open $uri")).use { input -> dest.outputStream().use { input.copyTo(it) } }
    return JSONObject().put("name", displayName(uri)).put("path", dest.absolutePath)
}

private fun Context.displayName(uri: Uri): String =
    contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use {
        if (it.moveToFirst()) it.getString(0) else null
    } ?: uri.lastPathSegment.orEmpty()
