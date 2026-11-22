pluginManagement {
    repositories {
        google {
            content {
                includeGroupByRegex("com\\.android.*")
                includeGroupByRegex("com\\.google.*")
                includeGroupByRegex("androidx.*")
            }
        }
        mavenCentral()
        gradlePluginPortal()
    }
}
dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        google()
        mavenCentral()
        maven { url = uri("https://jitpack.io") } // required by vendored HeliBoard (colorpicker-compose)
    }
}

rootProject.name = "voxa"
include(":app")

// HeliBoard, vendored as a library module under voxa-kotlin/keyboard/app
// (originally an Android application; converted to com.android.library in our fork
// so it ships inside the main Voxa APK rather than as a separate install).
include(":keyboard")
project(":keyboard").projectDir = file("keyboard/app")
