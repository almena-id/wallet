package id.almena.call

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.media.AudioAttributes
import android.media.RingtoneManager
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.core.app.Person

/**
 * The incoming call, as Android shows it: a CallStyle notification with
 * Answer and Decline that takes the whole screen when the phone is locked or
 * off, and rings until it is answered, declined or [RING_MS] have passed.
 *
 * It says only that somebody is calling: a closed wallet cannot read who.
 * Answering and the full screen both open the wallet with [EXTRA] set, which
 * [CallPlugin] hands to the interface.
 */
object CallRinger {
    private const val CHANNEL = "almena_calls"
    private const val NOTIFICATION = 0x416c6d
    const val EXTRA = "id.almena.call.action"
    const val OPEN = "open"
    const val ANSWER = "answer"

    /** As long as the caller's wallet rings (`RING_MS` in `src/call.ts`). */
    private const val RING_MS = 45_000L

    fun ring(context: Context) {
        val manager = context.getSystemService(NotificationManager::class.java) ?: return
        channel(context, manager)
        val caller = Person.Builder()
            .setName(context.getString(R.string.almena_call_caller))
            .setImportant(true)
            .build()
        val open = opening(context, OPEN, 1)
        val answer = opening(context, ANSWER, 2)
        val decline = PendingIntent.getBroadcast(
            context,
            3,
            Intent(context, DeclineReceiver::class.java),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
        val notification = NotificationCompat.Builder(context, CHANNEL)
            .setSmallIcon(icon(context))
            .setContentTitle(context.getString(R.string.almena_call_title))
            .setContentText(context.getString(R.string.almena_call_text))
            .setCategory(NotificationCompat.CATEGORY_CALL)
            .setPriority(NotificationCompat.PRIORITY_MAX)
            .setVisibility(NotificationCompat.VISIBILITY_PUBLIC)
            .setOngoing(true)
            .setAutoCancel(true)
            .setTimeoutAfter(RING_MS)
            .setContentIntent(open)
            .setFullScreenIntent(open, true)
            .setStyle(NotificationCompat.CallStyle.forIncomingCall(caller, decline, answer))
            .build()
        // Rings until it is dealt with, as a phone call does, not once.
        notification.flags = notification.flags or Notification.FLAG_INSISTENT
        manager.notify(NOTIFICATION, notification)
    }

    fun stop(context: Context) {
        context.getSystemService(NotificationManager::class.java)?.cancel(NOTIFICATION)
    }

    /** The calls channel: the ringtone, at the highest importance. */
    private fun channel(context: Context, manager: NotificationManager) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
        val channel = NotificationChannel(
            CHANNEL,
            context.getString(R.string.almena_call_channel),
            NotificationManager.IMPORTANCE_HIGH,
        )
        channel.setSound(
            RingtoneManager.getDefaultUri(RingtoneManager.TYPE_RINGTONE),
            AudioAttributes.Builder()
                .setUsage(AudioAttributes.USAGE_NOTIFICATION_RINGTONE)
                .setContentType(AudioAttributes.CONTENT_TYPE_SONIFICATION)
                .build(),
        )
        channel.enableVibration(true)
        channel.lockscreenVisibility = Notification.VISIBILITY_PUBLIC
        manager.createNotificationChannel(channel)
    }

    /** The wallet's own launch, carrying what the person did. */
    private fun opening(context: Context, action: String, code: Int): PendingIntent {
        val intent = (context.packageManager.getLaunchIntentForPackage(context.packageName) ?: Intent())
            .putExtra(EXTRA, action)
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP)
        return PendingIntent.getActivity(
            context,
            code,
            intent,
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
    }

    /** The launcher's monochrome layer, as the wake-up notification uses. */
    private fun icon(context: Context): Int {
        val monochrome = context.resources.getIdentifier("ic_launcher_monochrome", "mipmap", context.packageName)
        return if (monochrome != 0) monochrome else context.applicationInfo.icon
    }
}
