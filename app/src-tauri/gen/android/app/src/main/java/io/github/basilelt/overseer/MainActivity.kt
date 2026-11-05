package io.github.basilelt.overseer

import android.content.res.Configuration
import android.os.Bundle
import android.webkit.WebView
import androidx.activity.enableEdgeToEdge
import androidx.webkit.WebSettingsCompat
import androidx.webkit.WebViewFeature

class MainActivity : TauriActivity() {
  private var webView: WebView? = null
  private var night: Int = Configuration.UI_MODE_NIGHT_UNDEFINED

  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    night = resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK
  }

  override fun onWebViewCreate(webView: WebView) {
    this.webView = webView
    if (WebViewFeature.isFeatureSupported(WebViewFeature.ALGORITHMIC_DARKENING)) {
      WebSettingsCompat.setAlgorithmicDarkeningAllowed(webView.settings, false)
    }
  }

  // The activity handles uiMode itself (no recreate, so the SSH session survives), which leaves
  // the WebView's prefers-color-scheme stale; tell the page directly.
  override fun onConfigurationChanged(newConfig: Configuration) {
    super.onConfigurationChanged(newConfig)
    val now = newConfig.uiMode and Configuration.UI_MODE_NIGHT_MASK
    if (now == night) return
    night = now
    val dark = now == Configuration.UI_MODE_NIGHT_YES
    webView?.evaluateJavascript("window.dispatchEvent(new CustomEvent('overseer:system-theme',{detail:{dark:$dark}}))", null)
  }
}
