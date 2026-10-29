use super::ProjectAnalysisRequest;
use biome_configuration::project_analysis::{
    LocalExportUses, ProjectIssueKind, ProjectIssueSeverity, WorkspaceAnalysisConfiguration,
};
use biome_diagnostics::Severity;
use biome_project_analysis::{
    inventory::Inventory,
    options::{AnalysisOptions, HintOptions, WorkspacePolicy},
    policy::{IgnoreField, PolicyOrigin, ReportIgnore, select_issues},
    result::IssueKind,
};
use std::collections::BTreeSet;

/// Optional caller selections are resolved against configuration per category.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", default)]
pub struct PolicyOverrides {
    pub include: Vec<IssueKind>,
    pub exclude: Vec<IssueKind>,
    pub dependencies: bool,
    pub exports: bool,
    pub include_entry_exports: bool,
    pub tags: Option<Vec<String>>,
    pub configuration_hints: Option<bool>,
    pub tag_hints: Option<bool>,
    pub configuration_hint_errors: Option<bool>,
    pub tag_hint_errors: Option<bool>,
}

impl ProjectAnalysisRequest {
    /// Translates captured static configuration into engine policy before semantic analysis.
    pub fn analysis_options(&self, inventory: &Inventory) -> AnalysisOptions {
        self.analysis_options_with_overrides(inventory, &PolicyOverrides::default())
    }

    pub fn analysis_options_with_overrides(
        &self,
        inventory: &Inventory,
        overrides: &PolicyOverrides,
    ) -> AnalysisOptions {
        let config = &self.configuration;
        let mut options = AnalysisOptions {
            mode: self.mode,
            include_entry_exports: config
                .include_entry_exports
                .unwrap_or(overrides.include_entry_exports),
            ignore_local_uses: local_uses(config.ignore_exports_used_in_file.as_ref())
                .unwrap_or_default(),
            hints: HintOptions {
                configuration: overrides.configuration_hints.unwrap_or(true),
                tags: overrides.tag_hints.unwrap_or(true),
                configuration_errors: overrides
                    .configuration_hint_errors
                    .or(config.treat_config_hints_as_errors)
                    .unwrap_or(false),
                tag_errors: overrides
                    .tag_hint_errors
                    .or(config.treat_tag_hints_as_errors)
                    .unwrap_or(false),
            },
            ..Default::default()
        };
        let include: Vec<_> = config
            .include
            .iter()
            .flatten()
            .copied()
            .map(issue_kind)
            .collect();
        let exclude: Vec<_> = config
            .exclude
            .iter()
            .flatten()
            .copied()
            .map(issue_kind)
            .collect();
        options.issue_selection = Some(select_issues(
            self.mode,
            &include,
            &exclude,
            &overrides.include,
            &overrides.exclude,
            overrides.dependencies,
            overrides.exports,
        ));
        for (&kind, severity) in config.rules.iter().flatten() {
            options.issue_severities.insert(
                issue_kind(kind),
                match severity {
                    ProjectIssueSeverity::Error => Some(Severity::Error),
                    ProjectIssueSeverity::Warn => Some(Severity::Warning),
                    ProjectIssueSeverity::Off => None,
                },
            );
        }
        for value in overrides
            .tags
            .as_ref()
            .or_else(|| config.tags.as_ref().map(|tags| &tags.0))
            .into_iter()
            .flatten()
        {
            for tag in value.split(',') {
                let excluded = tag.starts_with('-');
                let tag = tag.trim_start_matches(['+', '-']).trim_start_matches('@');
                if !tag.is_empty() {
                    if excluded {
                        options.excluded_tags.insert(tag.into());
                    } else {
                        options.included_tags.insert(tag.into());
                    }
                }
            }
        }
        if let Some(cycles) = &config.cycles {
            options.cycles.dynamic_imports = cycles.dynamic_imports.unwrap_or(false);
            options.cycles.allowed_paths = cycles
                .allow
                .iter()
                .flatten()
                .map(|cycle| cycle.iter().map(Into::into).collect())
                .collect();
        }
        let root = serde_json::to_value(config).expect("static configuration");
        append_ignores(&mut options, inventory, &root, ".", None, "");
        for workspace in &inventory.workspaces {
            let settings: WorkspaceAnalysisConfiguration =
                serde_json::from_str(&workspace.configuration_json).unwrap_or_default();
            options.workspaces.insert(
                workspace.workspace,
                WorkspacePolicy {
                    include_entry_exports: settings.include_entry_exports,
                    ignore_local_uses: local_uses(settings.ignore_exports_used_in_file.as_ref()),
                },
            );
            let value = serde_json::to_value(settings).expect("workspace configuration");
            let key = selected_workspace_key(config, &workspace.path);
            let pointer = key
                .map(|key| format!("/workspaces/{}", escape(key)))
                .unwrap_or_default();
            append_ignores(
                &mut options,
                inventory,
                &value,
                workspace.path.as_str(),
                Some(workspace.workspace),
                &pointer,
            );
        }
        append_discovery_notices(self, inventory, &mut options);
        options
    }
}

