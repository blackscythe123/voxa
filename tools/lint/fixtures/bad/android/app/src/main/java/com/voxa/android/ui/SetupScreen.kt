// Fixture for tools/lint (android-onboarding-permissions): not compiled.
fun routes(activity: Activity) {
    launcher.launch(Manifest.permission.RECORD_AUDIO)                                    // Good
    launcher.launch(Manifest.permission.POST_NOTIFICATIONS)                              // Bad: notifications during onboarding
    launcher.launch(Manifest.permission.CAMERA)                                          // Bad: another permission
    activity.startActivity(Intent(Settings.ACTION_INPUT_METHOD_SETTINGS))                // Good
    activity.startActivity(Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES, uri))     // Good
    activity.startActivity(Intent(Settings.ACTION_WIFI_SETTINGS))                        // Bad: not an onboarding route
}
