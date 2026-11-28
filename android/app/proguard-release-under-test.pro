# Added only for -PtestBuildType=release: the test APK's androidx.test calls Kotlin stdlib methods
# the app itself never uses, which R8 would otherwise remove. The app's own JNA/UniFFI rules are unchanged.
-keep class kotlin.** { *; }
-keep class androidx.** { *; }
# The tests set Voxa's settings with the UniFFI records' copy(...), which the app never calls.
-keep class uniffi.voxa_ffi.** { *; }
# LicencesTest reads the licence data through the Licences object, which R8 would rename.
-keep class com.voxa.android.ui.Licences { *; }
-keep class com.voxa.android.ui.Licences$* { *; }
