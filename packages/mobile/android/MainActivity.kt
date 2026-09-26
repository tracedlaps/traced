package dev.dioxus.main

import android.Manifest
import androidx.activity.result.contract.ActivityResultContracts
import dev.traced.gps.GpsPlugin

// dx generates BuildConfig in the package named by Dioxus.toml's `bundle.identifier`.
typealias BuildConfig = dev.traced.app.BuildConfig

// Replaces dx's generated MainActivity (Dioxus.toml `application.android_main_activity`).
// `registerForActivityResult` must run before the activity starts, so the location-permission
// launcher lives here and is handed to GpsPlugin's companion. Raw onRequestPermissionsResult
// did not fire reliably under WryActivity.
class MainActivity : WryActivity() {
    private val locationPermissionLauncher =
        registerForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
            GpsPlugin.onPermissionResult(granted)
        }

    init {
        GpsPlugin.requestLocationPermission = {
            locationPermissionLauncher.launch(Manifest.permission.ACCESS_FINE_LOCATION)
        }
    }
}
