use crate::CliDiagnostic;
use crate::commands::analyze::ProjectAnalysisOptions;
use biome_configuration::{ConfigurationSource, project_analysis::ProjectAnalysisMode};
use biome_diagnostics::{Diagnostic, Error, MessageAndDescription, Severity};
use biome_project_analysis::{
    coverage::{Coverage, GapReason},
    model::{Location, SourceContent},
    options::{AnalysisMode, ReportSelection},
    result::{IssueKind, ProjectAnalysisResult},
    select_report,
};
use biome_service::{
    Workspace,
    project_analysis::{PolicyOverrides, ProjectAnalysisRequest, select_workspaces},
    workspace::PullProjectDiagnosticsParams,
};
use camino::Utf8Path;

pub(crate) fn classify_early_failure(
    error: CliDiagnostic,
    fs: &dyn biome_resolver::FsWithResolverProxy,
    cli: &crate::cli_options::CliOptions,
    supports_analysis: bool,
    explicit_analysis: bool,
) -> CliDiagnostic {
    if matches!(error, CliDiagnostic::ProjectAnalysis(_)) {
        return error;
    }
    let enabled = explicit_analysis
        || (supports_analysis && {
            let working_dir = fs.working_directory().unwrap_or_default();
            biome_service::configuration::load_configuration(
                fs,
                cli.as_configuration_path_hint(&working_dir),
            )
            .is_ok_and(|loaded| {
                loaded
                    .resolved_configuration()
                    .project_analysis
                    .as_ref()
                    .is_some_and(|configuration| configuration.enabled == Some(true))
            })
        });
    if enabled {
        error.into_project_failure()
    } else {
        error
    }
}

pub(crate) struct ProjectPhase {
    pub request: ProjectAnalysisRequest,
    pub overrides: PolicyOverrides,
    pub workspaces: Vec<String>,
    pub no_exit_code: bool,
}

impl ProjectPhase {
    pub fn new(
        source: &ConfigurationSource,
        cli: &ProjectAnalysisOptions,
    ) -> Result<Self, CliDiagnostic> {
        let configuration = source.resolve();
        let configured_mode = configuration.project_analysis.as_ref().and_then(|c| c.mode);
        let mode = match cli.mode.as_deref() {
            Some("default") => AnalysisMode::Default,
            Some("production") => AnalysisMode::Production,
            Some("strict") => AnalysisMode::Strict,
            Some(value) => {
                return Err(CliDiagnostic::project_failure(format!(
                    "Invalid analysis mode {value:?}; use default, production, or strict."
                )));
            }
            None => match configured_mode.unwrap_or_default() {
                ProjectAnalysisMode::Default => AnalysisMode::Default,
                ProjectAnalysisMode::Production => AnalysisMode::Production,
                ProjectAnalysisMode::Strict => AnalysisMode::Strict,
            },
        };
        Ok(Self {
            request: ProjectAnalysisRequest::from_configuration_source(source, mode),
            overrides: PolicyOverrides {
                include: parse_issues(&cli.include)?,
                exclude: parse_issues(&cli.exclude)?,
                dependencies: cli.dependencies,
                exports: cli.exports,
                include_entry_exports: cli.include_entry_exports,
                tags: cli.tags.as_ref().map(|value| vec![value.clone()]),
                configuration_hints: cli.configuration_hints,
                tag_hints: cli.tag_hints,
                configuration_hint_errors: cli.configuration_hint_errors,
                tag_hint_errors: cli.tag_hint_errors,
            },
            workspaces: cli.workspaces.clone(),
            no_exit_code: cli.no_exit_code,
        })
    }

