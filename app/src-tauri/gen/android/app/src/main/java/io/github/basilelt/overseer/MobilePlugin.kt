package io.github.basilelt.overseer

import android.app.Activity
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.provider.Settings
import android.content.res.Configuration
import android.webkit.MimeTypeMap
import androidx.core.view.WindowCompat
import app.tauri.annotation.Command
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import java.io.File

private const val MAX_PASTE = 50L * 1024 * 1024

@TauriPlugin
class MobilePlugin(private val activity: Activity) : Plugin(activity) {
  @Command
  fun minimize(invoke: Invoke) {
    activity.runOnUiThread { activity.moveTaskToBack(true) }
    invoke.resolve()
  }

  @Command
  fun systemDark(invoke: Invoke) {
    val night = activity.resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK
    invoke.resolve(JSObject().put("dark", night == Configuration.UI_MODE_NIGHT_YES))
  }

  @Command
  fun setSystemBars(invoke: Invoke) {
    val dark = invoke.getArgs().getBoolean("dark", false)
    activity.runOnUiThread {
      val bars = WindowCompat.getInsetsController(activity.window, activity.window.decorView)
      bars.isAppearanceLightStatusBars = !dark
      bars.isAppearanceLightNavigationBars = !dark
    }
    invoke.resolve()
  }

  @Command
  fun notificationSettings(invoke: Invoke) {
    try {
      val intent = Intent(Settings.ACTION_APP_NOTIFICATION_SETTINGS)
        .putExtra(Settings.EXTRA_APP_PACKAGE, activity.packageName)
        .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
      activity.startActivity(intent)
      invoke.resolve()
    } catch (ex: Exception) {
      invoke.reject(ex.message ?: "The notification settings could not be opened")
    }
  }

  @Command
  fun clipboardImage(invoke: Invoke) {
    try {
      val clipboard = activity.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
      val clip = clipboard.primaryClip
      if (clip == null) {
        invoke.resolve(JSObject())
        return
      }
      val resolver = activity.contentResolver
      for (i in 0 until clip.itemCount) {
        val uri = clip.getItemAt(i).uri ?: continue
        val mime = resolver.getType(uri) ?: clip.description.getMimeType(0) ?: continue
        if (!mime.startsWith("image/")) continue
        val ext = MimeTypeMap.getSingleton().getExtensionFromMimeType(mime) ?: "png"
        val dir = File(activity.cacheDir, "overseer-paste")
        dir.mkdirs()
        val file = File(dir, System.currentTimeMillis().toString() + "-paste." + ext)
        val input = resolver.openInputStream(uri) ?: continue
        input.use { src -> file.outputStream().use { src.copyTo(it) } }
        if (file.length() > MAX_PASTE) {
          file.delete()
          invoke.reject("The clipboard image is larger than the 50 MiB upload limit")
          return
        }
        invoke.resolve(JSObject().put("path", file.absolutePath))
        return
      }
      invoke.resolve(JSObject())
    } catch (ex: Exception) {
      invoke.reject(ex.message ?: "The clipboard could not be read")
    }
  }
}
