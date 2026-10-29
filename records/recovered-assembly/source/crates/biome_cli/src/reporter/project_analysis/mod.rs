//! Structured reports borrow one completed result and return independent output buffers.
mod compatibility;
mod explain;
mod native;

use crate::CliDiagnostic;
use biome_diagnostics::Severity;
use biome_project_analysis::{
    coverage::Coverage,
    model::{Location, SourceContent},
    result::ProjectAnalysisResult,
};
use serde::Serialize;
use serde_json::{Value, json};

pub const BUILTIN_REPORTERS: [&str; 10] = [
    "symbols",
    "compact",
    "codeowners",
    "cycles",
    "disclosure",
    "codeclimate",
    "json",
    "markdown",
    "sarif",
    "github-actions",
];

#[derive(Clone, Debug, Default)]
pub struct ProjectReportRequest {
    pub profile: String,
    pub format: String,
    pub max_show_issues: Option<usize>,
    pub max_issues: Option<usize>,
    pub no_exit_code: bool,
    pub error_on_warnings: bool,
    pub explain: Option<String>,
    pub trace_file: Option<String>,
    pub trace_export: Option<String>,
    pub trace_dependency: Option<String>,
    pub reporter_options: Option<String>,
    pub codeowners: Option<ProjectReportInput>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ProjectReportInput {
    pub path: String,
    pub text: String,
}

#[derive(Clone, Debug, Default)]
pub struct RenderedProjectReport {
    pub output: String,
    pub stderr: String,
    pub companion: Value,
    pub issue_total: usize,
    pub exit_code: u8,
}

impl ProjectReportRequest {
    pub fn validate(&self) -> Result<(), CliDiagnostic> {
        if !matches!(self.profile.as_str(), "" | "native" | "knip-6.43") {
            return Err(failure(format!(
                "Unsupported project report profile: {}",
                self.profile
            )));
        }
        if !self.format.is_empty() && !BUILTIN_REPORTERS.contains(&self.format.as_str()) {
            return Err(failure(format!(
                "Unsupported custom project reporter: {}. Only built-in reporters are available; JavaScript is not executed.",
                self.format
            )));
        }
        if [
            &self.explain,
            &self.trace_file,
            &self.trace_export,
            &self.trace_dependency,
        ]
        .iter()
        .filter(|value| value.is_some())
        .count()
            > 1
        {
            return Err(failure("Choose one project explanation or trace query."));
        }
        if let Some(options) = &self.reporter_options {
            let options: Value = serde_json::from_str(options)
                .map_err(|error| failure(format!("Invalid project reporter options: {error}")))?;
            let map = options
                .as_object()
                .ok_or_else(|| failure("Project reporter options must be a JSON object."))?;
            let key = match self.format.as_str() {
                "codeowners" => "path",
                "json" => "codeowners",
                _ => "",
            };
            if map
                .iter()
                .any(|(name, value)| name != key || !value.is_string())
            {
                return Err(failure(
                    "Unsupported project reporter option. codeowners accepts path; json accepts codeowners; values must be paths.",
                ));
            }
        }
        Ok(())
    }

    pub fn ownership_path(&self) -> Result<Option<String>, CliDiagnostic> {
        self.validate()?;
        if !matches!(self.format.as_str(), "json" | "codeowners") {
            return Ok(None);
        }
        let options: Value = self
            .reporter_options
            .as_ref()
            .map(|options| serde_json::from_str(options))
            .transpose()
            .map_err(|error| failure(error.to_string()))?
            .unwrap_or_default();
        Ok(Some(
            options[if self.format == "json" {
                "codeowners"
            } else {
                "path"
            }]
            .as_str()
            .unwrap_or(".github/CODEOWNERS")
            .into(),
        ))
    }
}

pub fn render_project_report(
    result: &ProjectAnalysisResult,
    request: &ProjectReportRequest,
) -> Result<RenderedProjectReport, CliDiagnostic> {
    request.validate()?;
    let errors = result
        .issues
        .iter()
        .filter(|issue| issue.severity >= Severity::Error)
        .count();
    let exit_code = report_exit_code(result, request);
    let companion = json!({
        "revision": result.revision, "context": result.context,
        "coverage": result.coverage, "capabilities": result.capabilities,
        "identities": result.issues.iter().map(|issue|&issue.identity).collect::<Vec<_>>(),
        "issues": result.issues, "hints": result.hints, "evidence": result.evidence,
        "sources": result.sources, "workspaces": result.workspaces,
        "issue_total": result.issues.len(), "error_total": errors, "exit_code": exit_code,
        "qualification_eligible": false, "compatibility_status": "partial; actual-project qualification and benchmarks remain pending",
        "profile": if request.profile.is_empty() {"native"} else {&request.profile},
        "reporter": if request.format.is_empty() {"symbols"} else {&request.format},
        "inputs": {"reporter_options": request.reporter_options,"codeowners":request.codeowners},
    });
    let (output, stderr) = if request.explain.is_some()
        || request.trace_file.is_some()
        || request.trace_export.is_some()
        || request.trace_dependency.is_some()
    {
        (explain::render(result, request)?, String::new())
    } else if request.profile == "knip-6.43" {
        compatibility::render(result, request, false)?
    } else {
        native::render(result, request)?
    };
    Ok(RenderedProjectReport {
        output,
        stderr,
        companion,
        issue_total: result.issues.len(),
        exit_code,
    })
}

fn failure(message: impl Into<String>) -> CliDiagnostic {
    CliDiagnostic::project_failure(message)
}
fn json_line(value: &impl Serialize) -> Result<String, CliDiagnostic> {
    serde_json::to_string(value)
        .map(|value| value + "\n")
        .map_err(|error| failure(format!("Could not serialize project report: {error}")))
}

#[derive(Clone, Copy)]
struct Position {
    line: usize,
    col: usize,
    pos: usize,
}
fn position(result: &ProjectAnalysisResult, location: Location) -> Option<Position> {
    let SourceContent::Available { text, .. } = &result.sources.get(&location.file)?.content else {
        return None;
    };
    let prefix = text.get(..usize::from(location.range.start()))?;
    Some(Position {
        line: prefix.bytes().filter(|byte| *byte == b'\n').count() + 1,
        col: prefix.rsplit('\n').next()?.encode_utf16().count() + 1,
        pos: prefix.encode_utf16().count(),
    })
}
fn source_path(result: &ProjectAnalysisResult, location: Location) -> Result<&str, CliDiagnostic> {
    result
        .sources
        .get(&location.file)
        .map(|source| source.logical_path.as_str())
        .ok_or_else(|| failure("Project report source identity is unavailable."))
}

pub(crate) fn report_exit_code(
    result: &ProjectAnalysisResult,
    request: &ProjectReportRequest,
) -> u8 {
    let threshold = if request.profile != "knip-6.43" && request.error_on_warnings {
        Severity::Warning
    } else {
        Severity::Error
    };
    let errors = result
        .issues
        .iter()
        .filter(|issue| issue.severity >= threshold)
        .count();
    let hint_errors = result.hints.iter().any(|hint| hint.severity >= threshold);
    if !matches!(result.coverage, Coverage::Complete) {
        2
    } else if !request.no_exit_code && (errors > request.max_issues.unwrap_or(0) || hint_errors) {
        1
    } else {
        0
    }
}
