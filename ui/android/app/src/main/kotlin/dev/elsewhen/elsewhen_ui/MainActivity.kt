package dev.elsewhen.elsewhen_ui

import android.content.Intent
import androidx.core.content.FileProvider
import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel
import java.io.File

/**
 * 应用内升级：Rust 侧下载并 sha256 校验后的 APK 通过 FileProvider 拉起
 * 系统安装器（用户在系统弹窗确认覆盖安装；首装会让用户授权
 * 「允许安装未知应用」）。
 */
class MainActivity : FlutterActivity() {
    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, "dev.elsewhen/app_update")
            .setMethodCallHandler { call, result ->
                when (call.method) {
                    "getCacheDir" -> {
                        result.success(cacheDir.absolutePath)
                    }
                    "installApk" -> {
                        val path = call.argument<String>("path")
                        if (path == null) {
                            result.error("invalid_args", "path required", null)
                        } else {
                            try {
                                installApk(path)
                                result.success(true)
                            } catch (e: Exception) {
                                result.error("install_failed", e.message, null)
                            }
                        }
                    }
                    else -> result.notImplemented()
                }
            }
    }

    private fun installApk(path: String) {
        val file = File(path)
        if (!file.exists()) throw IllegalArgumentException("apk file missing: $path")
        val uri = FileProvider.getUriForFile(
            this, "${applicationContext.packageName}.update.provider", file
        )
        val intent = Intent(Intent.ACTION_VIEW).apply {
            setDataAndType(uri, "application/vnd.android.package-archive")
            addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
            addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        }
        startActivity(intent)
    }
}
