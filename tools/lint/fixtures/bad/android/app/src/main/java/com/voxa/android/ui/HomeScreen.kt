// Fixture for tools/lint: not compiled.

@Composable
fun Home(text: String, said: String) {
    Text("Good morning 🌞")                         // Bad: copy-emoji (a surrogate pair is still an emoji)
    Text("You saved 3 minutes – nice")                  // Bad: copy-dash
    Text("Wishing you a happy Pongal")                        // Bad: copy-occasion
    Button(onClick = {}) { Text("Try Again Later") }          // Bad: copy-case
    Text("Try Voxa")                                          // Good
    Text("${a - b} words")                                    // Good: arithmetic in a template
    Diag.log("insert", "pasted: $text")                       // Bad: privacy-log
    Log.i(TAG, "heard ${said.trim()}")                        // Bad: privacy-log
    Diag.log("insert", "pasted ${text.length} chars")         // Good
    Diag.log("ime", "event ${event.javaClass.simpleName}")    // Good
}
