package dev.elsewhen.elsewhen_ui

import android.Manifest
import android.content.pm.PackageManager
import android.os.Build
import androidx.core.app.ActivityCompat
import androidx.core.content.ContextCompat
import io.flutter.embedding.android.FlutterFragmentActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel

/// 平台通道只有一条：配对/初始快照传输期间的保活开关。
/// （应用内升级已移除：relay 不再承担 APK 分发，安装靠手动装包。）
class MainActivity : FlutterFragmentActivity() {
    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
        // 配对/初始快照传输期间的保活开关：Dart 侧按「还有多少件事在跑」
        // 计数（lib/sync/sync_keep_alive.dart），这里只负责拉起/停掉前台服务。
        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, "dev.elsewhen/sync_keepalive")
            .setMethodCallHandler { call, result ->
                when (call.method) {
                    "start" -> {
                        ensureNotificationPermission()
                        SyncKeepAliveService.start(this, call.argument<String>("text"))
                        result.success(true)
                    }
                    "stop" -> {
                        SyncKeepAliveService.stop(this)
                        result.success(true)
                    }
                    else -> result.notImplemented()
                }
            }
    }

    /**
     * Android 13+ 的前台服务通知需要 POST_NOTIFICATIONS。没授权时服务照跑、
     * 进程照样豁免冻结，只是通知不可见；所以这里只问一次，拒绝就不再打扰，
     * 绝不能让权限弹窗挡在配对流程前面。
     */
    private fun ensureNotificationPermission() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) return
        val granted = ContextCompat.checkSelfPermission(
            this, Manifest.permission.POST_NOTIFICATIONS,
        ) == PackageManager.PERMISSION_GRANTED
        if (granted) return
        val prefs = getSharedPreferences("sync_keepalive", MODE_PRIVATE)
        if (prefs.getBoolean("notification_asked", false)) return
        prefs.edit().putBoolean("notification_asked", true).apply()
        ActivityCompat.requestPermissions(
            this,
            arrayOf(Manifest.permission.POST_NOTIFICATIONS),
            REQUEST_NOTIFICATIONS,
        )
    }

    private companion object {
        private const val REQUEST_NOTIFICATIONS = 1062
    }
}
