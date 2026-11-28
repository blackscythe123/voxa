# Only for the instrumented-test APK when testing the R8 release build (-PtestBuildType=release).
-dontwarn com.google.errorprone.annotations.**
-keep class com.voxa.android.** { *; }
-keep class androidx.test.** { *; }
-keep class org.junit.** { *; }
-keep class junit.** { *; }
