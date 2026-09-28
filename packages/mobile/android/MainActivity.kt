package dev.dioxus.main

import android.Manifest
import android.os.Build
import androidx.activity.result.contract.ActivityResultContracts
import dev.traced.gps.GpsPlugin
import dev.traced.storage.StoragePlugin

// dx generates BuildConfig in the package named by Dioxus.toml's `bundle.identifier`.
typealias BuildConfig = com.tracedlaps.app.BuildConfig

// Replaces dx's generated MainActivity (Dioxus.toml `application.android_main_activity`).
// `registerForActivityResult` must run before the activity starts, so the location-permission
// and file launchers live here and are handed to the plugins' companions. Raw onRequestPermissionsResult
// did not fire reliably under WryActivity.
class MainActivity : WryActivity() {
    private val locationPermissionLauncher =
        registerForActivityResult(ActivityResultContracts.RequestMultiplePermissions()) { granted ->
            GpsPlugin.onPermissionResult(granted[Manifest.permission.ACCESS_FINE_LOCATION] == true)
        }
    private val filePicker =
        registerForActivityResult(ActivityResultContracts.OpenDocument(), StoragePlugin::onFileChosen)
    private val fileSaver =
        registerForActivityResult(ActivityResultContracts.CreateDocument("application/vnd.sqlite3"), StoragePlugin::onFileChosen)

    init {
        GpsPlugin.requestLocationPermission = {
            // Notifications too: they show the recording's foreground service (Android 13+).
            val notifications = listOfNotNull(Manifest.permission.POST_NOTIFICATIONS.takeIf { Build.VERSION.SDK_INT >= 33 })
            locationPermissionLauncher.launch(arrayOf(Manifest.permission.ACCESS_FINE_LOCATION) + notifications)
        }
        // CSV, GPX and KML have no reliable MIME types, so any file; the parser tells them apart.
        StoragePlugin.launchFilePicker = { filePicker.launch(arrayOf("*/*")) }
        StoragePlugin.launchFileSaver = { name -> fileSaver.launch(name) }
    }
}
