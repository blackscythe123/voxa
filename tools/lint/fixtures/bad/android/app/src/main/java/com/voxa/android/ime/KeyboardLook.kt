// Fixture for tools/lint (contrast): not compiled.

object KeyboardLook {
    val presets = listOf(
        Preset("Black", 0xFF000000.toInt()),     // Good
        Preset("Fog", 0xFF8C8C8C.toInt()),       // Bad: mid grey, neither text colour reaches AAA
    )
    val micColors = listOf(
        MicSwatch("Ocean", 0xFF2F6FDB.toInt()),  // Good
        MicSwatch("Rose", 0xFFD04870.toInt()),   // Bad: 4.3:1 with its icon
    )
}
