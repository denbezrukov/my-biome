use crate::{model::WorkspaceId, result::IssueKind};
use biome_diagnostics::Severity;
use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
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
    pub issue_severities: BTreeMap<IssueKind, Option<Severity>>,
    pub include_entry_exports: bool,
    pub ignore_local_uses: BTreeSet<String>,
    pub included_tags: BTreeSet<String>,
    pub excluded_tags: BTreeSet<String>,
    pub ignore: Vec<String>,
    pub cycles: CycleOptions,
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
            issue_severities: BTreeMap::new(),
            include_entry_exports: false,
            ignore_local_uses: BTreeSet::new(),
            included_tags: BTreeSet::new(),
            excluded_tags: BTreeSet::new(),
            ignore: Vec::new(),
            cycles: CycleOptions::default(),
        }
    }
}

impl AnalysisOptions {
    pub fn includes(&self, kind: IssueKind) -> bool {
        self.severity(kind).is_some()
    }

    pub fn severity(&self, kind: IssueKind) -> Option<Severity> {
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
                IssueKind::Cycles if self.enabled_issues.contains(&kind) => Some(Severity::Warning),
                IssueKind::NsExports | IssueKind::NsTypes
                    if self.enabled_issues.contains(&kind) =>
                {
                    Some(Severity::Error)
                }
                IssueKind::NsExports | IssueKind::NsTypes | IssueKind::Cycles => None,
                _ => Some(Severity::Error),
            })
    }

    pub fn configuration_hints_enabled(&self) -> bool {
        self.mode == AnalysisMode::Default
    }
}

/// Selection applies after analysis and cannot establish capability or completeness.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ReportSelection {
    pub workspaces: BTreeSet<WorkspaceId>,
    pub paths: Vec<Utf8PathBuf>,
    pub issues: BTreeSet<IssueKind>,
}
