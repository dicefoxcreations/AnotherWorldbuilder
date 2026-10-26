//! Where two texts differ — the way a person would point it out.
//!
//! First line by line; where lines changed, word by word inside them; and
//! where a word changed into a similar one, letter by letter: `aaaa` →
//! `aaabbb` is one `a` gone and `bbb` new, while `aaa` stays. Two words with
//! little in common are shown whole — letter by letter they would fall apart
//! into a confetti of single signs.
//!
//! The same rules serve two purposes: what is written into a layer is this
//! difference (so the history holds changes as a reader sees them), and where
//! two states of a layer cannot be compared by their characters' identity —
//! across generations — it is what gets marked.
//!
//! Positions and lengths count UTF-16 code units, as the browser and the
//! layer documents do.

/// What a piece of text did between the two.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Same,
    Gone,
    New,
}

/// One piece of the way from the old text to the new one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    pub kind: Kind,
    pub text: String,
}

/// A stretch of a text, in UTF-16 code units.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub struct Range {
    pub start: usize,
    pub end: usize,
}

/// One edit of a text: at `at`, take away `delete` units and put `insert`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Splice {
    pub at: usize,
    pub delete: usize,
    pub insert: String,
}

fn units(text: &str) -> usize {
    text.encode_utf16().count()
}

/// The longest common subsequence as a list of steps — or `None` when the
/// table would be too big.
fn lcs<T: PartialEq + Clone>(a: &[T], b: &[T], limit: usize) -> Option<Vec<(Kind, T)>> {
    let (n, m) = (a.len(), b.len());
    if n.saturating_mul(m) > limit {
        return None;
    }
    let mut table = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            table[i][j] = if a[i] == b[j] { table[i + 1][j + 1] + 1 } else { table[i + 1][j].max(table[i][j + 1]) };
        }
    }
    let mut out = Vec::with_capacity(n.max(m));
    let (mut i, mut j) = (0, 0);
    while i < n || j < m {
        if i < n && j < m && a[i] == b[j] {
            out.push((Kind::Same, a[i].clone()));
            i += 1;
            j += 1;
        } else if j < m && (i == n || table[i][j + 1] >= table[i + 1][j]) {
            out.push((Kind::New, b[j].clone()));
            j += 1;
        } else {
            out.push((Kind::Gone, a[i].clone()));
            i += 1;
        }
    }
    Some(out)
}

/// Lines with their line break.
fn lines(text: &str) -> Vec<&str> {
    text.split_inclusive('\n').collect()
}

/// Words, runs of space and single signs — the pieces a change is shown in.
fn words(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let class = |c: char| if c.is_whitespace() { 0 } else if c.is_alphanumeric() || c == '_' { 1 } else { 2 };
    for c in text.chars() {
        match out.last_mut() {
            Some(last) if class(c) != 2 && last.chars().last().is_some_and(|l| class(l) == class(c)) => last.push(c),
            _ => out.push(c.to_string()),
        }
    }
    out
}

fn push(out: &mut Vec<Step>, kind: Kind, text: &str) {
    if text.is_empty() {
        return;
    }
    match out.last_mut() {
        Some(last) if last.kind == kind => last.text.push_str(text),
        _ => out.push(Step { kind, text: text.to_string() }),
    }
}

/// A replaced run, letter by letter — when the two are alike.
fn letters(out: &mut Vec<Step>, gone: &str, new: &str) {
    let a: Vec<char> = gone.chars().collect();
    let b: Vec<char> = new.chars().collect();
    if let Some(steps) = lcs(&a, &b, 250_000) {
        let shared = steps.iter().filter(|(k, _)| *k == Kind::Same).count();
        if shared >= 2 && shared * 2 >= a.len().min(b.len()) {
            for (kind, c) in steps {
                push(out, kind, c.encode_utf8(&mut [0; 4]));
            }
            return;
        }
    }
    push(out, Kind::Gone, gone);
    push(out, Kind::New, new);
}

