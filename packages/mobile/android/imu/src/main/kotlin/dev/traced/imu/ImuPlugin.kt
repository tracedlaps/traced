package dev.traced.imu

import android.app.Activity
import android.hardware.Sensor
import android.hardware.SensorEvent
import android.hardware.SensorEventListener
import android.hardware.SensorManager
import android.os.Handler
import android.os.HandlerThread
import android.os.SystemClock
import org.json.JSONArray
import org.json.JSONObject
import java.util.concurrent.ConcurrentLinkedQueue
import kotlin.math.exp

/**
 * Accelerometer bridge for Rust (`#[manganis::ffi]`), replying in the GPS plugin's JSON envelope.
 *
 * Listeners run on a private HandlerThread and queue `[timeMs, [accel], [gravity]]` samples
 * (m/s², device axes) that Rust drains. Linear acceleration and gravity come from the fused
 * sensors; without them, gravity is low-passed out of the raw accelerometer here.
 */
class ImuPlugin(
    activity: Activity,
) {
    private val sensors = activity.getSystemService(SensorManager::class.java)
    private val samples = ConcurrentLinkedQueue<JSONArray>()

    @Volatile private var thread: HandlerThread? = null

    /** Bumped by each start, so a stale stop can't end a newer session. */
    @Volatile private var session = 0

    private var gravity = FloatArray(3)
    private var gravityAt = 0L

    private val listener =
        object : SensorEventListener {
            override fun onSensorChanged(e: SensorEvent) {
                when (e.sensor.type) {
                    Sensor.TYPE_GRAVITY -> gravity = e.values.copyOf(3)
                    Sensor.TYPE_LINEAR_ACCELERATION -> push(e.timestamp, e.values)
                    Sensor.TYPE_ACCELEROMETER -> {
                        val k = if (gravityAt == 0L) 1f else 1f - exp(-(e.timestamp - gravityAt) / GRAVITY_TAU_NS)
                        gravity = FloatArray(3) { gravity[it] + k * (e.values[it] - gravity[it]) }
                        gravityAt = e.timestamp
                        push(e.timestamp, FloatArray(3) { e.values[it] - gravity[it] })
                    }
                }
            }

            override fun onAccuracyChanged(
                sensor: Sensor,
                accuracy: Int,
            ) = Unit
        }

    /** Replies `{session, fused}`, or `unavailable` without an accelerometer. */
    fun start(): String =
        guard {
            stopSensors()
            val linear = sensors.getDefaultSensor(Sensor.TYPE_LINEAR_ACCELERATION)
            val grav = sensors.getDefaultSensor(Sensor.TYPE_GRAVITY)
            val raw = sensors.getDefaultSensor(Sensor.TYPE_ACCELEROMETER)
            val used = if (linear != null && grav != null) listOf(grav, linear) else listOfNotNull(raw)
            if (used.isEmpty()) return@guard err("unavailable", "no accelerometer")
            val handler = Handler(HandlerThread("traced-imu").apply { start() }.also { thread = it }.looper)
            used.forEach { sensors.registerListener(listener, it, SAMPLING_US, handler) }
            ok(JSONObject().put("session", ++session).put("fused", used.size == 2))
        }

    fun stop(session: Int): String =
        guard {
            if (session == this.session) stopSensors()
            ok(null)
        }

    /** Samples since the last drain, oldest first. */
    fun drain(): String = ok(JSONArray(generateSequence { samples.poll() }.toList()))

    private fun stopSensors() {
        sensors.unregisterListener(listener)
        thread?.quitSafely()
        thread = null
        samples.clear()
        gravity = FloatArray(3)
        gravityAt = 0L
    }

    private fun push(
        timestampNs: Long,
        accel: FloatArray,
    ) {
        // Sensor timestamps run on the boot clock, like fixes' elapsedRealtimeNanos: anchored the same way.
        val timeMs = System.currentTimeMillis() - SystemClock.elapsedRealtime() + timestampNs / 1_000_000
        samples.offer(JSONArray().put(timeMs).put(JSONArray(accel.take(3))).put(JSONArray(gravity.toList())))
        while (samples.size > MAX_QUEUED) samples.poll()
    }

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
        /** ~50 Hz (SENSOR_DELAY_GAME). */
        private const val SAMPLING_US = 20_000
        private const val MAX_QUEUED = 256
        private const val GRAVITY_TAU_NS = 500_000_000f
    }
}
