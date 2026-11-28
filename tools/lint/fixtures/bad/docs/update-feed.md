# Fixture for tools/lint (release-latest, the feed release)

Bad, the feed release could become latest:

    gh release create update-feed --title "Update feed"

Good:

    gh release create update-feed --prerelease --latest=false --title "Update feed"
