/// Extracted front-matter fields. Missing fields become empty strings.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Meta {
    pub name: String,
    pub description: String,
}

/// Parse the leading `---` YAML front matter for `name:` and `description:`.
/// Only simple single-line scalar values are supported (sufficient for SKILL.md).
pub fn parse_meta(content: &str) -> Meta {
    let mut meta = Meta::default();
    let trimmed = content.trim_start_matches('\u{feff}');
    let mut lines = trimmed.lines();
    if lines.next().map(str::trim) != Some("---") {
        return meta;
    }
    for line in lines {
        let line = line.trim_end();
        if line.trim() == "---" {
            break;
        }
        if let Some((key, value)) = line.split_once(':') {
            let key = key.trim();
            let value = unquote(value.trim());
            match key {
                "name" => meta.name = value,
                "description" => meta.description = value,
                _ => {}
            }
        }
    }
    meta
}

fn unquote(s: &str) -> String {
    let bytes = s.as_bytes();
    if s.len() >= 2
        && ((bytes[0] == b'"' && bytes[s.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[s.len() - 1] == b'\''))
    {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

/// The first Markdown heading (`# Title`) outside the front matter, if any.
pub fn first_heading(content: &str) -> Option<String> {
    let trimmed = content.trim_start_matches('\u{feff}');
    let mut in_fm = false;
    for (i, line) in trimmed.lines().enumerate() {
        let t = line.trim();
        if i == 0 && t == "---" {
            in_fm = true;
            continue;
        }
        if in_fm {
            if t == "---" {
                in_fm = false;
            }
            continue;
        }
        if let Some(rest) = t.strip_prefix('#') {
            let title = rest.trim_start_matches('#').trim();
            if !title.is_empty() {
                return Some(title.to_string());
            }
        }
    }
    None
}

/// Title precedence for items that are not named after themselves:
/// front matter `name:` → first `# heading` → the provided fallback.
pub fn title_from(content: &str, fallback: &str) -> String {
    let name = parse_meta(content).name;
    if !name.trim().is_empty() {
        return name;
    }
    if let Some(h) = first_heading(content) {
        return h;
    }
    fallback.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_prefers_frontmatter_then_heading_then_fallback() {
        assert_eq!(
            title_from("---\nname: From FM\n---\n# Heading\n", "file"),
            "From FM"
        );
        assert_eq!(title_from("# Heading Title\n\nbody", "file"), "Heading Title");
        assert_eq!(title_from("just body, no name", "file"), "file");
        // heading inside front matter is ignored
        assert_eq!(title_from("---\ndescription: x\n---\n# Real\n", "file"), "Real");
    }

    #[test]
    fn reads_name_and_description() {
        let c = "---\nname: babysit\ndescription: Watch a task and report changes\n---\n# Body\n";
        assert_eq!(
            parse_meta(c),
            Meta {
                name: "babysit".into(),
                description: "Watch a task and report changes".into()
            }
        );
    }

    #[test]
    fn strips_surrounding_quotes() {
        let c = "---\nname: \"a-b\"\ndescription: 'has, comma'\n---\n";
        assert_eq!(parse_meta(c).description, "has, comma");
        assert_eq!(parse_meta(c).name, "a-b");
    }

    #[test]
    fn no_frontmatter_is_empty() {
        assert_eq!(parse_meta("# Just a heading\n"), Meta::default());
    }
}
