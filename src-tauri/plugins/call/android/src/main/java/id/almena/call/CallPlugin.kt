package id.almena.call

import android.app.Activity
import android.content.Intent
import android.os.Build
import android.view.WindowManager
import android.webkit.WebView
import app.tauri.annotation.Command
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin

/**
 * The wallet's side of a call that rang while it was closed. When it is opened
 * from the call — the full screen, or Answer — the action is kept for the
 * interface to [take] once it is unlocked, and the wallet shows over the lock
 * screen until the call is [settle]d, so its PIN can be typed without first
 * unlocking the phone.
 */
@TauriPlugin
class CallPlugin(private val activity: Activity) : Plugin(activity) {
    override fun load(webView: WebView) {
        arrived(activity.intent)
    }

    override fun onNewIntent(intent: Intent) {
        arrived(intent)
    }

    private fun arrived(intent: Intent?) {
        val action = intent?.getStringExtra(CallRinger.EXTRA) ?: return
        intent.removeExtra(CallRinger.EXTRA)
        CallState.set(action)
        if (action == CallRinger.ANSWER) {
            CallRinger.stop(activity)
        }
        overLockScreen(true)
        val event = JSObject()
        event.put("action", action)
        trigger("call", event)
    }

    @Command
    fun take(invoke: Invoke) {
        val answer = JSObject()
        CallState.take()?.let { (action, at) ->
            answer.put("action", action)
            answer.put("at", at)
        }
        invoke.resolve(answer)
    }

    @Command
    fun settle(invoke: Invoke) {
        CallRinger.stop(activity)
        CallState.clear()
        activity.runOnUiThread { overLockScreen(false) }
        invoke.resolve()
    }

    private fun overLockScreen(on: Boolean) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O_MR1) {
            activity.setShowWhenLocked(on)
            activity.setTurnScreenOn(on)
        } else {
            @Suppress("DEPRECATION")
            val flags = WindowManager.LayoutParams.FLAG_SHOW_WHEN_LOCKED or
                WindowManager.LayoutParams.FLAG_TURN_SCREEN_ON
            if (on) activity.window.addFlags(flags) else activity.window.clearFlags(flags)
        }
    }
}
