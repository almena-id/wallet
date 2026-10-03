package id.almena.call

import app.tauri.notification.Notification
import app.tauri.notification.NotificationPlugin
import app.tauri.plugin.JSObject
import com.google.firebase.messaging.FirebaseMessagingService
import com.google.firebase.messaging.RemoteMessage

/**
 * The app's one Firebase messaging service (it is declared ahead of the
 * notifications plugin's): a call push (`{"type": "almena.call"}`, data only,
 * high priority) rings here; anything else, and every new token, goes to the
 * notifications plugin exactly as its own service would have passed it.
 */
class CallMessagingService : FirebaseMessagingService() {
    override fun onNewToken(token: String) {
        super.onNewToken(token)
        NotificationPlugin.instance?.handleNewToken(token)
    }

    override fun onMessageReceived(message: RemoteMessage) {
        super.onMessageReceived(message)
        if (message.data["type"] == CALL) {
            CallRinger.ring(this)
            return
        }
        passOn(message)
    }

    /** What `TauriFirebaseMessagingService.onMessageReceived` does. */
    private fun passOn(message: RemoteMessage) {
        val pushData = mutableMapOf<String, Any>()
        message.notification?.let { notification ->
            notification.title?.let { pushData["title"] = it }
            notification.body?.let { pushData["body"] = it }
            notification.channelId?.let { pushData["channelId"] = it }
            notification.sound?.let { pushData["sound"] = it }
            notification.tag?.let { pushData["tag"] = it }
        }
        if (message.data.isNotEmpty()) {
            pushData["data"] = message.data
        }
        message.messageId?.let { pushData["messageId"] = it }
        message.from?.let { pushData["from"] = it }
        pushData["sentTime"] = message.sentTime
        NotificationPlugin.instance?.triggerPushMessage(pushData)

        val notification = message.notification ?: return
        val shown = Notification().apply {
            id = System.currentTimeMillis().toInt()
            title = notification.title ?: ""
            body = notification.body
            channelId = notification.channelId
            sound = notification.sound
            if (message.data.isNotEmpty()) {
                val extraData = JSObject()
                for ((key, value) in message.data) {
                    extraData.put(key, value)
                }
                extra = extraData
            }
        }
        NotificationPlugin.triggerNotification(shown, "push")
    }

    private companion object {
        const val CALL = "almena.call"
    }
}
