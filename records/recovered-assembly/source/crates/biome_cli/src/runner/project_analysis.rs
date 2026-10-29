use crate::CliDiagnostic;
use crate::commands::analyze::ProjectAnalysisOptions;
use crate::reporter::project_analysis::{
    ProjectReportInput, ProjectReportRequest, render_project_report,
};
use biome_configuration::{ConfigurationSource, project_analysis::ProjectAnalysisMode};
use biome_console::{Console, LogLevel};
use biome_diagnostics::{Diagnostic, Error, MessageAndDescription, Severity};
use biome_fs::{FileSystem, OpenOptions};
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
use camino::{Utf8Path, Utf8PathBuf};

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
    pub reports: Vec<ProjectReportDestination>,
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
            reports: project_reports(cli)?,
        })
    }

    pub fn prepare_reports(
        &mut self,
        fs: &dyn FileSystem,
        cli: &crate::cli_options::CliOptions,
    ) -> Result<(), CliDiagnostic> {
        prepare_reports(&mut self.reports, fs, cli)
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
    pub reports: Vec<ProjectReportDestination>,
}

impl ProjectCompletion {
    pub fn status(&self, error_on_warnings: bool) -> Result<(), CliDiagnostic> {
        if !matches!(self.result.coverage, Coverage::Complete) {
            return Err(CliDiagnostic::project_failure(
                "Project analysis is incomplete or failed. Resolve the reported coverage gaps before certifying this project.",
            ));
        }
        if let Some(report) = self.reports.first() {
            return match crate::reporter::project_analysis::report_exit_code(
                &self.result,
                &report.request,
            ) {
                0 => Ok(()),
                1 => Err(CliDiagnostic::project_findings(false)),
                _ => Err(CliDiagnostic::project_failure(
                    "Project report has incomplete or failed coverage.",
                )),
            };
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

pub(crate) struct ProjectReportDestination {
    pub request: ProjectReportRequest,
    pub path: Option<Utf8PathBuf>,
}

fn project_reports(
    cli: &ProjectAnalysisOptions,
) -> Result<Vec<ProjectReportDestination>, CliDiagnostic> {
    let number = |value: &Option<String>, name: &str| {
        value
            .as_ref()
            .map(|value| {
                if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
                    return Err(CliDiagnostic::project_failure(format!(
                        "{name} must be a nonnegative integer."
                    )));
                }
                value.parse::<usize>().map_err(|_| {
                    CliDiagnostic::project_failure(format!(
                        "{name} exceeds the supported integer range."
                    ))
                })
            })
            .transpose()
    };
    let request = ProjectReportRequest {
        profile: cli
            .report_profile
            .clone()
            .unwrap_or_else(|| "native".into()),
        format: "symbols".into(),
        max_show_issues: number(&cli.max_show_issues, "--max-show-issues")?,
        max_issues: number(&cli.max_issues, "--max-issues")?,
        no_exit_code: cli.no_exit_code,
        explain: cli.explain.clone(),
        trace_file: cli.trace_file.clone(),
        trace_export: cli.trace_export.clone(),
        trace_dependency: cli.trace_dependency.clone(),
        ..Default::default()
    };
    request.validate()?;
    if !has_structured_reports(cli) {
        return Ok(Vec::new());
    }
    if cli.reporters.is_empty() {
        return Ok(vec![ProjectReportDestination {
            request,
            path: None,
        }]);
    }
    cli.reporters
        .iter()
        .map(|reporter| {
            let request = ProjectReportRequest {
                format: reporter.kind.clone(),
                reporter_options: reporter.options.clone(),
                ..request.clone()
            };
            request.validate()?;
            Ok(ProjectReportDestination {
                request,
                path: reporter.destination.clone(),
            })
        })
        .collect()
}

pub(crate) fn has_structured_reports(cli: &ProjectAnalysisOptions) -> bool {
    cli.report_profile.is_some()
        || !cli.reporters.is_empty()
        || cli.max_show_issues.is_some()
        || cli.max_issues.is_some()
        || cli.explain.is_some()
        || cli.trace_file.is_some()
        || cli.trace_export.is_some()
        || cli.trace_dependency.is_some()
}

fn output_identity(fs: &dyn FileSystem, path: &Utf8Path) -> Utf8PathBuf {
    let path = fs.working_directory().unwrap_or_default().join(path);
    if let Ok(canonical) = fs.canonicalize_path(&path) {
        return canonical;
    }
    let mut ancestor = path.as_path();
    while let Some(parent) = ancestor.parent() {
        if let Ok(canonical) = fs.canonicalize_path(parent)
            && let Ok(suffix) = path.strip_prefix(parent)
        {
            return biome_fs::normalize_path(&canonical.join(suffix));
        }
        ancestor = parent;
    }
    biome_fs::normalize_path(&path)
}
fn companion_path(path: &Utf8Path) -> Utf8PathBuf {
    format!("{path}.coverage.json").into()
}

fn prepare_reports(
    reports: &mut [ProjectReportDestination],
    fs: &dyn FileSystem,
    cli: &crate::cli_options::CliOptions,
) -> Result<(), CliDiagnostic> {
    if reports.is_empty() {
        return Ok(());
    }
    let stdout = reports
        .iter()
        .filter(|report| report.path.is_none())
        .count();
    if stdout > 1
        || stdout > 0
            && cli
                .cli_reporter
                .iter()
                .any(|report| report.destination.is_none())
    {
        return Err(CliDiagnostic::project_failure(
            "Project machine output requires one stdout report. Assign independent --project-report-file and --reporter-file destinations.",
        ));
    }
    let mut destinations = std::collections::BTreeSet::new();
    for path in reports
        .iter()
        .filter_map(|report| report.path.as_ref())
        .flat_map(|path| [path.clone(), companion_path(path)])
        .chain(
            cli.cli_reporter
                .iter()
                .filter_map(|report| report.destination.clone()),
        )
    {
        if fs.path_is_dir(&fs.working_directory().unwrap_or_default().join(&path)) {
            return Err(CliDiagnostic::project_failure(format!(
                "Project report destination is a directory: {path}"
            )));
        }
        if !destinations.insert(output_identity(fs, &path)) {
            return Err(CliDiagnostic::project_failure(format!(
                "Project report destination collision: {path}"
            )));
        }
    }
    let mut inputs: std::collections::BTreeMap<Utf8PathBuf, Option<ProjectReportInput>> =
        Default::default();
    for report in reports {
        report.request.error_on_warnings = cli.error_on_warnings;
        if let Some(path) = report.request.ownership_path()? {
            let identity = output_identity(fs, Utf8Path::new(&path));
            if destinations.contains(&identity) {
                return Err(CliDiagnostic::project_failure(format!(
                    "Report output would overwrite ownership input: {path}"
                )));
            }
            let input = if let Some(input) = inputs.get(&identity) {
                input.clone()
            } else {
                let absolute = fs.working_directory().unwrap_or_default().join(&path);
                let input = if fs.path_exists(&absolute) {
                    Some(ProjectReportInput {
                        path: path.clone(),
                        text: fs.read_file_from_path(&absolute).map_err(|error| {
                            CliDiagnostic::from(biome_service::WorkspaceError::from(error))
                                .into_project_failure()
                        })?,
                    })
                } else {
                    None
                };
                inputs.insert(identity, input.clone());
                input
            };
            if report.request.format == "codeowners" && input.is_none() {
                return Err(CliDiagnostic::project_failure(format!(
                    "CODEOWNERS report input is unavailable: {path}"
                )));
            }
            report.request.codeowners = input;
        }
    }
    Ok(())
}

impl ProjectCompletion {
    pub fn write_reports(
        &self,
        fs: &dyn FileSystem,
        console: &mut dyn Console,
        runtime_failure: bool,
        local_failure: bool,
    ) -> Result<(), CliDiagnostic> {
        let rendered = self
            .reports
            .iter()
            .map(|destination| {
                render_project_report(&self.result, &destination.request)
                    .map(|report| (destination, report))
            })
            .collect::<Result<Vec<_>, _>>()?;
        for (destination, mut report) in rendered {
            report.companion["runtime_failure"] = serde_json::json!(runtime_failure);
            report.companion["command_exit_code"] = serde_json::json!(if runtime_failure {
                2
            } else if local_failure {
                report.exit_code.max(1)
            } else {
                report.exit_code
            });
            let companion = serde_json::to_string(&report.companion)
                .map_err(|error| CliDiagnostic::project_failure(error.to_string()))?
                + "\n";
            if let Some(path) = &destination.path {
                for (path, output) in [
                    (companion_path(path), companion),
                    (path.clone(), report.output),
                ] {
                    let mut file = fs
                        .open_with_options(&path, OpenOptions::default().write(true).create(true))
                        .map_err(|error| CliDiagnostic::from(error).into_project_failure())?;
                    file.set_content(output.as_bytes())
                        .map_err(|error| CliDiagnostic::from(error).into_project_failure())?;
                }
            } else {
                console
                    .try_print_raw(LogLevel::Log, &report.output)
                    .map_err(|error| CliDiagnostic::from(error).into_project_failure())?;
                console
                    .try_print_raw(LogLevel::Error, &companion)
                    .map_err(|error| CliDiagnostic::from(error).into_project_failure())?;
            }
            if !report.stderr.is_empty() {
                console
                    .try_print_raw(LogLevel::Error, &report.stderr)
                    .map_err(|error| CliDiagnostic::from(error).into_project_failure())?;
            }
        }
        Ok(())
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
