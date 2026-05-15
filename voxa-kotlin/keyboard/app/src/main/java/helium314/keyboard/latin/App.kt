// SPDX-License-Identifier: Apache-2.0 AND GPL-3.0-only
package helium314.keyboard.latin

import android.app.Application
import helium314.keyboard.keyboard.emoji.SupportedEmojis
import helium314.keyboard.latin.define.DebugFlags
import helium314.keyboard.latin.settings.Defaults
import helium314.keyboard.latin.settings.Settings
import helium314.keyboard.latin.utils.LayoutUtilsCustom
import helium314.keyboard.latin.utils.Log
import helium314.keyboard.latin.utils.SubtypeSettings

class App : Application() {
    override fun onCreate() {
        super.onCreate()
        initHeliBoard(this)
    }

    companion object {
        // used so JniUtils can access application once
        private var app: Application? = null
        fun getApp(): Application? {
            val application = app
            app = null
            return application
        }

        /**
         * The body of [App.onCreate], extracted so a host application (such as Voxa)
         * can call it from its own `Application.onCreate()`. When HeliBoard is consumed
         * as a library module, the [App] Application class never runs — the host's
         * Application takes its place.
         */
        @JvmStatic
        fun initHeliBoard(application: Application) {
            DebugFlags.init(application)
            Settings.init(application)
            SubtypeSettings.init(application)
            RichInputMethodManager.init(application)

            AppUpgrade.checkVersionUpgrade(application)
            AppUpgrade.transferOldPinnedClips(application)
            app = application
            Defaults.initDynamicDefaults(application)
            LayoutUtilsCustom.removeMissingLayouts(application)
            SupportedEmojis.load(application)

            val packageInfo = application.packageManager.getPackageInfo(application.packageName, 0)
            @Suppress("DEPRECATION")
            Log.i(
                "startup", "Starting ${application.applicationInfo.processName} version ${packageInfo.versionName} (${
                    packageInfo.versionCode
                }) on Android ${android.os.Build.VERSION.RELEASE} (SDK ${android.os.Build.VERSION.SDK_INT})"
            )
        }
    }
}
