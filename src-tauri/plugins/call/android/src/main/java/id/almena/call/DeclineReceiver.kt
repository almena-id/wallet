package id.almena.call

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent

/**
 * Decline: the ringing stops. Nothing is sent — a closed wallet has no key to
 * say it with — so the caller's wallet rings out.
 */
class DeclineReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        CallRinger.stop(context)
        CallState.clear()
    }
}
