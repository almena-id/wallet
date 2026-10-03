package id.almena.call

/**
 * What the person did with the last incoming call, until the interface takes
 * it: opened the wallet from it, or answered. Older than an offer lives, it
 * is nothing.
 */
object CallState {
    private const val FRESH_MS = 60_000L

    @Volatile private var action: String? = null
    @Volatile private var at: Long = 0

    fun set(value: String) {
        action = value
        at = System.currentTimeMillis()
    }

    fun take(): Pair<String, Long>? {
        val taken = action ?: return null
        val when_ = at
        action = null
        return if (System.currentTimeMillis() - when_ <= FRESH_MS) taken to when_ else null
    }

    fun clear() {
        action = null
    }
}