/// A replaced run of lines, word by word — and alike words letter by letter.
fn inside(out: &mut Vec<Step>, gone: &str, new: &str) {
    let Some(steps) = lcs(&words(gone), &words(new), 2_000_000) else {
        push(out, Kind::Gone, gone);
        push(out, Kind::New, new);
        return;
    };
    let (mut g, mut n) = (String::new(), String::new());
    let flush = |out: &mut Vec<Step>, g: &mut String, n: &mut String| {
        if !g.is_empty() && !n.is_empty() {
            letters(out, g, n);
        } else {
            push(out, Kind::Gone, g);
            push(out, Kind::New, n);
        }
        g.clear();
        n.clear();
    };
    for (kind, word) in steps {
        match kind {
            Kind::Same => {
                flush(out, &mut g, &mut n);
                push(out, Kind::Same, &word);
            }
            Kind::Gone => g.push_str(&word),
            Kind::New => n.push_str(&word),
        }
    }
    flush(out, &mut g, &mut n);
}

/// The way from `old` to `new`, as a reader would see it.
pub fn steps(old: &str, new: &str) -> Vec<Step> {
    let mut out = Vec::new();
    if old == new {
        push(&mut out, Kind::Same, old);
        return out;
    }
    let (a, b) = (lines(old), lines(new));
    // Common lines at the start and the end cost nothing.
    let head = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let tail = a[head..].iter().rev().zip(b[head..].iter().rev()).take_while(|(x, y)| x == y).count();
    for line in &a[..head] {
        push(&mut out, Kind::Same, line);
    }
    let (a_mid, b_mid) = (&a[head..a.len() - tail], &b[head..b.len() - tail]);
    let by_line = lcs(a_mid, b_mid, 2_000_000)
        .unwrap_or_else(|| a_mid.iter().map(|l| (Kind::Gone, *l)).chain(b_mid.iter().map(|l| (Kind::New, *l))).collect());
    let (mut g, mut n) = (String::new(), String::new());
    let flush = |out: &mut Vec<Step>, g: &mut String, n: &mut String| {
        if !g.is_empty() && !n.is_empty() {
            inside(out, g, n);
        } else {
            push(out, Kind::Gone, g);
            push(out, Kind::New, n);
        }
        g.clear();
        n.clear();
    };
    for (kind, line) in by_line {
        match kind {
            Kind::Same => {
                flush(&mut out, &mut g, &mut n);
                push(&mut out, Kind::Same, line);
            }
            Kind::Gone => g.push_str(line),
            Kind::New => n.push_str(line),
        }
    }
    flush(&mut out, &mut g, &mut n);
    for line in &a[a.len() - tail..] {
        push(&mut out, Kind::Same, line);
    }
    out
}

/// What is gone from `old` and what is new in `new`, as stretches of each.
/// Stretches of nothing but space are left out — there is nothing to see.
pub fn ranges(old: &str, new: &str) -> (Vec<Range>, Vec<Range>) {
    let (mut gone, mut added) = (Vec::new(), Vec::new());
    let (mut at_old, mut at_new) = (0, 0);
    for step in steps(old, new) {
        let len = units(&step.text);
        let visible = !step.text.trim().is_empty();
        match step.kind {
            Kind::Same => {
                at_old += len;
                at_new += len;
            }
            Kind::Gone => {
                if visible {
                    gone.push(Range { start: at_old, end: at_old + len });
                }
                at_old += len;
            }
            Kind::New => {
                if visible {
                    added.push(Range { start: at_new, end: at_new + len });
                }
                at_new += len;
            }
        }
    }
    (gone, added)
}

/// The edits that turn `old` into `new`, in order — each position counts in
/// the text as the edits before it left it.
pub fn splices(old: &str, new: &str) -> Vec<Splice> {
    as_splices(steps(old, new))
}

/// The edits to write `new` over `old` — letter by letter, the fewest. What
/// one did not change is never taken out and put back: someone writing at
/// the same moment next to it keeps what they wrote, where they wrote it.
/// (`splices` is the way a reader sees a change — whole words.)
pub fn edits(old: &str, new: &str) -> Vec<Splice> {
    let a: Vec<char> = old.chars().collect();
    let b: Vec<char> = new.chars().collect();
    let head = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let tail = a[head..].iter().rev().zip(b[head..].iter().rev()).take_while(|(x, y)| x == y).count();
    let (a_mid, b_mid) = (&a[head..a.len() - tail], &b[head..b.len() - tail]);
    let text = |c: &[char]| c.iter().collect::<String>();
    let mut out = Vec::new();
    push(&mut out, Kind::Same, &text(&a[..head]));
    match lcs(a_mid, b_mid, 4_000_000) {
        Some(steps) => {
            for (kind, c) in steps {
                push(&mut out, kind, c.encode_utf8(&mut [0; 4]));
            }
        }
        None => {
            push(&mut out, Kind::Gone, &text(a_mid));
            push(&mut out, Kind::New, &text(b_mid));
        }
    }
    push(&mut out, Kind::Same, &text(&a[a.len() - tail..]));
    as_splices(out)
}

