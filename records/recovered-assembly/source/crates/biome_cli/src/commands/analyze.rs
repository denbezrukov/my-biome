use super::get_files_to_process_with_cli_options;
use crate::CliDiagnostic;
use crate::cli_options::CliOptions;
use crate::runner::execution::{AnalyzerSelectors, Execution};
use crate::runner::impls::commands::traversal::TraversalCommand;
use biome_configuration::Configuration;
use biome_console::{Console, MarkupBuf, markup};
use biome_diagnostics::{Category, category};
use biome_fs::FileSystem;
use biome_service::workspace::{
    FeatureName, FeaturesBuilder, FeaturesSupported, ScanKind, SupportKind,
};
use biome_service::{Workspace, WorkspaceError};
use bpaf::Bpaf;
use camino::Utf8PathBuf;
use std::ffi::OsString;
use std::time::Duration;

#[derive(Debug, Default, Clone, Bpaf)]
pub struct ProjectAnalysisOptions {
    /// Selects native or knip-6.43 project presentation. Compatibility remains partial and always includes coverage evidence.
    #[bpaf(long("report-profile"), argument("native|knip-6.43"))]
    pub report_profile: Option<String>,
    #[bpaf(external(crate::cli_options::project_cli_reporter), many)]
    pub reporters: Vec<crate::cli_options::ProjectCliReporter>,
    /// Limits displayed symbols per category without changing totals, coverage or exit status. Other built-in formats retain all issues.
    #[bpaf(long("max-show-issues"), argument("NUMBER"))]
    pub max_show_issues: Option<String>,
    /// Allows this many error findings; equality passes. Hints and runtime failures remain separate. Values must be nonnegative integers.
    #[bpaf(long("max-issues"), argument("NUMBER"))]
    pub max_issues: Option<String>,
    /// Explains a retained issue identity from the completed analysis. Does not run a second analysis.
    #[bpaf(long("explain"), argument("ISSUE_ID"))]
    pub explain: Option<String>,
    /// Traces a captured project-relative file, including files with no finding.
    #[bpaf(long("trace-file"), argument("PATH"))]
    pub trace_file: Option<String>,
    /// Traces a captured export, including used or protected exports.
    #[bpaf(long("trace-export"), argument("PATH:SYMBOL"))]
    pub trace_export: Option<String>,
    /// Traces a dependency's captured declarations, uses and uncertainty.
    #[bpaf(long("trace-dependency"), argument("PACKAGE"))]
    pub trace_dependency: Option<String>,
    /// Selects default, production, or strict project analysis. Overrides the configured mode.
    #[bpaf(long("analysis-mode"), argument("MODE"))]
    pub mode: Option<String>,
    /// Limits project reports to matching workspace names, paths, or globs. Repeat to union; prefix ! to subtract. Other workspaces remain analysis context.
    #[bpaf(long("workspace"), argument("SELECTOR"))]
    pub workspaces: Vec<String>,
    /// Includes comma-separated project issue categories. Explicit configured severity off remains off.
    #[bpaf(long("include"), argument("ISSUES"))]
    pub include: Vec<String>,
    /// Excludes comma-separated project issue categories without excluding graph inputs.
    #[bpaf(long("exclude"), argument("ISSUES"))]
    pub exclude: Vec<String>,
    /// Selects the dependency issue group for project analysis.
    #[bpaf(long("dependencies"), switch)]
    pub dependencies: bool,
    /// Selects the export issue group, excluding namespace-only exports and types.
    #[bpaf(long("exports"), switch)]
    pub exports: bool,
    /// Includes entry exports when neither workspace nor root configuration sets includeEntryExports.
    #[bpaf(long("include-entry-exports"), switch)]
    pub include_entry_exports: bool,
    /// Replaces project tag selection. Native tag spelling preserves digits and underscores.
    #[bpaf(long("tags"), argument("TAGS"))]
    pub tags: Option<String>,
    /// Enables configuration hints. Production and strict modes disable these hints.
    #[bpaf(long("configuration-hints"), argument("true|false"))]
    pub configuration_hints: Option<bool>,
    /// Enables tag hints independently of configuration hints.
    #[bpaf(long("tag-hints"), argument("true|false"))]
    pub tag_hints: Option<bool>,
    /// Treats configuration hints as errors.
    #[bpaf(long("treat-config-hints-as-errors"), argument("true|false"))]
    pub configuration_hint_errors: Option<bool>,
    /// Treats tag hints as errors.
    #[bpaf(long("treat-tag-hints-as-errors"), argument("true|false"))]
    pub tag_hint_errors: Option<bool>,
    /// Suppresses project finding exit failures only. Coverage, configuration, runtime, and local-check failures still fail.
    #[bpaf(long("no-exit-code"), switch)]
    pub no_exit_code: bool,
}

