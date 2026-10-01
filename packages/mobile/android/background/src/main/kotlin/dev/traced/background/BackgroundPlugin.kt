package dev.traced.background

import android.Manifest
import android.annotation.SuppressLint
import android.app.Activity
import android.app.ActivityManager
import android.app.ApplicationExitInfo
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.os.PowerManager
import android.provider.Settings
import android.view.WindowManager
import androidx.core.content.ContextCompat
import org.json.JSONObject
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

/**
 * Keeps a recording alive with the screen off, for Rust (`#[manganis::ffi]`), replying in the GPS plugin's JSON envelope:
 * the screen kept on, [RecordingService], and what it needs from the user and the phone's battery manager.
 */
class BackgroundPlugin(
    private val activity: Activity,
) {
    private val prefs = activity.getSharedPreferences("traced-background", Context.MODE_PRIVATE)

    /** On these, the exemption dialog is useless: it grants the exemption, yet the phone still kills Traced. */
    private val strict = Build.MANUFACTURER.lowercase() in STRICT_MANUFACTURERS

    /** Read before anything clears it: the last run ended mid-recording, and not by a crash or the user. */
    private val killedWhileRecording = prefs.getBoolean(RECORDING, false) && !endedByCrashOrUser()

    private var askedExemption = false

    /** Keeps the screen on and, with the location permission, runs [RecordingService] so recording survives a locked phone. */
    fun setRecording(on: Boolean): String =
        guard {
            val flag = WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON
            activity.runOnUiThread { if (on) activity.window.addFlags(flag) else activity.window.clearFlags(flag) }
            prefs.edit().putBoolean(RECORDING, on).apply()
            val service = Intent(activity, RecordingService::class.java)
            if (!on) {
                activity.stopService(service)
            } else if (granted(Manifest.permission.ACCESS_FINE_LOCATION)) { // the service's type is location
                ContextCompat.startForegroundService(activity, service)
                askNotifications()
            }
            ok(null)
        }

    /**
     * `{strict, exempt, killed, notifications}`: whether this is a phone whose own battery manager ignores the system exemption
     * (OnePlus, OPPO, realme: only its per-app "Allow background activity" keeps Traced alive, and that also
     * grants the exemption), whether Traced is exempt, and whether the last run was killed mid-recording, and whether the recording's notification can show.
     */
    fun check(): String =
        ok(
            JSONObject()
                .put("strict", strict)
                .put("exempt", isExempt())
                .put("killed", killedWhileRecording)
                .put("notifications", notificationsAllowed()),
        )

    /** Fires the notification dialog (Android 13+) and waits (≤60 s) for the answer. Never call on the UI thread. */
    fun requestNotifications(): String =
        guard {
            val fire = requestNotificationPermission
            if (!notificationsAllowed() && fire != null) {
                val latch = CountDownLatch(1)
                onNotificationAnswer = latch::countDown
                activity.runOnUiThread { fire() }
                latch.await(PERMISSION_WAIT_SECONDS, TimeUnit.SECONDS)
            }
            ok(null)
        }

    /** The recording's notification (Android 13+), then the exemption: one dialog at a time. */
    private fun askNotifications() {
        val fire = requestNotificationPermission
        if (notificationsAllowed() || fire == null) {
            return askExemption()
        }
        onNotificationAnswer = ::askExemption
        activity.runOnUiThread { fire() }
    }

    /**
     * Once per app run, asks to lift battery optimisation: OEM battery managers (e.g. Samsung's) kill
     * even a foreground service after minutes with the screen off. The APK is sideloaded, so Play's limits on this don't apply.
     */
    @SuppressLint("BatteryLife")
    private fun askExemption() {
        if (strict || askedExemption || isExempt()) return
        askedExemption = true
        val intent = Intent(Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS, Uri.fromParts("package", activity.packageName, null))
        activity.runOnUiThread { activity.startActivity(intent) }
    }

    private fun notificationsAllowed() =
        Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU || granted(Manifest.permission.POST_NOTIFICATIONS)

    private fun isExempt() = activity.getSystemService(PowerManager::class.java).isIgnoringBatteryOptimizations(activity.packageName)

    /** Crashes, ANRs, force stops and swiping Traced away from recents (before Android 11: unknown, so no). */
    private fun endedByCrashOrUser(): Boolean {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.R) return false
        val exit =
            activity.getSystemService(ActivityManager::class.java).getHistoricalProcessExitReasons(activity.packageName, 0, 1).firstOrNull()
                ?: return false
        return exit.reason in USER_OR_CRASH || exit.description?.contains("remove task") == true
    }

    private fun granted(permission: String) = ContextCompat.checkSelfPermission(activity, permission) == PackageManager.PERMISSION_GRANTED

    private inline fun guard(block: () -> String): String =
        try {
            block()
        } catch (e: Exception) {
            err("unavailable", e.message ?: e.javaClass.simpleName)
        }

    private fun ok(data: Any?): String = JSONObject().put("ok", true).put("data", data ?: JSONObject.NULL).toString()

    private fun err(
        code: String,
        detail: String,
    ): String = JSONObject().put("ok", false).put("error", code).put("detail", detail).toString()

    companion object {
        private const val RECORDING = "recording"
        private const val PERMISSION_WAIT_SECONDS = 60L
        private val STRICT_MANUFACTURERS = setOf("oneplus", "oppo", "realme")
        private val USER_OR_CRASH =
            setOf(
                ApplicationExitInfo.REASON_CRASH,
                ApplicationExitInfo.REASON_CRASH_NATIVE,
                ApplicationExitInfo.REASON_ANR,
                ApplicationExitInfo.REASON_USER_REQUESTED,
            )

        // Wired by MainActivity, like GpsPlugin's location launcher.
        @Volatile
        var requestNotificationPermission: (() -> Unit)? = null

        @Volatile
        private var onNotificationAnswer: (() -> Unit)? = null

        /** Invoked by MainActivity's ActivityResultCallback, granted or not. */
        fun onNotificationResult() {
            onNotificationAnswer?.also { onNotificationAnswer = null }?.invoke()
        }
    }
}
