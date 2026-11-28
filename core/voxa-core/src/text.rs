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

// ── word list: fixes, snippets, spoken formatting, per-app rules ──────────────────────────────
//
// The transcription service takes audio only (no vocabulary or prompt), so recurring mistakes
// are fixed here, on the device, after the text comes back.

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '\'' || c == '_'
}

/// Byte ranges of `phrase` in `text`, case-insensitive, on word boundaries.
///
/// Matching runs on a lowercased copy, and every match is mapped back to `text` through a table
/// of where each lowercased byte came from. Lowercasing changes byte lengths in some scripts
/// ("ẞ" shrinks, "İ" grows), so offsets of the copy are never used on `text` directly, and a
/// match that starts or ends inside one character's lowercase form is skipped.
fn find_phrase(text: &str, phrase: &str) -> Vec<(usize, usize)> {
    let phrase = phrase.trim();
    if phrase.is_empty() {
        return Vec::new();
    }
    let needle = phrase.to_lowercase();
    // For each byte of `hay`: the byte offset in `text` of the character it came from, and
    // whether it is the first byte of that character's lowercase form.
    let mut hay = String::with_capacity(text.len());
    let mut origin: Vec<(usize, bool)> = Vec::with_capacity(text.len());
    for (o, c) in text.char_indices() {
        let mut first = true;
        for l in c.to_lowercase() {
            for _ in 0..l.len_utf8() {
                origin.push((o, first));
                first = false;
            }
            hay.push(l);
        }
    }
    let to_text = |i: usize| if i == hay.len() { Some(text.len()) } else { origin.get(i).filter(|(_, f)| *f).map(|(o, _)| *o) };
    let mut out: Vec<(usize, usize)> = Vec::new();
    let mut from = 0;
    while let Some(i) = hay[from..].find(&needle) {
        let hs = from + i;
        let he = hs + needle.len();
        let mut next = hs + hay[hs..].chars().next().map_or(1, char::len_utf8);
        if let (Some(s), Some(e)) = (to_text(hs), to_text(he)) {
            let before_ok = text[..s].chars().next_back().is_none_or(|c| !is_word_char(c));
            let after_ok = text[e..].chars().next().is_none_or(|c| !is_word_char(c));
            if before_ok && after_ok && s < e {
                out.push((s, e));
                next = he;
            }
        }
        from = next;
    }
    out
}

/// Replace each phrase (whole words, any case) with its replacement. Longer phrases first, so
/// "claude code" wins over "claude".
pub fn replace_phrases(text: &str, pairs: &[(String, String)]) -> String {
    let mut pairs: Vec<&(String, String)> = pairs.iter().filter(|(f, _)| !f.trim().is_empty()).collect();
    pairs.sort_by_key(|(f, _)| std::cmp::Reverse(f.len()));
    let mut s = text.to_string();
    for (from, to) in pairs {
        let hits = find_phrase(&s, from);
        for (a, b) in hits.into_iter().rev() {
            s.replace_range(a..b, to);
        }
    }
    s
}

/// "new line", "new paragraph" and "bullet point" spoken while dictating become line breaks and
/// bullets. Whisper writes them as words with punctuation around them; that punctuation goes.
pub fn spoken_formatting(text: &str) -> String {
    // (spoken, inserted, strict). Whisper often writes "new line" as one word, "Newline"; the
    // one-word and short forms only count when they end a phrase (punctuation or the end follows),
    // so "the newline character" stays as it is.
    const COMMANDS: &[(&str, &str, bool)] = &[
        ("new paragraph", "\n\n", false),
        ("next paragraph", "\n\n", false),
        ("new para", "\n\n", true),
        ("next para", "\n\n", true),
        ("new line", "\n", false),
        ("next line", "\n", false),
        ("newline", "\n", true),
        ("bullet point", "\n- ", false),
    ];
    let ends_phrase = |rest: &str| rest.trim_start_matches(' ').chars().next().is_none_or(|c| matches!(c, '.' | ',' | '!' | '?' | ';' | ':' | '\n'));
    let mut s = text.to_string();
    for (cmd, out, strict) in COMMANDS {
        let mut from = 0;
        loop {
            let Some(&(a, b)) = find_phrase(&s[from..], cmd).first().map(|(a, b)| (a + from, b + from)).as_ref() else { break };
            if *strict && !ends_phrase(&s[b..]) {
                from = b;
                continue;
            }
            // Drop the space and a comma before; keep a full stop (it ends the sentence before).
            let mut start = a;
            while start > 0 && s[..start].ends_with(' ') {
                start -= 1;
            }
            if s[..start].ends_with(',') {
                start -= 1;
            }
            // Drop punctuation, spaces and any line break Whisper already put there.
            let mut end = b;
            while let Some(c) = s[end..].chars().next() {
                if matches!(c, '.' | ',' | ';' | ':' | ' ' | '\n') { end += c.len_utf8() } else { break }
            }
            s.replace_range(start..end, out);
            // Capitalise what follows a break.
            let after = start + out.len();
            from = after;
            if let Some(c) = s[after..].chars().next() {
                if c.is_lowercase() {
                    let up: String = c.to_uppercase().collect();
                    s.replace_range(after..after + c.len_utf8(), &up);
                }
            }
        }
    }
    s.trim().to_string()
}

/// Per-app style: no full stop at the end (chat apps, terminals), and/or all lowercase.
pub fn apply_style(text: &str, no_final_period: bool, lowercase: bool) -> String {
    let mut s = text.trim().to_string();
    if no_final_period && s.ends_with('.') && !s.ends_with("...") {
        s.pop();
    }
    if lowercase {
        s = s.to_lowercase();
    }
    s
}

/// After the user edits a dictation, the one short phrase they changed, if that is all they did:
/// (what it said, what they wanted). Used to offer "Always fix this?".
pub fn changed_phrase(before: &str, after: &str) -> Option<(String, String)> {
    let a: Vec<&str> = before.split_whitespace().collect();
    let b: Vec<&str> = after.split_whitespace().collect();
    let norm = |w: &str| w.trim_matches(|c: char| !c.is_alphanumeric()).to_string();
    let mut pre = 0;
    while pre < a.len() && pre < b.len() && norm(a[pre]) == norm(b[pre]) {
        pre += 1;
    }
    let mut suf = 0;
    while suf < a.len() - pre && suf < b.len() - pre && norm(a[a.len() - 1 - suf]) == norm(b[b.len() - 1 - suf]) {
        suf += 1;
    }
    let old = &a[pre..a.len() - suf];
    let new = &b[pre..b.len() - suf];
    let clean = |ws: &[&str]| ws.iter().map(|w| w.trim_matches(|c: char| !c.is_alphanumeric() && c != '\'')).collect::<Vec<_>>().join(" ");
    let (o, n) = (clean(old), clean(new));
    (!old.is_empty() && !new.is_empty() && old.len() <= 4 && new.len() <= 4 && o != n && !o.is_empty() && !n.is_empty()).then_some((o, n))
}
