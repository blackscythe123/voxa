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
// -Pvoxa.release=true: a real release. The Rust core then refuses to build while the update
// public keys are placeholders (core/voxa-update/build.rs). Everyday builds only warn.
val voxaRelease = (findProperty("voxa.release") as String?) == "true"

// ── Release signing (B2, docs/update-keys.md) ────────────────────────────────────────────────
//
// The permanent release key comes from android/local.properties (never committed) or the
// environment: voxa.release.storeFile / storePassword / keyAlias / keyPassword
// (VOXA_RELEASE_STORE_FILE / _STORE_PASSWORD / _KEY_ALIAS / _KEY_PASSWORD). The APK Signature
// Scheme v3 lineage from the old debug key to it is voxa.release.lineage (VOXA_RELEASE_LINEAGE):
// with it, installs signed with the debug key update in place. AGP can't sign with a lineage, so
// after packaging, lineageSignRelease re-signs the release APKs with apksigner (old signer, then
// --next-signer the release key, --lineage). Without a release key the build falls back to the
// debug key with a loud warning; once the lineage exists that fallback fails the build.
fun releaseProp(key: String, env: String): String? = (localProps.getProperty(key) ?: System.getenv(env))?.trim()?.takeIf { it.isNotEmpty() }
val releaseStoreFile = releaseProp("voxa.release.storeFile", "VOXA_RELEASE_STORE_FILE")
val releaseStorePassword = releaseProp("voxa.release.storePassword", "VOXA_RELEASE_STORE_PASSWORD")
val releaseKeyAlias = releaseProp("voxa.release.keyAlias", "VOXA_RELEASE_KEY_ALIAS")
val releaseKeyPassword = releaseProp("voxa.release.keyPassword", "VOXA_RELEASE_KEY_PASSWORD")
val releaseLineage = releaseProp("voxa.release.lineage", "VOXA_RELEASE_LINEAGE")
val hasReleaseKey = releaseStoreFile != null && File(releaseStoreFile).isFile && releaseStorePassword != null && releaseKeyAlias != null
// Once the owner made the lineage (or says so), a release build may never use the debug key again.
val lineageCreated = (releaseLineage != null && File(releaseLineage).isFile) ||
    releaseProp("voxa.release.lineageCreated", "VOXA_RELEASE_LINEAGE_CREATED") == "true"
val debugKeystore = File(System.getProperty("user.home"), ".android/debug.keystore")

