# UniFFI's Kotlin bindings call the Rust library through JNA, which binds native methods and
# Structure fields (RustBuffer, ForeignBytes, callbacks) by reflection. Keep both intact.
-keep class com.sun.jna.** { *; }
-keepclassmembers class * extends com.sun.jna.** { public *; }
# Only the parts JNA reaches by reflection: the two direct mapped libraries (their native methods),
# the C structures (RustBuffer, ForeignBytes, call status, vtables) and the callback glue. The
# generated functions, records, enums and converters are plain Kotlin, so R8 removes the ones the
# app never calls (each unused export used to cost dex bytes against the 6.0 MB budget).
-keep class uniffi.voxa_ffi.UniffiLib { *; }
-keep class uniffi.voxa_ffi.IntegrityCheckingUniffiLib { *; }
-keep class uniffi.voxa_ffi.Uniffi* { *; }
-keep class uniffi.voxa_ffi.uniffi* { *; }
-keep class uniffi.voxa_ffi.RustBuffer* { *; }
-keep class uniffi.voxa_ffi.ForeignBytes* { *; }
-dontwarn java.awt.**
-dontwarn com.sun.jna.**
