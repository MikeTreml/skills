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

#[cfg(test)]
mod tests {
    use super::*;

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
