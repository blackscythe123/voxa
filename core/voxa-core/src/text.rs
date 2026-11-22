//! Optional clean-up of transcribed text. Off by default: users keep their natural wording unless
//! they turn "Remove filler words" on.

const FILLERS: &[&str] = &["um", "umm", "uh", "uhh", "uhm", "erm", "er", "hmm", "hm", "mm", "mmm", "mhm", "ah", "ahh"];

fn is_filler(word: &str) -> bool {
    let core: String = word.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase();
    if core.is_empty() {
        return false;
    }
    // Stretched fillers: "ummmm", "mm-mm-mm".
    let squashed: String = core.chars().filter(|c| c.is_alphabetic()).collect();
    FILLERS.contains(&core.as_str())
        || (squashed.len() >= 2 && squashed.chars().all(|c| c == 'm'))
        || (squashed.starts_with("um") && squashed[1..].chars().all(|c| c == 'm'))
        || (squashed.starts_with("uh") && squashed[1..].chars().all(|c| c == 'h'))
}

/// Remove standalone filler words ("um", "uh", "hmm", "mm-mm"), keeping punctuation tidy and
/// re-capitalising a sentence whose first word was a filler.
pub fn remove_fillers(text: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut capitalize_next = false;
    for word in text.split_whitespace() {
        if is_filler(word) {
            // Keep sentence-ending punctuation the filler carried ("...um." -> "...").
            let tail: String = word.chars().rev().take_while(|c| matches!(c, '.' | '?' | '!')).collect();
            if !tail.is_empty() {
                if let Some(prev) = out.last_mut() {
                    let trimmed = prev.trim_end_matches(',').to_string();
                    *prev = trimmed + &tail.chars().rev().collect::<String>();
                }
            }
            if out.is_empty() || out.last().is_some_and(|w| w.ends_with(['.', '?', '!'])) {
                capitalize_next = true;
            }
            continue;
        }
        let mut w = word.to_string();
        if capitalize_next {
            let mut cs = w.chars();
            if let Some(f) = cs.next() {
                w = f.to_uppercase().collect::<String>() + cs.as_str();
            }
            capitalize_next = false;
        }
        out.push(w);
    }
    // A filler removed between "word," and "word" can leave a dangling comma at the end.
    let mut s = out.join(" ");
    while s.ends_with(',') {
        s.pop();
    }
    s
}