fn as_splices(steps: Vec<Step>) -> Vec<Splice> {
    let mut out: Vec<Splice> = Vec::new();
    let mut at = 0;
    for step in steps {
        let len = units(&step.text);
        match step.kind {
            Kind::Same => at += len,
            Kind::Gone => match out.last_mut() {
                Some(last) if last.at + units(&last.insert) == at => last.delete += len,
                _ => out.push(Splice { at, delete: len, insert: String::new() }),
            },
            Kind::New => {
                match out.last_mut() {
                    Some(last) if last.at + units(&last.insert) == at => last.insert.push_str(&step.text),
                    _ => out.push(Splice { at, delete: 0, insert: step.text.clone() }),
                }
                at += len;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(text: &str, splices: &[Splice]) -> String {
        let mut units: Vec<u16> = text.encode_utf16().collect();
        for s in splices {
            units.splice(s.at..s.at + s.delete, s.insert.encode_utf16());
        }
        String::from_utf16(&units).unwrap()
    }

    fn marked(text: &str, ranges: &[Range]) -> Vec<String> {
        let units: Vec<u16> = text.encode_utf16().collect();
        ranges.iter().map(|r| String::from_utf16(&units[r.start..r.end]).unwrap()).collect()
    }

    #[test]
    fn written_letter_by_letter_nothing_unchanged_is_taken_out() {
        let old = "At a pub. G\n\nNext.";
        assert_eq!(edits(old, "At a pub. GM\n\nNext."), vec![Splice { at: 11, delete: 0, insert: "M".into() }]);
        assert_eq!(edits(old, "At a pub. Bob G\n\nNext."), vec![Splice { at: 10, delete: 0, insert: "Bob ".into() }]);
        let both = "Ä🙂 one two three";
        let changed = "Ä🙂 one 2 three!";
        assert_eq!(apply(both, &edits(both, changed)), changed);
    }

    #[test]
    fn a_similar_word_changes_letter_by_letter() {
        let (gone, added) = ranges("aaaa", "aaabbb");
        assert_eq!(marked("aaaa", &gone), ["a"]);
        assert_eq!(marked("aaabbb", &added), ["bbb"]);
        let (gone, added) = ranges("The house.", "The mouse.");
        assert_eq!(marked("The house.", &gone), ["h"]);
        assert_eq!(marked("The mouse.", &added), ["m"]);
    }

    #[test]
    fn unlike_words_stay_whole() {
        let (gone, added) = ranges("It is beautiful.", "It is terrible.");
        assert_eq!(marked("It is beautiful.", &gone), ["beautiful"]);
        assert_eq!(marked("It is terrible.", &added), ["terrible"]);
    }

    #[test]
    fn only_the_changed_line_counts() {
        let (gone, added) = ranges("One.\nTwo.\nThree.\n", "One.\nTwo!\nThree.\nFour.\n");
        assert_eq!(marked("One.\nTwo.\nThree.\n", &gone), ["."]);
        assert_eq!(marked("One.\nTwo!\nThree.\nFour.\n", &added), ["!", "Four.\n"]);
    }

    #[test]
    fn splices_make_the_new_text() {
        for (old, new) in [
            ("aaaa", "aaabbb"),
            ("The inn is old.\nIt rains.\n", "The inn is new.\nIt rains a lot.\nSnow.\n"),
            ("😀 smile", "😀 smiles 😀"),
            ("", "Fresh."),
            ("Gone.", ""),
        ] {
            assert_eq!(apply(old, &splices(old, new)), new, "{old:?} → {new:?}");
        }
    }
}