val cargoNdkBuild = tasks.register<Exec>("cargoNdkBuild") {
    group = "rust"
    description = "Builds core/voxa-ffi for ${rustAbis.joinToString()} with cargo-ndk."
    workingDir = repoRoot
    environment("ANDROID_NDK_HOME", "$sdkDir/ndk/$ndkVersionPinned")
    environment("PATH", pathWithCargo)
    if (voxaRelease) environment("VOXA_RELEASE_BUILD", "1")
    val args = mutableListOf(cargo, "ndk")
    rustAbis.forEach { args += listOf("-t", it) }
    args += listOf("-P", "29", "-o", rustJniLibs.get().asFile.path, "build", "-p", "voxa-ffi")
    args += if (rustProfile == "debug" || rustProfile == "dev") listOf() else listOf("--profile", rustProfile)
    commandLine(args)
    inputs.files(fileTree(repoRoot.resolve("core")) { include("**/*.rs", "**/Cargo.toml") })
    inputs.file(repoRoot.resolve("Cargo.lock"))
    inputs.file(repoRoot.resolve("Cargo.toml"))
    inputs.property("rustProfile", rustProfile)
    inputs.property("voxaRelease", voxaRelease)
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
        versionCode = 10000
        versionName = "1.0.0"
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
        if (hasReleaseKey) {
            create("release") {
                storeFile = File(releaseStoreFile!!)
                storePassword = releaseStorePassword
                keyAlias = releaseKeyAlias
                keyPassword = releaseKeyPassword ?: releaseStorePassword
                enableV1Signing = true
                enableV2Signing = true
                enableV3Signing = true
            }
        }
    }

    buildTypes {
        release {
            // R8 (keep rules for JNA and the UniFFI bindings in proguard-rules.pro).
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            if (findProperty("testBuildType") == "release") proguardFiles("proguard-release-under-test.pro")
            // The release key when it is set up, else the debug key with a loud warning (see
            // releaseSigning() below; docs/update-keys.md).
            signingConfig = releaseSigning()
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

    // Voxa's own words are English only; drop the translations libraries bring along.
    androidResources {
        localeFilters += listOf("en")
    }

    packaging {
        // Per-phone APKs: stored uncompressed (smallest on the phone). Universal: compressed (smallest
        // download; the phone's own copy is extracted at install).
        jniLibs.useLegacyPackaging = universal
    }
}

tasks.named("preBuild") { dependsOn(uniffiBindgen) }

/** The release signing config: the permanent key when local.properties or the environment has it,
 *  otherwise today's debug key, with a loud warning. */
fun releaseSigning(): com.android.build.api.dsl.ApkSigningConfig =
    if (hasReleaseKey) android.signingConfigs.getByName("release") else debugFallback()

fun debugFallback(): com.android.build.api.dsl.ApkSigningConfig {
    if (gradle.startParameter.taskNames.any { it.contains("Release", ignoreCase = true) }) {
        logger.warn(
            "\n  WARNING: no Voxa release key (voxa.release.* in android/local.properties). The release APK is signed with the\n" +
                "  DEBUG key, as before. Create the release key and lineage (docs/update-keys.md) before publishing.\n",
        )
    }
    return android.signingConfigs.getByName("debug")
}

// Once the lineage exists, a release build signed with the debug key is refused.
gradle.taskGraph.whenReady {
    val releasing = allTasks.any { it.project == project && it.name.contains("Release") && (it.name.startsWith("assemble") || it.name.startsWith("package")) }
    if (releasing && lineageCreated && !hasReleaseKey) {
        throw GradleException("Voxa: the signing lineage exists, so a release must be signed with the release key, never the debug key. Set voxa.release.* in android/local.properties (docs/update-keys.md).")
    }
}

// Re-sign the release APKs with the v3 lineage: debug key first (old installs trust it), then the
// release key, so 0.3.0 and later debug-signed installs update in place.
val lineageSignRelease = tasks.register("lineageSignRelease") {
    group = "voxa"
    description = "Signs the release APKs with the debug to release key lineage (apksigner rotate output)."
    onlyIf { hasReleaseKey && releaseLineage != null && File(releaseLineage).isFile }
    doLast {
        val bt = File(sdkDir, "build-tools").listFiles()?.filter { it.isDirectory }?.maxByOrNull { it.name }
            ?: throw GradleException("no Android build-tools in $sdkDir")
        val apksigner = File(bt, if (isWindows) "apksigner.bat" else "apksigner").path
        val apks = layout.buildDirectory.dir("outputs/apk/release").get().asFile.listFiles()?.filter { it.name.endsWith(".apk") }.orEmpty()
        for (apk in apks) {
            project.exec {
                commandLine(
                    apksigner, "sign",
                    "--ks", debugKeystore.path, "--ks-pass", "pass:android", "--ks-key-alias", "androiddebugkey", "--key-pass", "pass:android",
                    "--next-signer",
                    "--ks", releaseStoreFile!!, "--ks-pass", "env:VOXA_LINEAGE_STORE_PASS", "--ks-key-alias", releaseKeyAlias!!, "--key-pass", "env:VOXA_LINEAGE_KEY_PASS",
                    "--lineage", releaseLineage!!,
                    "--v1-signing-enabled", "true", "--v2-signing-enabled", "true", "--v3-signing-enabled", "true",
                    apk.path,
                )
                environment("VOXA_LINEAGE_STORE_PASS", releaseStorePassword!!)
                environment("VOXA_LINEAGE_KEY_PASS", releaseKeyPassword ?: releaseStorePassword)
            }
            logger.lifecycle("Voxa: ${apk.name} signed with the debug to release lineage")
        }
    }
}
tasks.matching { it.name == "assembleRelease" }.configureEach { finalizedBy(lineageSignRelease) }

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
