-keep class com.voxa.android.** { *; }
-dontwarn okhttp3.**
-dontwarn okio.**

# HeliBoard's IME services + native bridge are loaded reflectively by Android
# and JNI respectively. R8 must not strip or rename them.
-keep class helium314.keyboard.** { *; }
-keepclassmembers class * {
    native <methods>;
}
-dontwarn helium314.keyboard.**