fn local_uses(value: Option<&LocalExportUses>) -> Option<BTreeSet<String>> {
    value.map(|value| match value {
        LocalExportUses::All(true) => ["*".into()].into(),
        LocalExportUses::All(false) => BTreeSet::new(),
        LocalExportUses::Kinds(kinds) => kinds
            .iter()
            .filter(|(_, enabled)| **enabled)
            .map(|(kind, _)| serde_json::to_value(kind).unwrap().as_str().unwrap().into())
            .collect(),
    })
}

fn issue_kind(kind: ProjectIssueKind) -> IssueKind {
    serde_json::from_value(serde_json::to_value(kind).expect("configuration category"))
        .expect("engine category")
}

fn escape(value: &str) -> String {
    value.replace('~', "~0").replace('/', "~1")
}

fn append_ignores(
    options: &mut AnalysisOptions,
    inventory: &Inventory,
    value: &serde_json::Value,
    base: &str,
    workspace: Option<biome_project_analysis::model::WorkspaceId>,
    pointer: &str,
) {
    for (key, field) in [
        ("ignore", IgnoreField::Ignore),
        ("ignoreFiles", IgnoreField::IgnoreFiles),
        ("ignoreDependencies", IgnoreField::IgnoreDependencies),
        ("ignoreBinaries", IgnoreField::IgnoreBinaries),
        ("ignoreUnresolved", IgnoreField::IgnoreUnresolved),
        ("ignoreMembers", IgnoreField::IgnoreMembers),
    ] {
        if field == IgnoreField::IgnoreMembers && workspace.is_none() {
            continue;
        }
        for (index, pattern) in value[key].as_array().into_iter().flatten().enumerate() {
            let Some(pattern) = pattern.as_str() else {
                continue;
            };
            let origin_key = format!("{pointer}/{key}/{index}");
            let origin = inventory
                .configuration_origins
                .get(&origin_key)
                .or_else(|| {
                    inventory
                        .configuration_origins
                        .get(&format!("{pointer}/{key}"))
                })
                .cloned()
                .unwrap_or_else(|| PolicyOrigin {
                    location: None,
                    unavailable: Some(format!("Source provenance unavailable for {origin_key}")),
                });
            options.report_ignores.push(ReportIgnore {
                field,
                pattern: pattern.into(),
                base: base.into(),
                workspace,
                issues: BTreeSet::new(),
                origin,
            });
        }
    }
    for (pattern, kinds) in value["ignoreIssues"].as_object().into_iter().flatten() {
        options.report_ignores.push(ReportIgnore {
            field: IgnoreField::IgnoreIssues,
            pattern: pattern.clone(),
            base: base.into(),
            workspace,
            issues: serde_json::from_value(kinds.clone()).unwrap_or_default(),
            origin: inventory
                .configuration_origins
                .get(&format!("{pointer}/ignoreIssues/{}", escape(pattern)))
                .cloned()
                .unwrap_or_else(|| PolicyOrigin {
                    location: None,
                    unavailable: Some("Source provenance unavailable for ignoreIssues".into()),
                }),
        });
    }
}

