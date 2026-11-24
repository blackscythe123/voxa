# Added only for -PtestBuildType=release: the test APK's androidx.test calls Kotlin stdlib methods
# the app itself never uses, which R8 would otherwise remove. The JNA/UniFFI rules are unchanged.
-keep class kotlin.** { *; }
-keep class androidx.** { *; }
