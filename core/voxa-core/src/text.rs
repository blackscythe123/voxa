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
fn find_phrase(text: &str, phrase: &str) -> Vec<(usize, usize)> {
    let phrase = phrase.trim();
    if phrase.is_empty() {
        return Vec::new();
    }
    let lower = text.to_lowercase();
    let needle = phrase.to_lowercase();
    // Lowercasing can change byte lengths for some scripts; then match exactly instead.
    let (hay, needle) = if lower.len() == text.len() { (lower, needle) } else { (text.to_string(), phrase.to_string()) };
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(i) = hay[from..].find(&needle) {
        let s = from + i;
        let e = s + needle.len();
        let before_ok = text[..s].chars().next_back().is_none_or(|c| !is_word_char(c));
        let after_ok = text[e..].chars().next().is_none_or(|c| !is_word_char(c));
        if before_ok && after_ok {
            out.push((s, e));
        }
        from = e.max(s + 1);
        while !hay.is_char_boundary(from) {
            from += 1;
        }
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
    const COMMANDS: &[(&str, &str)] = &[("new paragraph", "\n\n"), ("next paragraph", "\n\n"), ("new line", "\n"), ("next line", "\n"), ("bullet point", "\n- ")];
    let mut s = text.to_string();
    for (cmd, out) in COMMANDS {
        loop {
            let Some(&(a, b)) = find_phrase(&s, cmd).first() else { break };
            // Drop the space and a comma before; keep a full stop (it ends the sentence before).
            let mut start = a;
            while start > 0 && s[..start].ends_with(' ') {
                start -= 1;
            }
            if s[..start].ends_with(',') {
                start -= 1;
            }
            // Drop punctuation and spaces after.
            let mut end = b;
            while let Some(c) = s[end..].chars().next() {
                if matches!(c, '.' | ',' | ';' | ':' | ' ') { end += c.len_utf8() } else { break }
            }
            s.replace_range(start..end, out);
            // Capitalise what follows a break.
            let after = start + out.len();
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
