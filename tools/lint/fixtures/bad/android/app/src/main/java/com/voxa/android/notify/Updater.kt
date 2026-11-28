// Fixture for tools/lint (update-gate): not compiled.
object Updater {
    fun install(ctx: Context) {
        val session = ctx.packageManager.packageInstaller.openSession(id)
        session.commit(pending.intentSender)                    // Bad: no live dictation check first
    }

    fun installChecked(ctx: Context) {
        if (dictationLive()) return
        val session = ctx.packageManager.packageInstaller.openSession(id)
        session.commit(pending.intentSender)                    // Good
    }
}
