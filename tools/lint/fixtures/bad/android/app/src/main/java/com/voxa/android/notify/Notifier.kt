// Fixture for tools/lint: not compiled.

object Notifier {
    private fun builder(ctx: Context, title: String, body: String): NotificationCompat.Builder =
        NotificationCompat.Builder(ctx, "c")
            .setContentTitle(title)                                   // Good
            .setContentText(body)                                     // Good

    fun preview(ctx: Context, entry: Entry) {
        NotificationCompat.Builder(ctx, "c").setContentText(entry.text)   // Bad: privacy-notify
    }

    fun schedule(ctx: Context) {
        WorkManager.getInstance(ctx).enqueue(req)                     // Bad: android-workmanager
    }
}
