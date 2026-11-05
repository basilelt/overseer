package io.github.basilelt.overseer

import android.app.Activity
import android.content.Intent
import android.content.pm.PackageInfo
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.provider.Settings
import androidx.core.content.FileProvider
import app.tauri.annotation.Command
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import java.io.File
import java.security.MessageDigest

@TauriPlugin
class ApkInstallerPlugin(private val activity: Activity) : Plugin(activity) {
  private fun allowed(): Boolean =
    Build.VERSION.SDK_INT < Build.VERSION_CODES.O || activity.packageManager.canRequestPackageInstalls()

  @Command
  fun openInstallSettings(invoke: Invoke) {
    try {
      if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
        val intent = Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES, Uri.parse("package:" + activity.packageName))
        activity.startActivity(intent)
      }
      invoke.resolve()
    } catch (ex: Exception) {
      invoke.reject(ex.message)
    }
  }

  @Command
  fun installApk(invoke: Invoke) {
    try {
      val file = File(invoke.getArgs().getString("path"))
      if (!file.isFile) {
        invoke.reject("The downloaded APK is missing, download it again")
        return
      }
      if (!allowed()) {
        invoke.resolve(JSObject().put("status", "permission"))
        return
      }
      val problem = mismatch(file)
      if (problem != null) {
        invoke.reject(problem, "signature")
        return
      }
      val uri = FileProvider.getUriForFile(activity, activity.packageName + ".fileprovider", file)
      val intent = Intent(Intent.ACTION_VIEW)
        .setDataAndType(uri, "application/vnd.android.package-archive")
        .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_ACTIVITY_NEW_TASK)
      activity.startActivity(intent)
      invoke.resolve(JSObject().put("status", "started"))
    } catch (ex: Exception) {
      invoke.reject(ex.message)
    }
  }

  @Suppress("DEPRECATION")
  private fun mismatch(file: File): String? {
    val pm = activity.packageManager
    val flags = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) PackageManager.GET_SIGNING_CERTIFICATES else PackageManager.GET_SIGNATURES
    val archive = pm.getPackageArchiveInfo(file.path, flags) ?: return "The downloaded file is not a valid APK"
    if (archive.packageName != activity.packageName) return "The APK is for another app (" + archive.packageName + ")"
    val installed = pm.getPackageInfo(activity.packageName, flags)
    val theirs = signers(archive)
    val ours = signers(installed)
    if (theirs.isEmpty() || ours.isEmpty()) return null
    return if (theirs.intersect(ours).isEmpty()) "The APK is signed with a different key than the installed app" else null
  }

  @Suppress("DEPRECATION")
  private fun signers(info: PackageInfo): Set<String> {
    val signatures = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
      val signing = info.signingInfo ?: return emptySet()
      if (signing.hasMultipleSigners()) signing.apkContentsSigners else signing.signingCertificateHistory
    } else {
      info.signatures
    } ?: return emptySet()
    return signatures.map { sig ->
      MessageDigest.getInstance("SHA-256").digest(sig.toByteArray()).joinToString("") { "%02x".format(it) }
    }.toSet()
  }
}
