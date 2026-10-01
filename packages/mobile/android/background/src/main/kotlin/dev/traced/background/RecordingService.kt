package dev.traced.background

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.PowerManager
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat

/**
 * Keeps a recording alive with the screen off: as a location foreground service, the GPS and
 * sensors keep their full rate, and the partial wake lock keeps the CPU (and Rust's drain loop) awake.
 */
class RecordingService : Service() {
    private var wakeLock: PowerManager.WakeLock? = null

    override fun onBind(intent: Intent?) = null

    override fun onStartCommand(
        intent: Intent?,
        flags: Int,
        startId: Int,
    ): Int {
        try {
            ServiceCompat.startForeground(this, NOTIFICATION_ID, notification(), ServiceInfo.FOREGROUND_SERVICE_TYPE_LOCATION)
        } catch (e: Exception) { // e.g. the location permission was revoked
            stopSelf()
            return START_NOT_STICKY
        }
        if (wakeLock == null) {
            wakeLock =
                getSystemService(PowerManager::class.java)
                    .newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "traced:recording")
                    .apply { acquire(MAX_RECORDING_MS) }
        }
        return START_NOT_STICKY
    }

    override fun onDestroy() {
        wakeLock?.takeIf { it.isHeld }?.release()
        wakeLock = null
    }

    private fun notification() =
        NotificationCompat
            .Builder(this, CHANNEL)
            .setSmallIcon(icon())
            .setContentTitle("Recording")
            .setContentText("Traced keeps timing with the screen off")
            .setOngoing(true)
            .setContentIntent(
                PendingIntent.getActivity(
                    this,
                    0,
                    packageManager.getLaunchIntentForPackage(packageName),
                    PendingIntent.FLAG_IMMUTABLE,
                ),
            ).build()
            .also {
                getSystemService(NotificationManager::class.java)
                    .createNotificationChannel(NotificationChannel(CHANNEL, "Recording", NotificationManager.IMPORTANCE_LOW))
            }

    /** The app's monochrome launcher icon (overlaid by build-android.sh), else the launcher icon. */
    private fun icon() =
        resources.getIdentifier("ic_launcher_monochrome", "drawable", packageName).takeIf { it != 0 }
            ?: applicationInfo.icon

    companion object {
        private const val CHANNEL = "recording"
        private const val NOTIFICATION_ID = 1

        /** The wake lock's safety timeout, should a stop never come. */
        private const val MAX_RECORDING_MS = 6 * 60 * 60 * 1000L
    }
}
