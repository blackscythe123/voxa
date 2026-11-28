# Fixture for tools/lint (release-latest)

Bad, an Android release that would become "latest":

    gh release create android-v0.3.0 app-arm64-v8a-release.apk --title "Voxa for Android 0.3.0"

Good:

    gh release create android-v0.3.0 app-arm64-v8a-release.apk \
      --title "Voxa for Android 0.3.0" --latest=false

Good, the desktop release is meant to be latest:

    gh release create v0.4.7 dist/Voxa-Setup.exe --title "Voxa v0.4.7"