// Match the discovery comparator: depth, then literal segments, then source order.
fn selected_workspace_key<'a>(
    config: &'a biome_configuration::ProjectAnalysisConfiguration,
    path: &camino::Utf8Path,
) -> Option<&'a str> {
    let mut keys: Vec<_> = config
        .workspaces
        .as_ref()?
        .keys()
        .filter(|key| {
            key.replace('\\', "/")
                .parse::<biome_glob::NormalizedGlob>()
                .is_ok_and(|pattern| pattern.is_raw_match(path))
        })
        .collect();
    keys.sort_by(|left, right| {
        let left: Vec<_> = left.split('/').collect();
        let right: Vec<_> = right.split('/').collect();
        left.len().cmp(&right.len()).then_with(|| {
            left.iter()
                .zip(&right)
                .map(|(left, right)| (!left.contains('*')).cmp(&!right.contains('*')))
                .find(|order| !order.is_eq())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    });
    keys.last().map(|key| key.as_str())
}

/// Binary acquisition consumes the effective boolean before retaining provider gaps.
pub(super) fn prepare_inventory(request: &ProjectAnalysisRequest, inventory: &mut Inventory) {
    for workspace in &mut inventory.workspaces {
        let mut settings: WorkspaceAnalysisConfiguration =
            serde_json::from_str(&workspace.configuration_json).unwrap_or_default();
        settings.ignore_global_binaries = settings
            .ignore_global_binaries
            .or(request.configuration.ignore_global_binaries);
        workspace.configuration_json =
            serde_json::to_string(&settings).expect("static workspace policy");
    }
}

fn origin(inventory: &Inventory, pointer: &str) -> PolicyOrigin {
    inventory
        .configuration_origins
        .get(pointer)
        .cloned()
        .unwrap_or_else(|| PolicyOrigin {
            location: None,
            unavailable: Some(format!("Source provenance unavailable for {pointer}")),
        })
}

fn append_discovery_notices(
    request: &ProjectAnalysisRequest,
    inventory: &Inventory,
    options: &mut AnalysisOptions,
) {
    use biome_project_analysis::{policy::ConfigurationNotice, report_metadata::ReportHint};
    let root = inventory
        .workspaces
        .iter()
        .find(|workspace| workspace.path == ".")
        .map_or(biome_project_analysis::model::WorkspaceId(0), |workspace| {
            workspace.workspace
        });
    for workspace in &inventory.workspaces {
        let value: serde_json::Value =
            serde_json::from_str(&workspace.configuration_json).unwrap_or_default();
        let prefix = selected_workspace_key(&request.configuration, &workspace.path)
            .map(|key| format!("/workspaces/{}", escape(key)))
            .unwrap_or_default();
        for field in ["entry", "project"] {
            let mut seen = BTreeSet::new();
            for (index, pattern) in value[field].as_array().into_iter().flatten().enumerate() {
                let Some(pattern) = pattern.as_str() else {
                    continue;
                };
                if pattern.starts_with('!') {
                    continue;
                }
                let raw = pattern.trim_end_matches('!');
                let Ok(glob) = raw.parse::<biome_glob::NormalizedGlob>() else {
                    continue;
                };
                let matched = inventory
                    .files
                    .iter()
                    .filter(|file| file.workspace == workspace.workspace)
                    .any(|file| {
                        inventory.sources.get(&file.file).is_some_and(|source| {
                            let path = if workspace.path == "." {
                                Some(source.logical_path.as_path())
                            } else {
                                source.logical_path.strip_prefix(&workspace.path).ok()
                            };
                            path.is_some_and(|path| glob.is_raw_match(path))
                        })
                    });
                let repeated = !seen.insert(pattern);
                if !matched || repeated {
                    options.configuration_notices.push(ConfigurationNotice {
                        report: Some(ReportHint::Configuration {
                            identifier: pattern.into(),
                            description: if repeated {
                                format!("Remove redundant {field} pattern")
                            } else {
                                format!("Refine {field} pattern (no matches)")
                            },
                        }),
                        workspace: workspace.workspace,
                        origin: origin(inventory, &format!("{prefix}/{field}/{index}")),
                        message: format!("Review {field} pattern {pattern}"),
                        cause: if repeated {
                            "The same discovery pattern was already supplied".into()
                        } else {
                            "No captured workspace file matches this discovery pattern".into()
                        },
                    });
                }
            }
        }
    }
    if let Some(workspaces) = &request.configuration.workspaces {
        for pattern in workspaces.keys() {
            if pattern
                .parse::<biome_glob::NormalizedGlob>()
                .is_ok_and(|glob| {
                    !inventory
                        .workspaces
                        .iter()
                        .any(|workspace| glob.is_raw_match(&workspace.path))
                })
            {
                options.configuration_notices.push(ConfigurationNotice {
                    report: Some(ReportHint::Configuration {
                        identifier: pattern.clone(),
                        description: "Remove from workspaces".into(),
                    }),
                    workspace: root,
                    origin: origin(inventory, &format!("/workspaces/{}", escape(pattern))),
                    message: format!("Review unused workspace setting {pattern}"),
                    cause: "No discovered workspace matches this setting".into(),
                });
            }
        }
        for (field, patterns) in [
            ("entry", &request.configuration.entry),
            ("project", &request.configuration.project),
        ] {
            if patterns.is_some() {
                options.configuration_notices.push(ConfigurationNotice {report: Some(ReportHint::Configuration {identifier: format!("[{}]", patterns.as_ref().map(|patterns|patterns.0.join(", ")).unwrap_or_default()), description: format!("Remove, or move unused top-level {field} to one of \"workspaces\"")}),workspace:root,origin:origin(inventory,&format!("/{field}")),message:format!("Move top-level {field} into workspaces"),cause:"A workspaces map chooses discovery arrays independently of top-level arrays".into()});
            }
        }
    }
    for (index, pattern) in request
        .configuration
        .ignore_workspaces
        .iter()
        .flat_map(|values| &values.0)
        .enumerate()
    {
        let raw = pattern.trim_start_matches('!').trim_end_matches('!');
        if pattern.parse::<biome_glob::NormalizedGlob>().is_ok()
            && !inventory.workspaces.iter().any(|workspace| {
                raw.parse::<biome_glob::NormalizedGlob>()
                    .is_ok_and(|glob| glob.is_raw_match(&workspace.path))
            })
        {
            options.configuration_notices.push(ConfigurationNotice {
                report: Some(ReportHint::Configuration {
                    identifier: pattern.clone(),
                    description: "Remove from ignoreWorkspaces".into(),
                }),
                workspace: root,
                origin: origin(inventory, &format!("/ignoreWorkspaces/{index}")),
                message: format!("Review unused ignoreWorkspaces pattern {pattern}"),
                cause: "No discovered workspace matches this exclusion".into(),
            });
        }
    }
}
