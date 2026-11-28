import java.io.ByteArrayOutputStream
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
// ── Release builds (docs/update-keys.md, docs/release-runbook.md) ────────────────────────────
//
// Voxa 1.0.0 and every later version are signed with ONE permanent release key, from day one
// (owner decision 2026-11-28: the pre-1.0 APKs were test builds, so there is no lineage from the
// debug key and no rotation). The key comes from android/local.properties (never committed) or
// the environment: voxa.release.storeFile / storePassword / keyAlias / keyPassword
// (VOXA_RELEASE_STORE_FILE / _STORE_PASSWORD / _KEY_ALIAS / _KEY_PASSWORD).
//
// Any release build (assemble/package/bundle/install ...Release) FAILS when:
//  - the update public keys are still placeholders (it could never update itself), or
//  - no release key is configured (it would be signed with the debug key), or
//  - the configured key is a debug key (~/.android/debug.keystore or alias androiddebugkey), or
//  - after signing, the APK's certificate is a known test-build signer
//    (android/signing/forbidden-signers.txt) or differs from the pinned release certificate
//    (android/signing/release-cert.sha256, once the owner has committed it).
// Only the explicit dev flag -Pvoxa.devRelease=true (or VOXA_DEV_RELEASE=1) allows such a build:
// for local size checks and R8 tests, never handed out. It then says "Updates are off in this build".
fun releaseProp(key: String, env: String): String? = (localProps.getProperty(key) ?: System.getenv(env))?.trim()?.takeIf { it.isNotEmpty() }
val releaseStoreFile = releaseProp("voxa.release.storeFile", "VOXA_RELEASE_STORE_FILE")
val releaseStorePassword = releaseProp("voxa.release.storePassword", "VOXA_RELEASE_STORE_PASSWORD")
val releaseKeyAlias = releaseProp("voxa.release.keyAlias", "VOXA_RELEASE_KEY_ALIAS")
val releaseKeyPassword = releaseProp("voxa.release.keyPassword", "VOXA_RELEASE_KEY_PASSWORD")
val hasReleaseKey = releaseStoreFile != null && File(releaseStoreFile).isFile && releaseStorePassword != null && releaseKeyAlias != null
val debugKeystore = File(System.getProperty("user.home"), ".android/debug.keystore")
val devRelease = (findProperty("voxa.devRelease") as String?) == "true" || System.getenv("VOXA_DEV_RELEASE") == "1"
val signingDir: File = rootProject.file("signing")

/** A release key that is really a debug key: never acceptable for a release. */
fun releaseKeyIsDebug(): Boolean =
    releaseStoreFile != null && (File(releaseStoreFile).canonicalFile == debugKeystore.canonicalFile || releaseKeyAlias.equals("androiddebugkey", ignoreCase = true))

/** The update public keys in core/voxa-update/src/keys.rs are still the all-zero placeholders. */
fun placeholderUpdateKeys(): Boolean = repoRoot.resolve("core/voxa-update/src/keys.rs").readText().contains("0".repeat(64))

/** SHA-256 digests (lowercase hex) listed in a file under android/signing, one per line, # comments. */
fun pinnedDigests(name: String): Set<String> =
    File(signingDir, name).takeIf { it.isFile }?.readLines()?.map { it.substringBefore("#").trim().lowercase() }?.filter { it.matches(Regex("[0-9a-f]{64}")) }?.toSet().orEmpty()

/** True when this Gradle run builds a release APK of this app. */
var releasing = false
gradle.taskGraph.whenReady {
    releasing = allTasks.any { t ->
        t.project == project && t.name.contains("Release") && listOf("assemble", "package", "bundle", "install", "sign").any { t.name.startsWith(it) }
    }
}

