//! Duplicate / similar detection over classified items.
//! Exact = same (object, sub_object, verb). Near = same object, differing verbs.

use crate::model::Item;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, serde::Serialize)]
pub struct DupGroup {
    pub key: String,
    pub kind: &'static str, // "exact" | "near"
    pub item_ids: Vec<i64>,
}

pub fn group_duplicates(items: &[Item]) -> Vec<DupGroup> {
    let mut exact: BTreeMap<(String, String, String), Vec<i64>> = BTreeMap::new();
    let mut by_object: BTreeMap<String, Vec<i64>> = BTreeMap::new();
    let mut verbs_by_object: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    for it in items {
        let (object, verb) = match (it.object.as_deref(), it.verb.as_deref()) {
            (Some(o), Some(v)) if !o.is_empty() && !v.is_empty() => (o.to_string(), v.to_string()),
            _ => continue,
        };
        let sub = it.sub_object.clone().unwrap_or_default();
        exact
            .entry((object.clone(), sub, verb.clone()))
            .or_default()
            .push(it.id);
        by_object.entry(object.clone()).or_default().push(it.id);
        verbs_by_object.entry(object).or_default().insert(verb);
    }

    let mut groups = Vec::new();
    for ((object, sub, verb), ids) in &exact {
        if ids.len() > 1 {
            let key = if sub.is_empty() {
                format!("{object} — {verb}")
            } else {
                format!("{object} › {sub} — {verb}")
            };
            groups.push(DupGroup { key, kind: "exact", item_ids: ids.clone() });
        }
    }
    for (object, ids) in &by_object {
        // Near: the same object covered by more than one distinct verb (overlapping surface).
        if ids.len() > 1 && verbs_by_object.get(object).map_or(0, BTreeSet::len) > 1 {
            groups.push(DupGroup { key: object.clone(), kind: "near", item_ids: ids.clone() });
        }
    }
    groups
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ItemType;

    fn item(id: i64, object: &str, sub: &str, verb: &str) -> Item {
        Item {
            id,
            item_type: ItemType::Skill,
            name: format!("item{id}"),
            slug: format!("item{id}"),
            description: String::new(),
            category: None,
            subcategory: None,
            object: Some(object.into()),
            sub_object: if sub.is_empty() { None } else { Some(sub.into()) },
            verb: Some(verb.into()),
            qualifier: None,
            canonical_hash: "h".into(),
            library_path: "p".into(),
            has_variants: false,
            archived: false,
        }
    }

    #[test]
    fn same_object_sub_verb_is_exact() {
        let g = group_duplicates(&[item(1, "Ax", "Form", "Create"), item(2, "Ax", "Form", "Create")]);
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].kind, "exact");
        assert_eq!(g[0].item_ids, vec![1, 2]);
    }

    #[test]
    fn same_object_different_verb_is_near() {
        let g = group_duplicates(&[item(1, "Ax", "Form", "Create"), item(2, "Ax", "Form", "Review")]);
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].kind, "near");
        assert_eq!(g[0].item_ids, vec![1, 2]);
    }

    #[test]
    fn different_objects_no_group() {
        let g = group_duplicates(&[item(1, "Ax", "Form", "Create"), item(2, "Twilio", "", "Configure")]);
        assert!(g.is_empty());
    }

    #[test]
    fn unclassified_items_are_ignored() {
        let mut a = item(1, "Ax", "Form", "Create");
        a.object = None;
        let g = group_duplicates(&[a, item(2, "Ax", "Form", "Create")]);
        assert!(g.is_empty());
    }
}
