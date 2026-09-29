package dev.dioxus.main

import android.Manifest
import android.content.Context
import android.content.Intent
import android.os.Build
import android.os.Bundle
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
    // CreateDocument fixes its MIME type at registration; this one takes it from each file's name.
    private val fileSaver =
        registerForActivityResult(
            object : ActivityResultContracts.CreateDocument("*/*") {
                override fun createIntent(context: Context, input: String) = super.createIntent(context, input).setType(StoragePlugin.mimeOf(input))
            },
            StoragePlugin::onFileChosen,
        )

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

    // A .gpx opened with or shared to Traced (the storage module's intent filters). Received before
    // super.onCreate starts Rust, so it can take it on start; on onNewIntent, Rust takes it on resume.
    override fun onCreate(savedInstanceState: Bundle?) {
        if (savedInstanceState == null) StoragePlugin.receive(this, intent)
        super.onCreate(savedInstanceState)
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        StoragePlugin.receive(this, intent)
    }
}
