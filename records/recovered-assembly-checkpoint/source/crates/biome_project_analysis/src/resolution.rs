//! Source failures use final resolver outcomes and package attribution decisions.
use crate::{
    dependencies::DependencyAnalysis,
    facts::{ResolutionOutcome, ResolutionPurpose},
    model::AnalysisSnapshot,
    options::{AnalysisMode, AnalysisOptions},
    reachability::{AnalysisMembership, owner, path},
    result::{Evidence, EvidenceGraph, EvidenceId, IssueKind, ProjectIssue},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ResolutionAnalysis {
    pub issues: Vec<ProjectIssue>,
    pub evidence: EvidenceGraph,
    /// Repeated raw specifiers report once per file while retaining every failed occurrence.
    pub requests: Vec<FailedRequest>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FailedRequest {
    pub location: crate::model::Location,
    pub specifier: String,
    pub occurrence: usize,
    pub resolutions: Vec<crate::facts::ResolvedOccurrence>,
    pub package_category: Option<crate::dependencies::RequestCategory>,
}
pub fn analyze_unresolved(
    snapshot: &AnalysisSnapshot,
    membership: &AnalysisMembership,
    dependencies: &DependencyAnalysis,
    options: &AnalysisOptions,
) -> ResolutionAnalysis {
    let mut result = ResolutionAnalysis::default();
    for member in membership.files.iter().filter(|m| m.definite && m.analyzed) {
        let Some(module) = snapshot
            .modules
            .get(&member.file)
            .and_then(|m| m.collected.as_ref())
        else {
            continue;
        };
        let mut reported = BTreeSet::new();
        for import in &module.imports {
            if import.possible
                || import.optional
                || options.mode == AnalysisMode::Strict && import.type_only
            {
                continue;
            }
            let purpose = if import.type_only || module.declaration_file.is_some() {
                ResolutionPurpose::Declaration
            } else {
                ResolutionPurpose::Runtime
            };
            let selected: Vec<_> = module
                .resolutions
                .iter()
                .filter(|r| r.occurrence == import.ordinal && r.context.purpose == purpose)
                .collect();
            if selected.is_empty()
                || !selected
                    .iter()
                    .all(|r| matches!(r.outcome, ResolutionOutcome::Unresolved { .. }))
            {
                continue;
            }
            let package_category = dependencies
                .decisions
                .iter()
                .find(|d| {
                    d.location == import.specifier_location && d.specifier == import.specifier
                })
                .map(|d| d.category);
            result.requests.push(FailedRequest {
                location: import.specifier_location,
                specifier: import.specifier.clone(),
                occurrence: import.ordinal,
                resolutions: selected.iter().copied().cloned().collect(),
                package_category,
            });
            let id = EvidenceId(format!(
                "unresolved:{}:{}",
                path(snapshot, member.file),
                import.ordinal
            ));
            result.evidence.nodes.insert(
                id.clone(),
                Evidence {
                    producer: "module resolution".into(),
                    location: Some(import.location),
                    workspace: owner(snapshot, member.file),
                    explanation: format!(
                        "Failed literal {}; occurrence {}",
                        import.specifier, import.ordinal
                    ),
                    related: Vec::new(),
                },
            );
            if package_category.is_some()
                || excluded(&import.specifier)
                || !member.analyzed
                || !reported.insert(import.specifier.as_str())
            {
                continue;
            }
            if let Some(severity) = options.severity(IssueKind::Unresolved) {
                result.issues.push(ProjectIssue {
                    kind: IssueKind::Unresolved,
                    severity,
                    identity: format!(
                        "unresolved:{}:{}:{}",
                        path(snapshot, member.file),
                        import.specifier,
                        import.ordinal
                    ),
                    symbol: Some(import.specifier.clone()),
                    location: import.specifier_location,
                    workspace: owner(snapshot, member.file),
                    related_evidence: vec![id],
                    eligible_fixes: Vec::new(),
                });
            }
        }
    }
    result
}
fn excluded(specifier: &str) -> bool {
    specifier.starts_with("http")
        || specifier.starts_with("virtual:")
        || matches!(
            camino::Utf8Path::new(crate::dependencies::sanitize_specifier(specifier)).extension(),
            Some(
                "json"
                    | "avif"
                    | "css"
                    | "eot"
                    | "gif"
                    | "html"
                    | "ico"
                    | "jpeg"
                    | "jpg"
                    | "less"
                    | "mp3"
                    | "otf"
                    | "png"
                    | "sass"
                    | "scss"
                    | "sh"
                    | "styl"
                    | "stylus"
                    | "svg"
                    | "ttf"
                    | "webp"
                    | "woff"
                    | "woff2"
                    | "yaml"
                    | "yml"
            )
        )
}
