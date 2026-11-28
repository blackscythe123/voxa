// Fixture for tools/lint: not compiled.

plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.android)
    alias(libs.plugins.kotlin.compose)
    alias(libs.plugins.ksp)                                       // Bad: android-deps (new plugin)
}

dependencies {
    implementation(libs.androidx.core.ktx)
    implementation(platform(libs.androidx.compose.bom))
    implementation(libs.androidx.material3)
    implementation("com.squareup.okhttp3:okhttp:4.12.0")           // Bad: android-deps
    implementation("androidx.work:work-runtime-ktx:2.9.0")         // Bad: android-deps and android-workmanager
    debugImplementation(libs.androidx.ui.tooling)
}

android {
    defaultConfig {
        versionCode = 6                                           // Bad: release-version-sync (1.0.0 is 10000)
        versionName = "1.0.0"
    }
    buildTypes {
        release {
            signingConfig = signingConfigs.getByName("debug")     // Bad: android-signing
        }
        debug {
            signingConfig = signingConfigs.getByName("debug")     // Good: debug builds
        }
    }
}
