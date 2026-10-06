package dev.dioxus.main

import android.Manifest
import android.content.Context
import android.content.Intent
import android.os.Bundle
import android.webkit.WebView
import androidx.activity.OnBackPressedCallback
import androidx.activity.result.contract.ActivityResultContracts
import dev.traced.background.BackgroundPlugin
import dev.traced.gps.GpsPlugin
import dev.traced.storage.StoragePlugin

// dx generates BuildConfig in the package named by Dioxus.toml's `bundle.identifier`.
typealias BuildConfig = com.tracedlaps.app.BuildConfig

// Replaces dx's generated MainActivity (Dioxus.toml `application.android_main_activity`).
// `registerForActivityResult` must run before the activity starts, so the permission
// and file launchers live here and are handed to the plugins' companions. Raw onRequestPermissionsResult
// did not fire reliably under WryActivity.
class MainActivity : WryActivity() {
    private val locationPermissionLauncher =
        registerForActivityResult(ActivityResultContracts.RequestPermission(), GpsPlugin::onPermissionResult)
    private val notificationPermissionLauncher =
        registerForActivityResult(ActivityResultContracts.RequestPermission()) { BackgroundPlugin.onNotificationResult() }
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

    private var webView: WebView? = null

    // The screens live in Rust's router, not the WebView's history, so back asks it to go back a
    // screen (`window.tracedBack`, set by the app) and only leaves Traced from the first one.
    private val back = object : OnBackPressedCallback(true) {
        override fun handleOnBackPressed() {
            val view = webView ?: return leave()
            view.evaluateJavascript("window.tracedBack?.() === true") { if (it != "true") leave() }
        }
    }

    // To the background, not finished, so a recording carries on.
    private fun leave() {
        moveTaskToBack(true)
    }

    override fun onWebViewCreate(webView: WebView) {
        this.webView = webView
    }

    init {
        GpsPlugin.requestLocationPermission = { locationPermissionLauncher.launch(Manifest.permission.ACCESS_FINE_LOCATION) }
        BackgroundPlugin.requestNotificationPermission = { notificationPermissionLauncher.launch(Manifest.permission.POST_NOTIFICATIONS) }
        // CSV, GPX and KML have no reliable MIME types, so any file; the parser tells them apart.
        StoragePlugin.launchFilePicker = { filePicker.launch(arrayOf("*/*")) }
        StoragePlugin.launchFileSaver = { name -> fileSaver.launch(name) }
    }

    // A .gpx opened with or shared to Traced (the storage module's intent filters). Received before
    // super.onCreate starts Rust, so it can take it on start; on onNewIntent, Rust takes it on resume.
    override fun onCreate(savedInstanceState: Bundle?) {
        if (savedInstanceState == null) StoragePlugin.receive(this, intent)
        super.onCreate(savedInstanceState)
        onBackPressedDispatcher.addCallback(this, back)
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        StoragePlugin.receive(this, intent)
    }
}
