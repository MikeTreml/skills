use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ItemType {
    Skill,
    Agent,
}

impl ItemType {
    pub fn as_str(self) -> &'static str {
        match self {
            ItemType::Skill => "skill",
            ItemType::Agent => "agent",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "skill" => Some(ItemType::Skill),
            "agent" => Some(ItemType::Agent),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LocationKind {
    ClaudeSkills,
    Marketplace,
    Agents,
    Project,
    Codex,
    Tarball,
}

impl LocationKind {
    pub fn as_str(self) -> &'static str {
        match self {
            LocationKind::ClaudeSkills => "claude-skills",
            LocationKind::Marketplace => "marketplace",
            LocationKind::Agents => "agents",
            LocationKind::Project => "project",
            LocationKind::Codex => "codex",
            LocationKind::Tarball => "tarball",
        }
    }
    /// What the scanner looks for: agents = top-level `*.md`, everything else = `**/SKILL.md`.
    pub fn scans_agents(self) -> bool {
        matches!(self, LocationKind::Agents)
    }
}

/// A skill/agent discovered on disk, before it is stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScannedItem {
    pub item_type: ItemType,
    pub name: String,
    pub description: String,
    pub source_path: std::path::PathBuf, // the item's folder (skill) or file (agent)
    pub hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Item {
    pub id: i64,
    pub item_type: ItemType,
    pub name: String,
    pub slug: String,
    pub description: String,
    pub category: Option<String>,
    pub subcategory: Option<String>,
    pub canonical_hash: String,
    pub library_path: String,
    pub has_variants: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Location {
    pub id: i64,
    pub label: String,
    pub root_path: String,
    pub kind: LocationKind,
    pub enabled: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ImportSummary {
    pub locations_scanned: u32,
    pub items_found: u32,
    pub items_new: u32,
    pub placements_recorded: u32,
    pub variants_flagged: u32,
}
