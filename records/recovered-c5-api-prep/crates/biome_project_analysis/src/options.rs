use crate::{model::WorkspaceId, result::IssueKind};
use biome_diagnostics::Severity;
use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum AnalysisMode {
    #[default]
    Default,
    Production,
    Strict,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AnalysisOptions {
    pub compatibility_profile: String,
    pub mode: AnalysisMode,
    pub enabled_issues: BTreeSet<IssueKind>,
    /// None uses the defaults; an explicit empty set selects no categories.
    #[serde(default)]
    pub issue_selection: Option<BTreeSet<IssueKind>>,
    pub issue_severities: BTreeMap<IssueKind, Option<Severity>>,
    pub include_entry_exports: bool,
    pub ignore_local_uses: BTreeSet<String>,
    #[serde(default)]
    pub ignore_members: Vec<String>,
    pub included_tags: BTreeSet<String>,
    pub excluded_tags: BTreeSet<String>,
    pub ignore: Vec<String>,
    pub cycles: CycleOptions,
    #[serde(default)]
    pub workspaces: BTreeMap<WorkspaceId, WorkspacePolicy>,
    #[serde(default)]
    pub report_ignores: Vec<crate::policy::ReportIgnore>,
    #[serde(default)]
    pub hints: HintOptions,
    #[serde(default)]
    pub tag_mappings: Vec<crate::policy::TagInputMapping>,
    #[serde(default)]
    pub configuration_notices: Vec<crate::policy::ConfigurationNotice>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HintOptions {
    pub configuration: bool,
    pub tags: bool,
    pub configuration_errors: bool,
    pub tag_errors: bool,
}
impl Default for HintOptions {
    fn default() -> Self {
        Self {
            configuration: true,
            tags: true,
            configuration_errors: false,
            tag_errors: false,
        }
    }
}

/// Resolved workspace values replace their root fallback, including false and empty sets.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WorkspacePolicy {
    pub include_entry_exports: Option<bool>,
    pub ignore_local_uses: Option<BTreeSet<String>>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct CycleOptions {
    pub dynamic_imports: bool,
    pub allowed_paths: Vec<Vec<Utf8PathBuf>>,
}

impl Default for AnalysisOptions {
    fn default() -> Self {
        Self {
            compatibility_profile: "knip-6.43.0-native-core".into(),
            mode: AnalysisMode::Default,
            enabled_issues: BTreeSet::new(),
            issue_selection: None,
            issue_severities: BTreeMap::new(),
            include_entry_exports: false,
            ignore_local_uses: BTreeSet::new(),
            ignore_members: Vec::new(),
            included_tags: BTreeSet::new(),
            excluded_tags: BTreeSet::new(),
            ignore: Vec::new(),
            cycles: CycleOptions::default(),
            workspaces: BTreeMap::new(),
            report_ignores: Vec::new(),
            hints: HintOptions::default(),
            tag_mappings: Vec::new(),
            configuration_notices: Vec::new(),
        }
    }
}

impl AnalysisOptions {
    pub fn include_entry_exports_in(&self, workspace: WorkspaceId) -> bool {
        self.workspaces
            .get(&workspace)
            .and_then(|policy| policy.include_entry_exports)
            .unwrap_or(self.include_entry_exports)
    }

    pub fn local_uses_in(&self, workspace: WorkspaceId) -> &BTreeSet<String> {
        self.workspaces
            .get(&workspace)
            .and_then(|policy| policy.ignore_local_uses.as_ref())
            .unwrap_or(&self.ignore_local_uses)
    }

    pub fn allows_tags(&self, tags: &[String]) -> bool {
        !tags.iter().any(|tag| {
            matches!(tag.as_str(), "public" | "beta" | "alias")
                || (tag == "internal" && self.mode != AnalysisMode::Default)
        }) && self.allows_custom_tags(tags)
    }

    /// Imports use configured selectors only when an annotation is attached.
    pub(crate) fn allows_import_tags(&self, tags: &[String]) -> bool {
        tags.is_empty() || self.allows_custom_tags(tags)
    }

    fn allows_custom_tags(&self, tags: &[String]) -> bool {
        !tags.iter().any(|tag| self.excluded_tags.contains(tag))
            && (self.included_tags.is_empty()
                || tags.iter().any(|tag| self.included_tags.contains(tag)))
    }

    pub fn includes(&self, kind: IssueKind) -> bool {
        self.severity(kind).is_some()
    }

    pub fn severity(&self, kind: IssueKind) -> Option<Severity> {
        if self
            .issue_selection
            .as_ref()
            .is_some_and(|selected| !selected.contains(&kind))
        {
            return None;
        }
        if self.mode != AnalysisMode::Default
            && matches!(
                kind,
                IssueKind::DevDependencies | IssueKind::Catalog | IssueKind::CatalogReferences
            )
        {
            return None;
        }
        self.issue_severities
            .get(&kind)
            .copied()
            .unwrap_or_else(|| match kind {
                IssueKind::Cycles if self.selected_explicitly(kind) => Some(Severity::Warning),
                IssueKind::NsExports | IssueKind::NsTypes if self.selected_explicitly(kind) => {
                    Some(Severity::Error)
                }
                IssueKind::NsExports | IssueKind::NsTypes | IssueKind::Cycles => None,
                _ => Some(Severity::Error),
            })
    }

    fn selected_explicitly(&self, kind: IssueKind) -> bool {
        self.enabled_issues.contains(&kind)
            || self
                .issue_selection
                .as_ref()
                .is_some_and(|selection| selection.contains(&kind))
    }

    pub fn configuration_hints_enabled(&self) -> bool {
        self.mode == AnalysisMode::Default && self.hints.configuration
    }
}

/// Selection applies after analysis and cannot establish capability or completeness.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ReportSelection {
    pub workspaces: BTreeSet<WorkspaceId>,
    /// Treats an empty workspace set as selecting no workspaces.
    #[serde(default)]
    pub restrict_workspaces: bool,
    #[cfg_attr(feature = "schema", schemars(with = "Vec<String>"))]
    pub paths: Vec<Utf8PathBuf>,
    pub issues: BTreeSet<IssueKind>,
    /// Treats an empty issues set as selecting no categories.
    #[serde(default)]
    pub restrict_issues: bool,
    /// Exact changed paths intersect the other report predicates; an empty list selects none.
    #[serde(default)]
    #[cfg_attr(feature = "schema", schemars(with = "Option<BTreeSet<String>>"))]
    pub changed_paths: Option<BTreeSet<Utf8PathBuf>>,
}
