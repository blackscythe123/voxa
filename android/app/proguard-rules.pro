# UniFFI's Kotlin bindings call the Rust library through JNA, which binds native methods and
# Structure fields (RustBuffer, ForeignBytes, callbacks) by reflection. Keep both intact.
-keep class com.sun.jna.** { *; }
-keepclassmembers class * extends com.sun.jna.** { public *; }
-keep class uniffi.voxa_ffi.** { *; }
-dontwarn java.awt.**
-dontwarn com.sun.jna.**
