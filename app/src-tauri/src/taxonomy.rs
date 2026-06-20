//! The controlled verb vocabulary — PowerShell-approved-verbs style.
//! Synonyms collapse to one canonical verb so dedup works regardless of wording.

/// Canonical verb → the synonyms that normalize to it.
const VERBS: &[(&str, &[&str])] = &[
    (
        "Create",
        &[
            "new", "insert", "add", "build", "generate", "scaffold", "make", "author", "draft",
            "develop", "implement", "init",
        ],
    ),
    ("Analyze", &["inspect", "assess", "evaluate", "examine", "profile", "measure"]),
    ("Review", &["audit", "critique"]),
    ("Explain", &["document", "summarize", "describe", "guide", "teach"]),
    ("Refactor", &["cleanup", "simplify", "reorganize", "rename"]),
    (
        "Convert",
        &["migrate", "translate", "transform", "format", "import", "export", "port"],
    ),
    ("Optimize", &["tune", "improve"]),
    ("Test", &["validate", "verify", "lint", "assert"]),
    ("Fix", &["debug", "repair", "resolve", "troubleshoot", "patch"]),
    (
        "Search",
        &["find", "extract", "query", "lookup", "classify", "detect", "scrape"],
    ),
    ("Configure", &["setup", "install", "integrate", "provision", "enable"]),
    ("Manage", &["deploy", "monitor", "run", "sync", "schedule", "orchestrate"]),
    ("Design", &["plan", "architect", "model", "compare", "recommend", "spec"]),
];

pub const CANONICAL_VERBS: &[&str] = &[
    "Create", "Analyze", "Review", "Explain", "Refactor", "Convert", "Optimize", "Test", "Fix",
    "Search", "Configure", "Manage", "Design",
];

/// Normalize a verb to its canonical form (a canonical word maps to itself).
/// Returns None for unknown verbs — caller may keep them flagged "uncanonical".
pub fn canonical_verb(word: &str) -> Option<&'static str> {
    let w = word.trim().to_ascii_lowercase();
    if w.is_empty() {
        return None;
    }
    for (canon, syns) in VERBS {
        if canon.eq_ignore_ascii_case(&w) {
            return Some(canon);
        }
        if syns.iter().any(|s| *s == w) {
            return Some(canon);
        }
    }
    None
}

/// The full map, for seeding the editable verb table.
pub fn verb_synonyms() -> &'static [(&'static str, &'static [&'static str])] {
    VERBS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synonyms_map_to_canonical() {
        for w in ["new", "insert", "build", "generate", "scaffold", "make"] {
            assert_eq!(canonical_verb(w), Some("Create"), "{w}");
        }
        assert_eq!(canonical_verb("debug"), Some("Fix"));
        assert_eq!(canonical_verb("migrate"), Some("Convert"));
        assert_eq!(canonical_verb("audit"), Some("Review"));
    }

    #[test]
    fn canonical_words_map_to_themselves_case_insensitive() {
        assert_eq!(canonical_verb("create"), Some("Create"));
        assert_eq!(canonical_verb("CREATE"), Some("Create"));
        assert_eq!(canonical_verb("Design"), Some("Design"));
    }

    #[test]
    fn unknown_returns_none() {
        assert_eq!(canonical_verb("frobnicate"), None);
        assert_eq!(canonical_verb(""), None);
    }

    #[test]
    fn all_canonicals_present_and_self_mapping() {
        assert_eq!(CANONICAL_VERBS.len(), 13);
        for c in CANONICAL_VERBS {
            assert_eq!(canonical_verb(c), Some(*c));
        }
    }
}
