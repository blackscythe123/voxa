import java.util.Properties

plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.android)
    alias(libs.plugins.kotlin.compose)
}

// ── The shared Rust core (core/voxa-ffi), built with cargo-ndk, bindings from UniFFI ──────────
//
// preBuild runs `cargo ndk` for arm64-v8a (phones), then generates the Kotlin bindings from the
// built library. Phones only by default; for the x86_64 emulator pass -Pabis=arm64-v8a,x86_64.
// Pass -PskipRust to reuse what was built last time.

val repoRoot: File = rootProject.projectDir.parentFile
val ndkVersionPinned = "28.0.13004108"
val rustAbis: List<String> = ((findProperty("abis") as String?) ?: if (project.hasProperty("universal")) "arm64-v8a,armeabi-v7a,x86_64" else "arm64-v8a").split(",").map { it.trim() }.filter { it.isNotEmpty() }
val abiTriple = mapOf("arm64-v8a" to "aarch64-linux-android", "armeabi-v7a" to "armv7-linux-androideabi", "x86_64" to "x86_64-linux-android")
// -Puniversal: one APK for every phone (arm64, 32-bit ARM, x86_64), libraries compressed so the
// download stays small; Android extracts only the phone's own copy at install.
val universal = project.hasProperty("universal")
val rustJniLibs = layout.buildDirectory.dir("rustJniLibs")
val uniffiOut = layout.buildDirectory.dir("generated/uniffi")

val localProps = Properties().apply {
    val f = rootProject.file("local.properties")
    if (f.exists()) f.inputStream().use { load(it) }
}
val sdkDir: String = localProps.getProperty("sdk.dir") ?: System.getenv("ANDROID_HOME") ?: ""
val isWindows = System.getProperty("os.name").lowercase().contains("windows")
val cargoHome = File(System.getenv("CARGO_HOME") ?: "${System.getProperty("user.home")}/.cargo")
val cargo: String = File(cargoHome, "bin/cargo" + if (isWindows) ".exe" else "").takeIf { it.exists() }?.path ?: "cargo"
val pathWithCargo = File(cargoHome, "bin").path + File.pathSeparator + (System.getenv("PATH") ?: "")
// Cargo profile for the .so: "release-android" (root Cargo.toml: opt-level z, fat LTO, one codegen
// unit, panic = abort, stripped) by default; -PrustProfile=debug for faster local Rust builds.
val rustProfile = (findProperty("rustProfile") as String?) ?: "release-android"
val rustProfileDir = if (rustProfile == "dev") "debug" else rustProfile

val cargoNdkBuild = tasks.register<Exec>("cargoNdkBuild") {
    group = "rust"
    description = "Builds core/voxa-ffi for ${rustAbis.joinToString()} with cargo-ndk."
    workingDir = repoRoot
    environment("ANDROID_NDK_HOME", "$sdkDir/ndk/$ndkVersionPinned")
    environment("PATH", pathWithCargo)
    val args = mutableListOf(cargo, "ndk")
    rustAbis.forEach { args += listOf("-t", it) }
    args += listOf("-P", "29", "-o", rustJniLibs.get().asFile.path, "build", "-p", "voxa-ffi")
    args += if (rustProfile == "debug" || rustProfile == "dev") listOf() else listOf("--profile", rustProfile)
    commandLine(args)
    inputs.files(fileTree(repoRoot.resolve("core")) { include("**/*.rs", "**/Cargo.toml") })
    inputs.file(repoRoot.resolve("Cargo.lock"))
    inputs.file(repoRoot.resolve("Cargo.toml"))
    inputs.property("rustProfile", rustProfile)
    outputs.dir(rustJniLibs)
    onlyIf { !project.hasProperty("skipRust") }
}

val uniffiBindgen = tasks.register<Exec>("uniffiBindgen") {
    group = "rust"
    description = "Generates the Kotlin bindings for voxa-ffi with uniffi-bindgen (library mode)."
    dependsOn(cargoNdkBuild)
    workingDir = repoRoot
    environment("PATH", pathWithCargo)
    val lib = repoRoot.resolve("target/${abiTriple.getValue(rustAbis.first())}/${if (rustProfile == "debug") "debug" else rustProfileDir}/libvoxa_ffi.so")
    commandLine(
        cargo, "run", "-q", "-p", "voxa-ffi", "--features", "cli", "--bin", "uniffi-bindgen", "--",
        "generate", "--library", lib.path, "--language", "kotlin", "--no-format",
        "--out-dir", uniffiOut.get().asFile.path,
    )
    inputs.files(cargoNdkBuild)
    outputs.dir(uniffiOut)
    onlyIf { !project.hasProperty("skipRust") }
}

android {
    namespace = "com.voxa.android"
    compileSdk = 35
    ndkVersion = ndkVersionPinned

    defaultConfig {
        // Same id as the old app, so it installs as an update.
        applicationId = "com.voxa.android"
        minSdk = 29
        targetSdk = 35
        versionCode = 5
        versionName = "0.3.0"
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        testProguardFiles("proguard-test-rules.pro")
        // Only the ABIs the Rust core is built for, so no device installs an APK without it.
        if (universal) ndk { abiFilters += rustAbis }
    }

    // Older and newer signature schemes too, so every installer accepts the APK.
    signingConfigs {
        getByName("debug") {
            enableV1Signing = true
            enableV2Signing = true
            enableV3Signing = true
        }
    }

    buildTypes {
        release {
            // R8 (keep rules for JNA and the UniFFI bindings in proguard-rules.pro).
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            if (findProperty("testBuildType") == "release") proguardFiles("proguard-release-under-test.pro")
            // As before: the debug keystore, so the APK is sideload-installable. A real release key
            // is still to be set up (every key change needs an uninstall).
            signingConfig = signingConfigs.getByName("debug")
        }
    }

    // One APK per ABI: a phone gets only its own .so. No universal APK.
    splits {
        abi {
            isEnable = !universal
            reset()
            include(*rustAbis.toTypedArray())
            isUniversalApk = false
        }
    }

    // -PtestBuildType=release runs the instrumented tests against the R8-minified build.
    testBuildType = (findProperty("testBuildType") as String?) ?: "debug"

    sourceSets {
        getByName("main") {
            jniLibs.srcDir(rustJniLibs)
            java.srcDir(uniffiOut)
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlinOptions {
        jvmTarget = "17"
    }

    buildFeatures {
        compose = true
        buildConfig = true
    }

    packaging {
        // Per-phone APKs: stored uncompressed (smallest on the phone). Universal: compressed (smallest
        // download; the phone's own copy is extracted at install).
        jniLibs.useLegacyPackaging = universal
    }
}

tasks.named("preBuild") { dependsOn(uniffiBindgen) }

dependencies {
    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.activity.compose)
    implementation(platform(libs.androidx.compose.bom))
    implementation(libs.androidx.ui)
    implementation(libs.androidx.ui.graphics)
    implementation(libs.androidx.material3)
    // The Android 12+ splash screen, back-ported (a few KB): paper background, then the ink line.
    implementation(libs.androidx.core.splashscreen)
    implementation(libs.kotlinx.coroutines.android)
    // UniFFI's Kotlin bindings call the Rust library through JNA.
    implementation("${libs.jna.get()}@aar")
    debugImplementation(libs.androidx.ui.tooling)

    androidTestImplementation(libs.androidx.test.runner)
    androidTestImplementation(libs.androidx.test.ext.junit)
    androidTestImplementation(libs.junit)
}