    pub fn select(
        &self,
        result: &ProjectAnalysisResult,
        paths: &[String],
        working_dir: &Utf8Path,
        vcs_targeted: bool,
    ) -> Result<ProjectAnalysisResult, CliDiagnostic> {
        let root = Utf8Path::new(&result.revision.project_root);
        let paths = paths
            .iter()
            .map(|path| {
                let absolute = biome_fs::normalize_path(&working_dir.join(path));
                absolute
                    .strip_prefix(root)
                    .map(Utf8Path::to_path_buf)
                    .map_err(|_| {
                        CliDiagnostic::project_failure(format!(
                            "Report path is outside the acquired project: {path}"
                        ))
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let (workspaces, errors) = select_workspaces(
            root,
            &result.workspaces.values().cloned().collect::<Vec<_>>(),
            &self.workspaces,
        );
        if !errors.is_empty() {
            return Err(CliDiagnostic::project_failure(errors.join("\n")));
        }
        let mut selection = ReportSelection {
            workspaces: if self.workspaces.is_empty() {
                Default::default()
            } else {
                workspaces.into_iter().collect()
            },
            restrict_workspaces: !self.workspaces.is_empty(),
            ..Default::default()
        };
        if vcs_targeted {
            selection.changed_paths = Some(paths.into_iter().collect());
        } else {
            selection.paths = paths;
        }
        Ok(select_report(result, &selection))
    }
}

fn parse_issues(values: &[String]) -> Result<Vec<IssueKind>, CliDiagnostic> {
    values
        .iter()
        .flat_map(|value| value.split(','))
        .map(|value| {
            serde_json::from_value(serde_json::Value::String(value.to_string())).map_err(|_| {
                CliDiagnostic::project_failure(format!("Unknown project issue category: {value}"))
            })
        })
        .collect()
}

pub(crate) fn run_project_phase(
    workspace: &dyn Workspace,
    params: PullProjectDiagnosticsParams,
) -> Result<ProjectAnalysisResult, CliDiagnostic> {
    workspace
        .pull_project_diagnostics(params)
        .map_err(|error| CliDiagnostic::from(error).into_project_failure())
}

pub(crate) struct ProjectCompletion {
    pub result: ProjectAnalysisResult,
    pub no_exit_code: bool,
}

impl ProjectCompletion {
    pub fn status(&self, error_on_warnings: bool) -> Result<(), CliDiagnostic> {
        if !matches!(self.result.coverage, Coverage::Complete) {
            return Err(CliDiagnostic::project_failure(
                "Project analysis is incomplete or failed. Resolve the reported coverage gaps before certifying this project.",
            ));
        }
        let threshold = if error_on_warnings {
            Severity::Warning
        } else {
            Severity::Error
        };
        let failing = self
            .result
            .issues
            .iter()
            .any(|issue| issue.severity >= threshold)
            || self
                .result
                .hints
                .iter()
                .any(|hint| hint.severity >= threshold);
        if failing {
            Err(CliDiagnostic::project_findings(self.no_exit_code))
        } else {
            Ok(())
        }
    }

    pub fn diagnostics(&self) -> Vec<Error> {
        let result = &self.result;
        let mut diagnostics = Vec::new();
        for issue in &result.issues {
            let kind = serde_json::to_value(issue.kind).expect("issue kind serializes");
            let message = format!("{}: {}", kind.as_str().unwrap(), issue.identity);
            diagnostics.push(
                ProjectDiagnostic::at(result, Some(issue.location), issue.severity, message).into(),
            );
        }
        for hint in &result.hints {
            diagnostics.push(
                ProjectDiagnostic::at(result, hint.location, hint.severity, hint.message.clone())
                    .into(),
            );
        }
        for gap in result.coverage.gaps() {
            let reason = match &gap.reason {
                GapReason::Capability(reason)
                | GapReason::DynamicReference(reason)
                | GapReason::UnavailableInput(reason) => reason,
            };
            diagnostics.push(
                ProjectDiagnostic::at(
                    result,
                    gap.location,
                    Severity::Warning,
                    format!("Project coverage gap ({}): {reason}", gap.producer),
                )
                .into(),
            );
        }
        diagnostics
    }

    pub fn coverage_message(&self) -> String {
        let coverage = match self.result.coverage {
            Coverage::Complete => "complete",
            Coverage::Incomplete(_) => "incomplete",
            Coverage::Failed(_) => "failed",
        };
        format!(
            "Project analysis coverage: {coverage}; {} findings, {} hints. Experimental coverage; framework and tool plugins are deferred.",
            self.result.issues.len(),
            self.result.hints.len()
        )
    }
}

#[derive(Debug, Diagnostic)]
#[diagnostic(category = "project")]
struct ProjectDiagnostic {
    #[severity]
    severity: Severity,
    #[message]
    #[description]
    message: MessageAndDescription,
    #[location(resource)]
    path: Option<String>,
    #[location(source_code)]
    source: Option<String>,
    #[location(span)]
    range: Option<biome_rowan::TextRange>,
}

impl ProjectDiagnostic {
    fn at(
        result: &ProjectAnalysisResult,
        location: Option<Location>,
        severity: Severity,
        message: String,
    ) -> Self {
        let source = location.and_then(|location| result.sources.get(&location.file));
        let (text, range) = match source.map(|source| &source.content) {
            Some(SourceContent::Available { text, .. }) => {
                (Some(text.clone()), location.map(|location| location.range))
            }
            _ => (None, None),
        };
        Self {
            severity,
            message: message.into(),
            path: source.map(|source| source.logical_path.to_string()),
            source: text,
            range,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use biome_configuration::{
        Configuration, ConfigurationSourceEntry, project_analysis::ProjectAnalysisConfiguration,
    };

    #[test]
    fn configured_mode_and_explicit_default_remain_distinct() {
        let source = ConfigurationSource {
            root: Some(ConfigurationSourceEntry {
                configuration: Some(Configuration {
                    project_analysis: Some(ProjectAnalysisConfiguration {
                        mode: Some(ProjectAnalysisMode::Production),
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
                file_path: None,
                file_source: None,
            }),
            ..Default::default()
        };
        for (mode, expected) in [
            (None, AnalysisMode::Production),
            (Some("default"), AnalysisMode::Default),
            (Some("strict"), AnalysisMode::Strict),
        ] {
            let phase = ProjectPhase::new(
                &source,
                &ProjectAnalysisOptions {
                    mode: mode.map(str::to_string),
                    ..Default::default()
                },
            )
            .unwrap();
            assert_eq!(phase.request.mode, expected);
            assert_eq!(
                phase.request.analysis_options(&Default::default()).mode,
                expected
            );
        }
    }
}
