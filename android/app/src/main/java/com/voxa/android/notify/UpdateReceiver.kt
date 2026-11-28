package com.voxa.android.notify

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.pm.PackageInstaller
import android.os.Build
import com.voxa.android.diag.Diag

/**
 * Not exported. Two jobs:
 * - PackageInstaller's status for an update Voxa handed it ([Updater.ACTION_INSTALL_STATUS]): on
 *   STATUS_PENDING_USER_ACTION it opens the system's confirmation; every status is logged.
 * - MY_PACKAGE_REPLACED: the new version runs. The staged file goes, the jobs are scheduled again
 *   (session check, background send, update check), and Home says "Voxa was updated to X" once.
 */
class UpdateReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        when (intent.action) {
            Updater.ACTION_INSTALL_STATUS -> {
                val status = intent.getIntExtra(PackageInstaller.EXTRA_STATUS, PackageInstaller.STATUS_FAILURE)
                Updater.installStatus(context, status, intent.getStringExtra(PackageInstaller.EXTRA_STATUS_MESSAGE))
                if (status == PackageInstaller.STATUS_PENDING_USER_ACTION) {
                    val confirm = if (Build.VERSION.SDK_INT >= 33) intent.getParcelableExtra(Intent.EXTRA_INTENT, Intent::class.java)
                    else @Suppress("DEPRECATION") intent.getParcelableExtra(Intent.EXTRA_INTENT)
                    if (confirm != null) {
                        runCatching { context.startActivity(confirm.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) }
                            .onFailure { Diag.log("update", "couldn't open the system confirmation: ${it.javaClass.simpleName}") }
                    }
                }
            }
            Intent.ACTION_MY_PACKAGE_REPLACED -> {
                // History is read to reschedule the send: off the main thread.
                val done = goAsync()
                Thread {
                    runCatching {
                        Updater.replaced(context)
                        VoxaJobs.schedulePeriodic(context)
                        VoxaJobs.scheduleSendIfWaiting(context)
                        if (Updater.auto(context)) VoxaJobs.scheduleUpdateCheck(context)
                    }.onFailure { Diag.log("update", "after the update: ${it.javaClass.simpleName}") }
                    done.finish()
                }.start()
            }
        }
    }
}
