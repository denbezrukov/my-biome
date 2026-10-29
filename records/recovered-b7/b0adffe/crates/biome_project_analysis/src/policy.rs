//! Reporting policy retains its inputs and causes independently of selected findings.

use crate::{
    coverage::{Coverage, CoverageGap, GapReason, TargetDomain},
    model::{AnalysisSnapshot, Location, WorkspaceId},
    options::{AnalysisMode, AnalysisOptions},
    reachability::{hash, matches_patterns, owner},
    result::{
        AnalysisHint, Evidence, EvidenceId, HintKind, IssueKind, ProjectAnalysisResult,
        ProjectIssue,
    },
};
use biome_diagnostics::Severity;
use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct PolicyCauses {
    pub ignores: Vec<ReportIgnore>,
    #[serde(default)]
    pub used_ignores: BTreeSet<usize>,
    pub suppressions: Vec<ReportSuppression>,
    pub tag_mappings: Vec<TagInputMapping>,
    #[serde(default)]
    pub tag_suppressions: Vec<TagSuppression>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ConfigurationNotice {
    pub workspace: WorkspaceId,
    pub origin: PolicyOrigin,
    pub message: String,
    pub cause: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ReportIgnore {
    pub field: IgnoreField,
    pub pattern: String,
    pub base: Utf8PathBuf,
    pub workspace: Option<WorkspaceId>,
    pub issues: BTreeSet<IssueKind>,
    pub origin: PolicyOrigin,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct PolicyOrigin {
    pub location: Option<Location>,
    /// Explains an unavailable origin without inventing a source range.
    pub unavailable: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum IgnoreField {
    Ignore,
    IgnoreFiles,
    IgnoreDependencies,
    IgnoreBinaries,
    IgnoreUnresolved,
    IgnoreMembers,
    IgnoreIssues,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ReportSuppression {
    pub issue: String,
    pub policy: usize,
    pub evidence: EvidenceId,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TagSuppression {
    pub issue: String,
    pub tags: Vec<String>,
    pub evidence: EvidenceId,
}

/// Original compatibility input and its effective native tag name remain distinct.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TagInputMapping {
    pub original: String,
    pub effective: Option<String>,
    pub excluded: bool,
}

pub fn map_knip_tags(values: &[String]) -> Vec<TagInputMapping> {
    values
        .iter()
        .flat_map(|value| value.split(','))
        .map(|value| {
            let effective = value.find(|c: char| c.is_ascii_alphabetic()).map(|start| {
                value[start..]
                    .chars()
                    .take_while(char::is_ascii_alphabetic)
                    .collect()
            });
            TagInputMapping {
                original: value.into(),
                effective,
                excluded: value.starts_with('-'),
            }
        })
        .collect()
}

/// Combines per-key input precedence; explicit severity off remains a separate decision.
pub fn select_issues(
    mode: AnalysisMode,
    config_include: &[IssueKind],
    config_exclude: &[IssueKind],
    cli_include: &[IssueKind],
    cli_exclude: &[IssueKind],
    dependencies: bool,
    exports: bool,
) -> BTreeSet<IssueKind> {
    let mut overrides: BTreeSet<_> = cli_include.iter().copied().collect();
    if dependencies {
        overrides.extend([
            IssueKind::Dependencies,
            IssueKind::Unlisted,
            IssueKind::Binaries,
            IssueKind::Unresolved,
            IssueKind::Catalog,
            IssueKind::CatalogReferences,
        ]);
    }
    if exports {
        overrides.extend([
            IssueKind::Exports,
            IssueKind::Types,
            IssueKind::EnumMembers,
            IssueKind::NamespaceMembers,
            IssueKind::Duplicates,
        ]);
    }
    let mut include: BTreeSet<_> = config_include
        .iter()
        .filter(|kind| !cli_exclude.contains(kind))
        .chain(&overrides)
        .copied()
        .collect();
    let mut exclude: BTreeSet<_> = config_exclude
        .iter()
        .filter(|kind| !overrides.contains(kind))
        .chain(cli_exclude)
        .copied()
        .collect();
    if mode == AnalysisMode::Default {
        if include.contains(&IssueKind::Dependencies) {
            include.extend([
                IssueKind::DevDependencies,
                IssueKind::OptionalPeerDependencies,
            ]);
        }
        if exclude.contains(&IssueKind::Dependencies) {
            exclude.extend([
                IssueKind::DevDependencies,
                IssueKind::OptionalPeerDependencies,
            ]);
        }
    } else {
        exclude.extend([
            IssueKind::DevDependencies,
            IssueKind::Catalog,
            IssueKind::CatalogReferences,
        ]);
    }
    if include
        .iter()
        .all(|kind| matches!(kind, IssueKind::NsExports | IssueKind::NsTypes))
    {
        include.extend(IssueKind::ALL.into_iter().filter(|kind| {
            !matches!(
                kind,
                IssueKind::NsExports | IssueKind::NsTypes | IssueKind::Cycles
            )
        }));
    }
    include.retain(|kind| !exclude.contains(kind));
    include
}

pub(crate) fn collect_policy_causes(
    snapshot: &AnalysisSnapshot,
    result: &mut ProjectAnalysisResult,
    options: &AnalysisOptions,
) {
    let unknown = PolicyOrigin {
        location: None,
        unavailable: Some(
            "The caller supplied effective settings without source provenance".into(),
        ),
    };
    result.policy.tag_mappings.clone_from(&options.tag_mappings);
    for notice in &options.configuration_notices {
        add_hint(
            result,
            HintKind::Configuration,
            notice.origin.location,
            notice.workspace,
            notice.message.clone(),
            format!(
                "{}. {}",
                notice.cause,
                notice
                    .origin
                    .unavailable
                    .as_deref()
                    .unwrap_or("Origin captured from configuration")
            ),
        );
    }
    result.policy.ignores.clone_from(&options.report_ignores);
    for pattern in &options.ignore {
        result.policy.ignores.push(ReportIgnore {
            field: IgnoreField::Ignore,
            pattern: pattern.clone(),
            base: ".".into(),
            workspace: None,
            issues: BTreeSet::new(),
            origin: unknown.clone(),
        });
    }
    for policy in &snapshot.inventory.reporting_policies {
        for (field, patterns) in [
            (IgnoreField::Ignore, &policy.ignore),
            (IgnoreField::IgnoreFiles, &policy.ignore_files),
        ] {
            for pattern in patterns {
                if !result.policy.ignores.iter().any(|existing| {
                    existing.field == field
                        && existing.pattern == *pattern
                        && existing.base == policy.base
                        && existing.workspace == policy.workspace
                }) {
                    result.policy.ignores.push(ReportIgnore {
                        field,
                        pattern: pattern.clone(),
                        base: policy.base.clone(),
                        workspace: policy.workspace,
                        issues: BTreeSet::new(),
                        origin: unknown.clone(),
                    });
                }
            }
        }
    }
    for binary in &snapshot.inventory.script_usage.binaries {
        if binary.mode != options.mode
            || binary.optional
            || !binary
                .report_suppression
                .contains(&crate::scripts::BinaryReportSuppression::ExplicitIgnore)
            || !result.membership.script_producers.iter().any(|producer| {
                producer.producer == binary.producer
                    && producer.activation == crate::scripts::Activation::Definite
            })
        {
            continue;
        }
        for (index, policy) in result.policy.ignores.iter().enumerate() {
            if policy.field == IgnoreField::IgnoreBinaries
                && policy
                    .workspace
                    .is_none_or(|workspace| workspace == binary.workspace)
                && effective_pattern(policy, options.mode) == Some(binary.binary.as_str())
            {
                result.policy.used_ignores.insert(index);
                let id = EvidenceId(format!(
                    "binary-policy:{}",
                    hash(&(
                        &binary.binary,
                        &policy.pattern,
                        &policy.base,
                        result
                            .sources
                            .get(&binary.location.file)
                            .map(|source| &source.logical_path),
                        binary.location.range
                    ))
                ));
                result.evidence.nodes.insert(id, Evidence {producer:"binary reporting policy".into(),location:Some(binary.location),workspace:binary.workspace,explanation:format!("Literal ignore {} was consumed by an active command; provider facts and tool uncertainty remain retained",policy.pattern),related:Vec::new()});
            }
        }
    }
    result.issues.retain(|issue| {
        if !matches!(issue.kind, IssueKind::Unlisted | IssueKind::Unresolved) { return true; }
        let tags: Vec<_> = snapshot.modules.get(&issue.location.file).into_iter().flat_map(|module| &module.tags).filter(|tag| tag.target.is_some_and(|target| target.contains_range(issue.location.range))).map(|tag| tag.name.clone()).collect();
        if options.allows_tags(&tags) { return true; }
        let id = EvidenceId(format!("tag-suppression:{}", hash(&(&issue.identity, &tags))));
        result.evidence.nodes.insert(id.clone(), Evidence { producer: "import tag reporting policy".into(), location: Some(issue.location), workspace: issue.workspace, explanation: format!("Tags {tags:?} suppress reporting; original attribution and resolution remain available"), related: issue.related_evidence.clone() });
        result.policy.tag_suppressions.push(TagSuppression { issue: issue.identity.clone(), tags, evidence: id });
        false
    });
    for (file, module) in &snapshot.modules {
        if !result.membership.files.iter().any(|member| {
            member.file == *file && member.analyzed && member.definite && !member.possible
        }) {
            continue;
        }
        let Some(module) = &module.collected else {
            continue;
        };
        for declaration in &module.declarations {
            if !module.exports.iter().any(|export| matches!(export.target,crate::facts::ExportTarget::Local(id) if id.location==declaration.id.location)) || protected(result,module,declaration.id.location) || result.members.uncertainty.iter().any(|uncertainty|uncertainty.parent.location==declaration.id.location) {continue;}
            let tags = crate::exports::declaration_tags(module, declaration);
            if tags
                .iter()
                .any(|tag| matches!(tag.as_str(), "public" | "beta" | "alias"))
            {
                continue;
            }
            let used = result
                .exports
                .consumed
                .iter()
                .chain(&result.members.used)
                .any(|id| id.location == declaration.id.location);
            if !used {
                continue;
            }
            let useful_members = result
                .members
                .tag_retentions
                .iter()
                .any(|id| id.location == declaration.id.location);
            if useful_members {
                continue;
            }
            for tag in &tags {
                if !options.excluded_tags.contains(tag)
                    && !(tag == "internal" && options.mode != AnalysisMode::Default)
                {
                    continue;
                }
                add_hint(
                    result,
                    HintKind::Tag,
                    tag_location(snapshot, declaration.id.location, tag).into(),
                    owner(snapshot, *file),
                    format!("Unused tag on {}: @{tag}", declaration.name),
                    format!(
                        "Declaration {} is consumed and this tag protects no unused member",
                        declaration.name
                    ),
                );
            }
        }
        for member in &module.members {
            if !result
                .members
                .used
                .iter()
                .any(|id| id.location == member.declaration.location)
                || protected(result, module, member.parent.location)
                || member
                    .tags
                    .iter()
                    .any(|tag| matches!(tag.as_str(), "public" | "beta" | "alias"))
            {
                continue;
            }
            let parent = module
                .declarations
                .iter()
                .find(|declaration| declaration.id.location == member.parent.location)
                .map_or("", |declaration| declaration.name.as_str());
            for tag in &member.tags {
                if options.excluded_tags.contains(tag)
                    || (tag == "internal" && options.mode != AnalysisMode::Default)
                {
                    add_hint(
                        result,
                        HintKind::Tag,
                        Some(tag_location(snapshot, member.declaration.location, tag)),
                        owner(snapshot, *file),
                        format!("Unused tag on {parent}.{}: @{tag}", member.path.join(".")),
                        "The member has a definite use, so its tag no longer suppresses a finding"
                            .into(),
                    );
                }
            }
        }
    }
}

fn protected(
    result: &ProjectAnalysisResult,
    module: &crate::facts::CollectedModule,
    location: Location,
) -> bool {
    module.export_surfaces.iter().flat_map(|surface|&surface.slots).any(|slot| matches!(slot.resolution,crate::facts::ExportResolution::Found(id) if id.location==location) && slot.slots.iter().any(|key|result.exports.protected_slots.contains(key)))
}

fn tag_location(snapshot: &AnalysisSnapshot, declaration: Location, name: &str) -> Location {
    snapshot
        .modules
        .get(&declaration.file)
        .into_iter()
        .flat_map(|module| &module.tags)
        .filter(|tag| {
            tag.name == name
                && tag
                    .target
                    .is_some_and(|range| range.contains_range(declaration.range))
        })
        .min_by_key(|tag| tag.target.unwrap().len())
        .map_or(declaration, |tag| tag.location)
}

fn add_hint(
    result: &mut ProjectAnalysisResult,
    kind: HintKind,
    location: Option<Location>,
    workspace: WorkspaceId,
    message: String,
    cause: String,
) {
    let identity = location.and_then(|location| {
        result
            .sources
            .get(&location.file)
            .map(|source| (source.logical_path.clone(), location.range))
    });
    let id = EvidenceId(format!(
        "policy-hint:{}",
        hash(&(
            &identity,
            result
                .workspaces
                .get(&workspace)
                .map(|workspace| &workspace.path),
            &message,
            &cause
        ))
    ));
    result.evidence.nodes.insert(
        id.clone(),
        Evidence {
            producer: "analysis policy".into(),
            location,
            workspace,
            explanation: cause.clone(),
            related: Vec::new(),
        },
    );
    let hint = AnalysisHint {
        kind,
        severity: Severity::Warning,
        cause,
        message,
        location,
        workspace,
        related_evidence: vec![id],
    };
    if !result.hints.iter().any(|existing| {
        existing.kind == hint.kind
            && existing.workspace == hint.workspace
            && existing.location == hint.location
            && existing.message == hint.message
            && existing.cause == hint.cause
    }) {
        result.hints.push(hint);
    }
}

fn effective_pattern(policy: &ReportIgnore, mode: AnalysisMode) -> Option<&str> {
    if matches!(
        policy.field,
        IgnoreField::IgnoreDependencies | IgnoreField::IgnoreBinaries
    ) && policy.pattern.ends_with('!')
    {
        return (mode != AnalysisMode::Default)
            .then(|| &policy.pattern[..policy.pattern.len() - 1]);
    }
    Some(&policy.pattern)
}

fn symbol_pattern(policy: &ReportIgnore) -> bool {
    matches!(
        policy.field,
        IgnoreField::IgnoreDependencies
            | IgnoreField::IgnoreBinaries
            | IgnoreField::IgnoreUnresolved
    )
}
fn regex_pattern(pattern: &str) -> bool {
    pattern.contains(['*', '+', '\\', '(', '|', '{', '^', '$'])
}
fn negative(policy: &ReportIgnore, pattern: &str) -> bool {
    !symbol_pattern(policy) && pattern.starts_with('!')
}
fn pattern_error(policy: &ReportIgnore, pattern: &str) -> Option<String> {
    if symbol_pattern(policy) {
        if regex_pattern(pattern) {
            regex::Regex::new(pattern)
                .err()
                .map(|error| error.to_string())
        } else {
            None
        }
    } else {
        pattern
            .parse::<biome_glob::NormalizedGlob>()
            .err()
            .map(|error| error.to_string())
    }
}

fn matches_ignore(
    policy: &ReportIgnore,
    issue: &ProjectIssue,
    result: &ProjectAnalysisResult,
    pattern: &str,
) -> bool {
    if policy
        .workspace
        .is_some_and(|workspace| workspace != issue.workspace)
    {
        return false;
    }
    let Some(source) = result.sources.get(&issue.location.file) else {
        return false;
    };
    let relative = if policy.base.as_str() == "." || policy.base.as_str().is_empty() {
        source.logical_path.as_path()
    } else if let Ok(path) = source.logical_path.strip_prefix(&policy.base) {
        path
    } else {
        return false;
    };
    let target = match policy.field {
        IgnoreField::Ignore => relative,
        IgnoreField::IgnoreFiles if issue.kind == IssueKind::Files => relative,
        IgnoreField::IgnoreIssues if policy.issues.contains(&issue.kind) => relative,
        IgnoreField::IgnoreDependencies
            if matches!(
                issue.kind,
                IssueKind::Dependencies
                    | IssueKind::DevDependencies
                    | IssueKind::OptionalPeerDependencies
                    | IssueKind::Unlisted
            ) =>
        {
            Utf8Path::new(issue.symbol.as_deref().unwrap_or(""))
        }
        IgnoreField::IgnoreBinaries if issue.kind == IssueKind::Binaries => {
            Utf8Path::new(issue.symbol.as_deref().unwrap_or(""))
        }
        IgnoreField::IgnoreUnresolved if issue.kind == IssueKind::Unresolved => {
            Utf8Path::new(issue.symbol.as_deref().unwrap_or(""))
        }
        IgnoreField::IgnoreMembers
            if matches!(
                issue.kind,
                IssueKind::EnumMembers | IssueKind::NamespaceMembers
            ) =>
        {
            Utf8Path::new(
                issue
                    .symbol
                    .as_deref()
                    .unwrap_or("")
                    .split_once('.')
                    .map_or("", |(_, tail)| tail),
            )
        }
        _ => return false,
    };
    let pattern = if negative(policy, pattern) {
        &pattern[1..]
    } else {
        pattern
    };
    if symbol_pattern(policy) {
        return if regex_pattern(pattern) {
            regex::Regex::new(pattern).map_or(true, |regex| regex.is_match(target.as_str()))
        } else {
            target.as_str() == pattern
        };
    }
    matches_patterns(&[pattern.into()], target)
}

/// Suppresses presentation only, retaining candidates, membership and every uncertainty cause.
pub fn apply_policy(
    mut result: ProjectAnalysisResult,
    options: &AnalysisOptions,
) -> ProjectAnalysisResult {
    let policies = result.policy.ignores.clone();
    let mut used = result.policy.used_ignores.clone();
    used.extend(
        result
            .policy
            .suppressions
            .iter()
            .map(|suppression| suppression.policy),
    );
    let mut issues = Vec::new();
    for issue in std::mem::take(&mut result.issues) {
        let mut suppressed = false;
        for (index, policy) in policies.iter().enumerate() {
            let Some(pattern) = effective_pattern(policy, options.mode) else {
                continue;
            };
            if negative(policy, pattern) || !matches_ignore(policy, &issue, &result, pattern) {
                continue;
            }
            if policies.iter().any(|negative| {
                negative.field == policy.field
                    && negative.workspace == policy.workspace
                    && negative.base == policy.base
                    && effective_pattern(negative, options.mode).is_some_and(|pattern| {
                        self::negative(negative, pattern)
                            && matches_ignore(negative, &issue, &result, pattern)
                    })
            }) {
                continue;
            }
            used.insert(index);
            suppressed = true;
            let id = EvidenceId(format!(
                "policy-suppression:{}",
                hash(&(&issue.identity, index))
            ));
            result.evidence.nodes.insert(
                id.clone(),
                Evidence {
                    producer: "report suppression".into(),
                    location: policy.origin.location,
                    workspace: issue.workspace,
                    explanation: format!(
                        "{:?} {} suppresses {}",
                        policy.field, policy.pattern, issue.identity
                    ),
                    related: issue.related_evidence.clone(),
                },
            );
            result.policy.suppressions.push(ReportSuppression {
                issue: issue.identity.clone(),
                policy: index,
                evidence: id,
            });
        }
        if !suppressed {
            issues.push(issue);
        }
    }
    result.issues = issues;
    for (index, policy) in policies.iter().enumerate() {
        let Some(pattern) = effective_pattern(policy, options.mode) else {
            continue;
        };
        if let Some(error) = pattern_error(policy, pattern) {
            let gap = CoverageGap {
                producer: "analysis policy".into(),
                domain: TargetDomain::Unbounded,
                reason: GapReason::Capability(format!(
                    "Unsupported {:?} pattern {pattern}: {error}",
                    policy.field
                )),
                location: policy.origin.location,
            };
            let mut gaps = result.coverage.gaps().to_vec();
            if !gaps.contains(&gap) {
                gaps.push(gap);
            }
            result.coverage = if matches!(result.coverage, Coverage::Failed(_)) {
                Coverage::Failed(gaps)
            } else {
                Coverage::Incomplete(gaps)
            };
            continue;
        }
        if used.contains(&index)
            || negative(policy, pattern)
            || policy.field == IgnoreField::IgnoreIssues
            || matches!(policy.field, IgnoreField::Ignore | IgnoreField::IgnoreFiles)
                && !options.includes(IssueKind::Files)
        {
            continue;
        }
        let workspace = policy.workspace.unwrap_or_else(|| {
            result
                .workspaces
                .values()
                .find(|workspace| workspace.path == ".")
                .map_or(WorkspaceId(0), |workspace| workspace.workspace)
        });
        add_hint(
            &mut result,
            HintKind::Configuration,
            policy.origin.location,
            workspace,
            format!(
                "Remove unused {:?} pattern {}",
                policy.field, policy.pattern
            ),
            format!(
                "No candidate finding was suppressed by this pattern. {}",
                policy
                    .origin
                    .unavailable
                    .as_deref()
                    .unwrap_or("Origin captured from configuration")
            ),
        );
    }
    result.hints.retain_mut(|hint| {
        let (enabled, error) = match hint.kind {
            HintKind::Configuration => (
                options.configuration_hints_enabled(),
                options.hints.configuration_errors,
            ),
            HintKind::Tag => (options.hints.tags, options.hints.tag_errors),
        };
        hint.severity = if error {
            Severity::Error
        } else {
            Severity::Warning
        };
        enabled
    });
    result
}
