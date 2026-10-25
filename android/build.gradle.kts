plugins {
    alias(libs.plugins.android.application) apply false
    // Not applied (AGP 9 has built-in Kotlin); only pins the Kotlin version.
    alias(libs.plugins.kotlin.android) apply false
    alias(libs.plugins.kotlin.compose) apply false
}
