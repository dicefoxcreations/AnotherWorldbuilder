package com.dicefoxcreations.anotherworldbuilder

import android.os.Bundle
import android.os.Build
import android.view.View
import android.webkit.WebView
import androidx.activity.OnBackPressedCallback
import androidx.activity.enableEdgeToEdge
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat

class MainActivity : TauriActivity() {
  // Back first closes what is open in the app — the ⋮ menu, a dialog, the
  // side bar, the options, then the world (`useBack.ts`); only with nothing
  // open does it leave the app. The app has no page history to go back in.
  override val handleBackNavigation: Boolean = false

  override fun onWebViewCreate(webView: WebView) {
    super.onWebViewCreate(webView)
    val back = object : OnBackPressedCallback(true) {
      override fun handleOnBackPressed() {
        webView.evaluateJavascript("window.__awBack ? window.__awBack() : false") { closed ->
          if (closed != "true") {
            isEnabled = false
            onBackPressedDispatcher.onBackPressed()
            isEnabled = true
          }
        }
      }
    }
    onBackPressedDispatcher.addCallback(this, back)
    // The emulator's GPU backend can segfault on SVG masks. Real devices
    // retain hardware acceleration; VM UI tests use Android's CPU renderer.
    if (Build.HARDWARE == "ranchu" || Build.HARDWARE == "goldfish") {
      webView.setLayerType(View.LAYER_TYPE_SOFTWARE, null)
    }
    if (BuildConfig.DEBUG) WebView.setWebContentsDebuggingEnabled(true)
  }

  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    val content = findViewById<View>(android.R.id.content)
    val background = android.graphics.Color.rgb(23, 18, 15)
    content.setBackgroundColor(background)
    window.decorView.setBackgroundColor(background)
    ViewCompat.setOnApplyWindowInsetsListener(content) { view, insets ->
      val area = insets.getInsets(WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout() or WindowInsetsCompat.Type.ime())
      view.setPadding(area.left, area.top, area.right, area.bottom)
      WindowInsetsCompat.CONSUMED
    }
    WindowInsetsControllerCompat(window, window.decorView).apply {
      isAppearanceLightStatusBars = false
      isAppearanceLightNavigationBars = false
    }
    ViewCompat.requestApplyInsets(content)
  }
}
