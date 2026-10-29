use super::{ProjectReportRequest, compatibility, json_line};
use crate::CliDiagnostic;
use biome_project_analysis::result::ProjectAnalysisResult;

pub(super) fn render(
    result: &ProjectAnalysisResult,
    request: &ProjectReportRequest,
) -> Result<(String, String), CliDiagnostic> {
    if request.format == "json" {
        return Ok((json_line(result)?, String::new()));
    }
    compatibility::render(result, request, true)
}
