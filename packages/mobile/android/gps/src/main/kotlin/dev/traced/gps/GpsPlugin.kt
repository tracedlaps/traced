package dev.traced.gps

import android.Manifest
import android.annotation.SuppressLint
import android.app.Activity
import android.content.Intent
import android.content.pm.PackageManager
import android.location.GnssStatus
import android.location.Location
import android.location.LocationManager
import android.net.Uri
import android.os.Build
import android.os.Handler
import android.os.HandlerThread
import android.os.PowerManager
import android.os.SystemClock
import android.provider.Settings
import android.view.WindowManager
import androidx.core.content.ContextCompat
import androidx.core.location.LocationCompat
import androidx.core.location.LocationListenerCompat
import androidx.core.location.LocationManagerCompat
import com.google.android.gms.common.ConnectionResult
import com.google.android.gms.common.GoogleApiAvailability
import com.google.android.gms.location.LocationCallback
import com.google.android.gms.location.LocationRequest
import com.google.android.gms.location.LocationResult
import com.google.android.gms.location.LocationServices
import com.google.android.gms.location.Priority
import com.google.android.gms.tasks.Tasks
import org.json.JSONArray
import org.json.JSONObject
import java.util.concurrent.ConcurrentLinkedQueue
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

/**
 * Location bridge for Rust (`#[manganis::ffi]`). Every method returns a JSON envelope:
 * `{"ok":true,"data":…}` or `{"ok":false,"error":"permission_denied"|"location_disabled"|"unavailable","detail":…}`.
 *
 * Listeners run on a private HandlerThread and push fixes into a bounded queue that Rust drains.
 */
