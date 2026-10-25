package dev.elsewhen.elsewhen_ui

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import android.os.PowerManager
import androidx.core.app.NotificationCompat

/**
 * 配对与初始快照传输期间的保活前台服务。
 *
 * 为什么必须要有：OPPO / 一加（OplusHansManager）以及多数国内 ROM 会冻结
 * 「没有前台服务」的后台进程。进程一进 freezer，Dart 定时器、socket 和
 * 快照分片上传全部停摆——界面看着没崩，但配对轮询永远等不到下一步，初始快照
 * 传到一半就再也不动。带常驻通知的 FGS 进程默认不在冻结名单里，能把这几十
 * 秒到几分钟的传输跑完。
 *
 * 只在 Dart 侧明确 hold 的时候运行（见 lib/sync/sync_keep_alive.dart）：
 * 开始的时机是用户发起加入 / 批准邀请 / 进入快照传输阶段，结束的时机是
 * 交接完成、失败或用户放弃。它不做任何业务逻辑，不碰数据库。
 */
class SyncKeepAliveService : Service() {
    companion object {
        const val ACTION_START = "dev.elsewhen.sync.keepalive.START"
        const val ACTION_STOP = "dev.elsewhen.sync.keepalive.STOP"
        const val EXTRA_TEXT = "text"
        private const val CHANNEL_ID = "sync_keepalive"
        private const val NOTIFICATION_ID = 1061
        private const val DEFAULT_TEXT = "正在与另一台设备同步"

        /**
         * 拉起前台服务。Android 12+ 禁止后台启动 FGS，被拒绝时直接吞掉：
         * 用户此刻多半不在前台，本来也不该弹通知，下一次进入前台会再拉起。
         */
        fun start(context: Context, text: String?) {
            val intent = Intent(context, SyncKeepAliveService::class.java).apply {
                action = ACTION_START
                putExtra(EXTRA_TEXT, text)
            }
            try {
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                    context.startForegroundService(intent)
                } else {
                    context.startService(intent)
                }
            } catch (e: Exception) {
                // ForegroundServiceStartNotAllowedException / SecurityException：
                // 保活是优化项，绝不能因为它让配对流程抛错。
                android.util.Log.w("SyncKeepAlive", "start rejected: ${e.message}")
            }
        }

        fun stop(context: Context) {
            try {
                context.stopService(Intent(context, SyncKeepAliveService::class.java))
            } catch (e: Exception) {
                android.util.Log.w("SyncKeepAlive", "stop failed: ${e.message}")
            }
        }
    }

    private var wakeLock: PowerManager.WakeLock? = null

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent?.action == ACTION_STOP) {
            stopSelf()
            return START_NOT_STICKY
        }
        try {
            startInForeground(intent?.getStringExtra(EXTRA_TEXT))
        } catch (e: Exception) {
            // Android 12+ 后台启动限制 / 12+ 的类型校验都可能在这里抛错。
            // 服务自己挂掉会把整个进程拖死，所以只能放弃保活、干净退出。
            android.util.Log.w("SyncKeepAlive", "startForeground failed: ${e.message}")
            stopSelf()
            return START_NOT_STICKY
        }
        // 屏幕熄灭时 CPU 会跟着休眠，只靠 FGS 仍可能停住分片传输；
        // 传输结束（stopSelf）就释放，不做常驻 wakelock。
        try {
            acquireWakeLock()
        } catch (e: Exception) {
            android.util.Log.w("SyncKeepAlive", "wakelock failed: ${e.message}")
        }
        return START_NOT_STICKY
    }

    override fun onDestroy() {
        releaseWakeLock()
        super.onDestroy()
    }

    private fun startInForeground(text: String?) {
        val manager = getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            val channel = NotificationChannel(
                CHANNEL_ID,
                "设备同步",
                // LOW：不发声、不弹横幅，只在通知栏留一条常驻条目。
                NotificationManager.IMPORTANCE_LOW,
            ).apply {
                description = "配对与初始快照传输期间保持应用运行"
                setShowBadge(false)
            }
            manager.createNotificationChannel(channel)
        }
        val open = PendingIntent.getActivity(
            this,
            0,
            Intent(this, MainActivity::class.java).apply {
                flags = Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP
            },
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        val notification: Notification = NotificationCompat.Builder(this, CHANNEL_ID)
            .setSmallIcon(R.drawable.ic_stat_sync)
            .setContentTitle("Elsewhen 正在同步")
            .setContentText(text?.takeIf { it.isNotBlank() } ?: DEFAULT_TEXT)
            .setContentIntent(open)
            .setOngoing(true)
            .setShowWhen(false)
            .setPriority(NotificationCompat.PRIORITY_LOW)
            .setCategory(NotificationCompat.CATEGORY_PROGRESS)
            .build()
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            startForeground(
                NOTIFICATION_ID,
                notification,
                ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC,
            )
        } else {
            startForeground(NOTIFICATION_ID, notification)
        }
    }

    private fun acquireWakeLock() {
        if (wakeLock?.isHeld == true) return
        val power = getSystemService(Context.POWER_SERVICE) as PowerManager
        wakeLock = power
            .newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "elsewhen:sync_keepalive")
            .apply {
                setReferenceCounted(false)
                acquire(10 * 60 * 1000L)
            }
    }

    private fun releaseWakeLock() {
        try {
            wakeLock?.takeIf { it.isHeld }?.release()
        } catch (_: Exception) {
            // 已超时自动释放，忽略。
        }
        wakeLock = null
    }
}
