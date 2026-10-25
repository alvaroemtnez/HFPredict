# UniFFI's bindings reach the Rust library through JNA, by reflection.
-keep class com.sun.jna.** { *; }
-keep class * implements com.sun.jna.** { *; }
-keep class io.alvaroedo.hfpredict.core.** { *; }
-dontwarn java.awt.**