val cargoNdkBuild = tasks.register<Exec>("cargoNdkBuild") {
    group = "rust"
    description = "Builds core/voxa-ffi for ${rustAbis.joinToString()} with cargo-ndk."
    workingDir = repoRoot
    environment("ANDROID_NDK_HOME", "$sdkDir/ndk/$ndkVersionPinned")
    environment("PATH", pathWithCargo)
    // A release build tells the Rust core so (core/voxa-update/build.rs refuses placeholder keys
    // too); the dev flag lets a local test build through.
    doFirst {
        if (releasing) (this as Exec).environment("VOXA_RELEASE_BUILD", "1")
        if (devRelease) (this as Exec).environment("VOXA_ALLOW_PLACEHOLDER_KEYS", "1")
    }
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

/** The release signing config: the permanent release key. Without one, only a dev release
 *  (-Pvoxa.devRelease=true) gets this far; it is signed with the debug key and never handed out. */
fun releaseSigning(): com.android.build.api.dsl.ApkSigningConfig =
    if (hasReleaseKey) android.signingConfigs.getByName("release") else devFallback()

fun devFallback(): com.android.build.api.dsl.ApkSigningConfig {
    if (devRelease && gradle.startParameter.taskNames.any { it.contains("Release", ignoreCase = true) }) {
        logger.warn("\n  Voxa DEV release (-Pvoxa.devRelease=true): signed with the DEBUG key, never hand this APK out.\n")
    }
    return android.signingConfigs.getByName("debug")
}

/** Why this release build must not run (null: it may). Pure, so the rules read in one place. */
fun releaseRefusal(releasing: Boolean, dev: Boolean, placeholderKeys: Boolean, hasKey: Boolean, keyIsDebug: Boolean): String? = when {
    !releasing || dev -> null
    placeholderKeys -> "the update public keys in core/voxa-update/src/keys.rs are still placeholders, so this APK could never update itself. Add the real keys (docs/update-keys.md)"
    !hasKey -> "no release key is configured (voxa.release.* in android/local.properties), so it would be signed with the debug key. Set up the release key (docs/update-keys.md)"
    keyIsDebug -> "the configured release key is a debug key. Voxa releases are signed only with the permanent release key (docs/update-keys.md)"
    else -> null
}

gradle.taskGraph.whenReady {
    releaseRefusal(releasing, devRelease, placeholderUpdateKeys(), hasReleaseKey, releaseKeyIsDebug())?.let {
        throw GradleException("Voxa release refused: $it. A local test build only: -Pvoxa.devRelease=true.")
    }
    // Instrumented tests uninstall the app afterwards (and share its id): never on a real phone.
    val deviceTests = allTasks.any { it.project == project && it.name.startsWith("connected") && it.name.endsWith("AndroidTest") }
    val serial = System.getenv("ANDROID_SERIAL").orEmpty()
    if (deviceTests && !serial.startsWith("emulator-") && findProperty("voxa.allowDeviceTests") != "true") {
        throw GradleException("Voxa: instrumented tests run on an emulator only (they uninstall the app and its data). Set ANDROID_SERIAL=emulator-5554.")
    }
}

/** apksigner from the newest build-tools, by version number (not text: "35.0.0-rc1" never beats "35.0.0"). */
fun apksigner(): String {
    fun parts(name: String): List<Int> = name.split('.', '-').map { it.toIntOrNull() ?: -1 }
    val bt = File(sdkDir, "build-tools").listFiles()?.filter { it.isDirectory && !it.name.contains("rc") }
        ?.maxWithOrNull { a, b -> parts(a.name).zip(parts(b.name)).map { (x, y) -> x.compareTo(y) }.firstOrNull { it != 0 } ?: 0 }
        ?: throw GradleException("no Android build-tools in $sdkDir")
    return File(bt, if (isWindows) "apksigner.bat" else "apksigner").path
}

// After packaging: the APK's own signing certificate is the pinned release certificate, and never a
// known test-build signer (the owner's debug key signed every pre-1.0 test APK).
val verifyReleaseSigner = tasks.register("verifyReleaseSigner") {
    group = "voxa"
    description = "Checks the release APKs' signing certificate against android/signing."
    onlyIf { !devRelease }
    doLast {
        val forbidden = pinnedDigests("forbidden-signers.txt")
        val pinned = pinnedDigests("release-cert.sha256")
        val apks = layout.buildDirectory.dir("outputs/apk/release").get().asFile.listFiles()?.filter { it.name.endsWith(".apk") }.orEmpty()
        for (apk in apks) {
            val out = ByteArrayOutputStream()
            project.exec {
                commandLine(apksigner(), "verify", "--print-certs", apk.path)
                standardOutput = out
            }
            val digests = Regex("certificate SHA-256 digest: ([0-9a-fA-F]{64})").findAll(out.toString()).map { it.groupValues[1].lowercase() }.toSet()
            if (digests.isEmpty()) throw GradleException("Voxa: could not read the signing certificate of ${apk.name}")
            digests.firstOrNull { it in forbidden }?.let { throw GradleException("Voxa: ${apk.name} is signed by a test-build key ($it), never a release") }
            if (pinned.isNotEmpty() && digests.any { it !in pinned }) {
                throw GradleException("Voxa: ${apk.name} is signed by $digests, not the pinned release certificate (android/signing/release-cert.sha256)")
            }
            logger.lifecycle("Voxa: ${apk.name} signed by the release certificate ${digests.joinToString()}")
        }
    }
}
tasks.matching { it.name == "assembleRelease" }.configureEach { finalizedBy(verifyReleaseSigner) }

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