class GpsPlugin(
    private val activity: Activity,
) {
    private val locationManager = activity.getSystemService(LocationManager::class.java)
    private val fused by lazy { LocationServices.getFusedLocationProviderClient(activity) }
    private val fixes = ConcurrentLinkedQueue<JSONObject>()
    @Volatile private var thread: HandlerThread? = null

    @Volatile private var source: String? = null

    @Volatile private var satellitesUsed = 0

    private var askedBatteryExemption = false

    private val fusedCallback =
        object : LocationCallback() {
            override fun onLocationResult(result: LocationResult) = result.locations.forEach { push(it, FUSED) }
        }
    private val managerListener = LocationListenerCompat { push(it, LOCATION_MANAGER) }
    private val gnssCallback =
        object : GnssStatus.Callback() {
            override fun onSatelliteStatusChanged(status: GnssStatus) {
                satellitesUsed = (0 until status.satelliteCount).count(status::usedInFix)
            }
        }

    fun checkPermissions(): String = ok(JSONObject().put("granted", hasPermission()))

    /** Fires the dialog through MainActivity's launcher and waits (≤60 s) for the user's answer. Never call on the UI thread. */
    fun requestPermissions(): String {
        val fire = requestLocationPermission
        if ((!hasPermission() || !hasNotificationPermission()) && fire != null) {
            val latch = CountDownLatch(1).also { pendingResult = it }
            activity.runOnUiThread { fire() }
            latch.await(PERMISSION_WAIT_SECONDS, TimeUnit.SECONDS)
            pendingResult = null
        }
        // Denied with no rationale to show: Android won't show the dialog again, only Settings can grant it.
        val blocked = !hasPermission() && !activity.shouldShowRequestPermissionRationale(Manifest.permission.ACCESS_FINE_LOCATION)
        return ok(JSONObject().put("granted", hasPermission()).put("blocked", blocked))
    }

    /** Opens this app's page in the system Settings, where a blocked permission can be granted. */
    fun openAppSettings(): String =
        guard {
            val intent = Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.fromParts("package", activity.packageName, null))
            activity.runOnUiThread { activity.startActivity(intent) }
            ok(null)
        }

    /** `optsJson`: `{intervalMs, forceLocationManager}`. Replies `{source}`. Never call on the UI thread. */
    @SuppressLint("MissingPermission")
    fun start(optsJson: String): String =
        guard {
            if (!hasPermission()) return@guard err("permission_denied", "ACCESS_FINE_LOCATION not granted")
            if (!LocationManagerCompat.isLocationEnabled(locationManager)) {
                return@guard err("location_disabled", "Location services are turned off")
            }
            stop()
            val opts = JSONObject(optsJson)
            val intervalMs = opts.optLong("intervalMs", 100)
            val looper = HandlerThread("traced-gps").apply { start() }.also { thread = it }.looper
            locationManager.registerGnssStatusCallback(gnssCallback, Handler(looper))
            source =
                if (!opts.optBoolean("forceLocationManager") && startFused(intervalMs, looper)) {
                    FUSED
                } else {
                    locationManager.requestLocationUpdates(LocationManager.GPS_PROVIDER, 0L, 0f, managerListener, looper)
                    LOCATION_MANAGER
                }
            ok(JSONObject().put("source", source))
        }

    fun stop(): String =
        guard {
            if (source == FUSED) fused.removeLocationUpdates(fusedCallback)
            locationManager.removeUpdates(managerListener)
            locationManager.unregisterGnssStatusCallback(gnssCallback)
            thread?.quitSafely()
            thread = null
            source = null
            satellitesUsed = 0
            fixes.clear()
            ok(null)
        }

    /** Keeps the screen on and, with the permission, runs [RecordingService] so recording survives a locked phone. */
    fun setRecording(on: Boolean): String =
        guard {
            val flag = WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON
            activity.runOnUiThread { if (on) activity.window.addFlags(flag) else activity.window.clearFlags(flag) }
            val service = Intent(activity, RecordingService::class.java)
            if (!on) {
                activity.stopService(service)
            } else if (hasPermission()) {
                ContextCompat.startForegroundService(activity, service)
                askBatteryExemption()
            }
            ok(null)
        }

    /** Fixes received since the last drain, oldest first. */
    fun drainFixes(): String = ok(JSONArray(generateSequence { fixes.poll() }.toList()))

    fun status(): String =
        ok(
            JSONObject()
                .put("locationEnabled", LocationManagerCompat.isLocationEnabled(locationManager))
                .put("satellitesUsed", satellitesUsed)
                .put("source", source ?: JSONObject.NULL),
        )

    /** Fused only when Play Services is usable; any failure (sync or async) falls back to LocationManager. */
    @SuppressLint("MissingPermission")
    private fun startFused(
        intervalMs: Long,
        looper: android.os.Looper,
    ): Boolean {
        if (GoogleApiAvailability.getInstance().isGooglePlayServicesAvailable(activity) != ConnectionResult.SUCCESS) return false
        val request =
            LocationRequest
                .Builder(Priority.PRIORITY_HIGH_ACCURACY, intervalMs)
                .setMinUpdateIntervalMillis(0)
                .setMaxUpdateDelayMillis(0)
                .build()
        return try {
            Tasks.await(fused.requestLocationUpdates(request, fusedCallback, looper), FUSED_WAIT_SECONDS, TimeUnit.SECONDS)
            true
        } catch (e: SecurityException) {
            throw e
        } catch (e: Exception) {
            fused.removeLocationUpdates(fusedCallback) // a timed-out request may still land later
            false
        }
    }

    private fun push(
        location: Location,
        from: String,
    ) {
        if (!location.hasAccuracy()) return
        fixes.offer(fixJson(location, from))
        while (fixes.size > MAX_QUEUED) fixes.poll()
    }

    private fun fixJson(
        l: Location,
        from: String,
    ): JSONObject =
        JSONObject()
            .put("lat", l.latitude)
            .put("lon", l.longitude)
            .put("accuracyM", l.accuracy)
            .put("altitudeM", nullable(l.hasAltitude(), l.altitude))
            .put("verticalAccuracyM", nullable(l.hasVerticalAccuracy(), l.verticalAccuracyMeters))
            .put("speedMps", nullable(l.hasSpeed(), l.speed))
            .put("speedAccuracyMps", nullable(l.hasSpeedAccuracy(), l.speedAccuracyMetersPerSecond))
            .put("bearingDeg", nullable(l.hasBearing(), l.bearing))
            .put("bearingAccuracyDeg", nullable(l.hasBearingAccuracy(), l.bearingAccuracyDegrees))
            // location.time is wall-clock and can jump; anchor the monotonic clock to the boot epoch instead.
            .put("timeMs", System.currentTimeMillis() - SystemClock.elapsedRealtime() + l.elapsedRealtimeNanos / 1_000_000)
            .put("elapsedRealtimeNs", l.elapsedRealtimeNanos)
            .put("isMock", LocationCompat.isMock(l))
            .put("source", from)

    private fun nullable(
        has: Boolean,
        value: Number,
    ): Any = if (has) value else JSONObject.NULL

    private fun hasPermission(): Boolean =
        ContextCompat.checkSelfPermission(activity, Manifest.permission.ACCESS_FINE_LOCATION) == PackageManager.PERMISSION_GRANTED

    /**
     * Once per app run, asks to lift battery optimisation: OEM battery managers (e.g. OnePlus's) kill
     * even a foreground service after minutes with the screen off. The APK is sideloaded, so Play's limits on this don't apply.
     */
    @SuppressLint("BatteryLife")
    private fun askBatteryExemption() {
        val power = activity.getSystemService(PowerManager::class.java)
        if (askedBatteryExemption || power.isIgnoringBatteryOptimizations(activity.packageName)) return
        askedBatteryExemption = true
        val intent = Intent(Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS, Uri.fromParts("package", activity.packageName, null))
        activity.runOnUiThread { activity.startActivity(intent) }
    }

    /** Shows the recording notification; implied before Android 13. */
    private fun hasNotificationPermission(): Boolean =
        Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU ||
            ContextCompat.checkSelfPermission(activity, Manifest.permission.POST_NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED

    private inline fun guard(block: () -> String): String =
        try {
            block()
        } catch (e: SecurityException) {
            err("permission_denied", e.message ?: "permission denied")
        } catch (e: Exception) {
            err("unavailable", e.message ?: e.javaClass.simpleName)
        }

    private fun ok(data: Any?): String = JSONObject().put("ok", true).put("data", data ?: JSONObject.NULL).toString()

    private fun err(
        code: String,
        detail: String,
    ): String = JSONObject().put("ok", false).put("error", code).put("detail", detail).toString()

    companion object {
        private const val FUSED = "fused"
        private const val LOCATION_MANAGER = "location_manager"
        private const val MAX_QUEUED = 64
        private const val PERMISSION_WAIT_SECONDS = 60L
        private const val FUSED_WAIT_SECONDS = 5L

        // Wired by MainActivity: `registerForActivityResult` must run before the activity starts,
        // so the launcher lives there. Same mechanism wry's RustWebChromeClient uses.
        @Volatile
        var requestLocationPermission: (() -> Unit)? = null

        // Only one permission dialog can be on screen at a time, so one static slot is enough.
        @Volatile
        private var pendingResult: CountDownLatch? = null

        /** Invoked by MainActivity's ActivityResultCallback; the result is re-read via checkSelfPermission. */
        @Suppress("UNUSED_PARAMETER")
        fun onPermissionResult(granted: Boolean) {
            pendingResult?.countDown()
        }
    }
}
