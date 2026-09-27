package dev.dioxus.main

import android.Manifest
import androidx.activity.result.contract.ActivityResultContracts
import dev.traced.gps.GpsPlugin
import dev.traced.storage.StoragePlugin

// dx generates BuildConfig in the package named by Dioxus.toml's `bundle.identifier`.
typealias BuildConfig = dev.traced.app.BuildConfig

// Replaces dx's generated MainActivity (Dioxus.toml `application.android_main_activity`).
// `registerForActivityResult` must run before the activity starts, so the location-permission
// and file-picker launchers live here and are handed to the plugins' companions. Raw onRequestPermissionsResult
// did not fire reliably under WryActivity.
class MainActivity : WryActivity() {
    private val locationPermissionLauncher =
        registerForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
            GpsPlugin.onPermissionResult(granted)
        }
    private val filePicker =
        registerForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
            StoragePlugin.onFilePicked(uri)
        }

    init {
        GpsPlugin.requestLocationPermission = {
            locationPermissionLauncher.launch(Manifest.permission.ACCESS_FINE_LOCATION)
        }
        // CSV, GPX and KML have no reliable MIME types, so any file; the parser tells them apart.
        StoragePlugin.launchFilePicker = { filePicker.launch(arrayOf("*/*")) }
    }
}