pub(crate) struct AnalyzeCommandPayload {
    pub options: ProjectAnalysisOptions,
    pub paths: Vec<OsString>,
    pub changed: bool,
    pub staged: bool,
    pub since: Option<String>,
    pub watch: bool,
    pub stdin_file_path: Option<String>,
}

impl TraversalCommand for AnalyzeCommandPayload {
    type ProcessFile = ();

    fn command_name(&self) -> &'static str {
        "analyze"
    }
    fn is_watch_mode(&self) -> bool {
        self.watch
    }
    fn minimal_scan_kind(&self) -> Option<ScanKind> {
        Some(ScanKind::Project)
    }
    fn project_analysis_options(&self) -> Option<&ProjectAnalysisOptions> {
        Some(&self.options)
    }
    fn get_execution(
        &self,
        _: &CliOptions,
        _: &mut dyn Console,
        _: &dyn Workspace,
    ) -> Result<Box<dyn Execution>, CliDiagnostic> {
        Ok(Box::new(AnalyzeExecution {
            vcs_targeted: self.changed || self.staged,
            stdin_file_path: self.stdin_file_path.clone(),
        }))
    }
    fn merge_configuration(
        &mut self,
        configuration: Configuration,
        _: Option<Utf8PathBuf>,
        _: Option<Utf8PathBuf>,
        _: &dyn FileSystem,
        _: &mut dyn Console,
    ) -> Result<Configuration, WorkspaceError> {
        Ok(configuration)
    }
    fn get_files_to_process(
        &self,
        fs: &dyn FileSystem,
        configuration: &Configuration,
    ) -> Result<Vec<OsString>, CliDiagnostic> {
        Ok(get_files_to_process_with_cli_options(
            self.since.as_deref(),
            self.changed,
            self.staged,
            fs,
            configuration,
        )?
        .unwrap_or_else(|| self.paths.clone()))
    }
}

struct AnalyzeExecution {
    vcs_targeted: bool,
    stdin_file_path: Option<String>,
}
impl Execution for AnalyzeExecution {
    fn wanted_features(&self) -> FeatureName {
        FeaturesBuilder::new().build()
    }
    fn not_requested_features(&self) -> FeatureName {
        FeaturesBuilder::new().with_all().build()
    }
    fn can_handle(&self, _: FeaturesSupported) -> bool {
        false
    }
    fn is_vcs_targeted(&self) -> bool {
        self.vcs_targeted
    }
    fn supports_kind(&self, _: &FeaturesSupported) -> Option<SupportKind> {
        None
    }
    fn get_stdin_file_path(&self) -> Option<&str> {
        self.stdin_file_path.as_deref()
    }
    fn as_diagnostic_category(&self) -> &'static Category {
        category!("project")
    }
    fn requires_write_access(&self) -> bool {
        false
    }
    fn analyzer_selectors(&self) -> AnalyzerSelectors {
        AnalyzerSelectors::default()
    }
    fn is_analyze(&self) -> bool {
        true
    }
    fn summary_phrase(&self, _: usize, _: &Duration) -> MarkupBuf {
        markup! { "Analyzed the project (experimental; framework and tool plugins are deferred)." }
            .to_owned()
    }
}
