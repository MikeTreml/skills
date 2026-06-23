pub fn slugify(name: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = true; // true so leading separators are dropped
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lowercases_and_dashes() {
        assert_eq!(slugify("Systematic Debugging"), "systematic-debugging");
    }

    #[test]
    fn collapses_non_alnum_and_trims() {
        assert_eq!(slugify("A/B  Test_Design!"), "a-b-test-design");
        assert_eq!(slugify("  --Edge--  "), "edge");
    }

    #[test]
    fn empty_stays_empty() {
        assert_eq!(slugify("   "), "");
    }
}
