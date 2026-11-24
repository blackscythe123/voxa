# UniFFI bindings are called through JNA, which finds classes and methods by reflection.
-keep class com.sun.jna.** { *; }
-keep class * implements com.sun.jna.** { *; }
-keep class uniffi.voxa_ffi.** { *; }
-dontwarn java.awt.**
