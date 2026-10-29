use crate::{ModuleDb, ModuleInfo, SUPPORTED_EXTENSIONS};
use biome_resolver::{
    ResolveOptions, ResolvedSpecifier, ResolverDb, ResolverDbAdapter, ResolverPathData,
    resolve_with_metadata,
};
use camino::Utf8Path;

const JAVASCRIPT_EXTENSION_ALIASES: &[(&str, &[&str])] = &[
    ("js", &["ts", "tsx", "d.ts", "js", "jsx"]),
    ("mjs", &["mts", "d.mts", "mjs"]),
    ("cjs", &["cts", "d.cts", "cjs"]),
];

const HTML_SCRIPT_EXTENSIONS: &[&str] = &[
    "ts", "tsx", "mts", "cts", "js", "jsx", "mjs", "cjs", "json", "node", "vue", "astro", "svelte",
];

/// The resolution rules that apply to an import, which depend on the kind of
/// document that contains it.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ResolutionMode {
    /// Rules for imports of JavaScript and TypeScript modules.
    ///
    /// Resolution follows Node.js and TypeScript: the `exports` and `imports`
    /// of `package.json` with the `types`, `import` and `default` conditions,
    /// the path mappings of `tsconfig.json`, `index` files, and the
    /// extensions in [SUPPORTED_EXTENSIONS]. An import of a `.js` file may
    /// resolve to the matching `.ts` file. Built-in Node.js and Bun modules
    /// resolve to a [ResolveError::NodeBuiltIn] or
    /// [ResolveError::BunBuiltIn] error.
    ///
    /// [ResolveError::NodeBuiltIn]: biome_resolver::ResolveError::NodeBuiltIn
    /// [ResolveError::BunBuiltIn]: biome_resolver::ResolveError::BunBuiltIn
    JavaScript,

    /// Rules for stylesheet imports: `@import` rules of CSS files, and
    /// `<link rel="stylesheet">` elements and `<style>` blocks of HTML-like
    /// documents.
    ///
    /// Like in browsers, a specifier without a leading `./`, such as
    /// `theme.css`, is first resolved relative to the importing file, and only
    /// then as a package. Only `.css` files are resolved.
    Css,

    /// Rules for imports made by `<script>` blocks of HTML-like documents,
    /// such as HTML, Vue, Svelte and Astro files.
    ///
    /// These are the [ResolutionMode::JavaScript] rules, extended with the
    /// `.vue`, `.svelte` and `.astro` component extensions.
    HtmlScript,
}

/// Identifies one module-resolution query.
#[salsa::interned]
#[derive(Debug)]
pub struct ResolutionRequest<'db> {
    #[returns(copy)]
    pub base_directory: ResolverPathData,
    pub specifier: String,
    #[returns(copy)]
    pub mode: ResolutionMode,
}

/// Resolves one interned module request using the path info in the database.
#[salsa::tracked]
pub fn resolve_module_request<'db>(
    db: &'db dyn ResolverDb,
    request: ResolutionRequest<'db>,
) -> ResolvedSpecifier {
    let options = resolve_options(request.mode(db));
    let proxy = ResolverDbAdapter::new(db);
    resolve_with_metadata(
        request.specifier(db),
        request.base_directory(db).path(db),
        &proxy,
        &options,
    )
    .into()
}

/// Resolves `specifier` relative to `base_directory`.
///
/// Every module-graph consumer resolves through this function, so indexing,
/// lint rules and type inference share the same memoized results.
pub fn resolve_specifier<'db>(
    db: &'db dyn ResolverDb,
    base_directory: &Utf8Path,
    specifier: &str,
    mode: ResolutionMode,
) -> &'db ResolvedSpecifier {
    let base_directory = db.resolver_paths().get_or_create(db, base_directory);
    let request = ResolutionRequest::new(db, base_directory, specifier.to_string(), mode);
    resolve_module_request(db, request)
}

/// Resolves `specifier` as imported by `module`.
pub fn resolve_module_import<'db>(
    db: &'db dyn ModuleDb,
    module: ModuleInfo,
    specifier: &str,
    mode: ResolutionMode,
) -> &'db ResolvedSpecifier {
    let module_path = module.path(db);
    let base_directory = module_path.parent().unwrap_or(module_path);
    resolve_specifier(db, base_directory, specifier, mode)
}

fn resolve_options(mode: ResolutionMode) -> ResolveOptions<'static> {
    match mode {
        ResolutionMode::JavaScript => ResolveOptions {
            condition_names: &["types", "import", "default"],
            default_files: &["index"],
            extensions: SUPPORTED_EXTENSIONS,
            extension_aliases: JAVASCRIPT_EXTENSION_ALIASES,
            resolve_node_builtins: true,
            resolve_bun_builtins: true,
            resolve_types: true,
            ..Default::default()
        },
        ResolutionMode::Css => ResolveOptions {
            assume_relative: true,
            extensions: &["css"],
            ..Default::default()
        },
        ResolutionMode::HtmlScript => ResolveOptions {
            condition_names: &["types", "import", "default"],
            default_files: &["index"],
            extensions: HTML_SCRIPT_EXTENSIONS,
            extension_aliases: JAVASCRIPT_EXTENSION_ALIASES,
            resolve_node_builtins: true,
            resolve_bun_builtins: true,
            resolve_types: true,
            ..Default::default()
        },
    }
}

/// Compatibility conditions are independent of the syntax of an occurrence.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum ProjectResolutionProfile {
    #[default]
    Knip,
    Node,
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum ProjectResolutionPurpose {
    #[default]
    Runtime,
    Declaration,
}

/// Every semantic option and its immutable mapping payload participate in cache identity.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct ProjectResolutionContext {
    pub profile: ProjectResolutionProfile,
    pub purpose: ProjectResolutionPurpose,
    pub syntax: crate::ProjectImportSyntax,
    pub config_revision: String,
    pub mapping_revision: String,
    pub additional_conditions: Vec<String>,
    pub source_mappings: Vec<biome_resolver::SourceMapping>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProjectResolution {
    Resolved {
        target: camino::Utf8PathBuf,
        kind: biome_resolver::ResolutionKind,
        trace: biome_resolver::ResolutionTrace,
    },
    Unresolved {
        error: biome_resolver::ResolveError,
        trace: biome_resolver::ResolutionTrace,
    },
    Builtin {
        name: String,
        trace: biome_resolver::ResolutionTrace,
    },
    Unknown {
        reason: String,
        trace: biome_resolver::ResolutionTrace,
    },
}

#[salsa::interned]
struct ProjectResolutionRequest<'db> {
    #[returns(copy)]
    base_directory: ResolverPathData,
    specifier: String,
    context: ProjectResolutionContext,
}

/// Resolves an occurrence without discarding failures, builtins, or selection provenance.
pub fn resolve_project_specifier(
    db: &dyn ResolverDb,
    base_directory: &Utf8Path,
    specifier: &str,
    context: ProjectResolutionContext,
) -> ProjectResolution {
    let base_directory = db.resolver_paths().get_or_create(db, base_directory);
    project_resolution(
        db,
        ProjectResolutionRequest::new(db, base_directory, specifier.to_string(), context),
    )
    .clone()
}

#[salsa::tracked]
fn project_resolution(
    db: &dyn ResolverDb,
    request: ProjectResolutionRequest<'_>,
) -> ProjectResolution {
    use biome_resolver::{ResolutionEvent, ResolveError, resolve_with_trace};
    let context = request.context(db);
    let declaration = context.purpose == ProjectResolutionPurpose::Declaration;
    let mut conditions = match context.profile {
        ProjectResolutionProfile::Knip => vec!["import", "require", "node", "default"],
        ProjectResolutionProfile::Node => vec![
            if matches!(
                context.syntax,
                crate::ProjectImportSyntax::Require | crate::ProjectImportSyntax::RequireResolve
            ) {
                "require"
            } else {
                "import"
            },
            "node",
            "default",
        ],
    };
    if declaration {
        conditions.push("types");
    }
    conditions.extend(context.additional_conditions.iter().map(String::as_str));
    let aliases: &[(&str, &[&str])] = if declaration {
        &[
            ("js", &["d.ts", "ts", "tsx", "js"]),
            ("ts", &["d.ts", "ts"]),
            ("mts", &["d.mts", "mts"]),
            ("cts", &["d.cts", "cts"]),
            ("mjs", &["d.mts", "mts", "mjs"]),
            ("cjs", &["d.cts", "cts", "cjs"]),
        ]
    } else {
        JAVASCRIPT_EXTENSION_ALIASES
    };
    let options = ResolveOptions {
        condition_names: &conditions,
        default_files: &["index"],
        extensions: SUPPORTED_EXTENSIONS,
        extension_aliases: aliases,
        resolve_node_builtins: true,
        resolve_bun_builtins: true,
        resolve_types: declaration,
        source_mappings: &context.source_mappings,
        ..Default::default()
    };
    let proxy = ResolverDbAdapter::new(db);
    let base = request.base_directory(db).path(db);
    let specifier = request.specifier(db);
    let mut resolved = resolve_with_trace(specifier, base, &proxy, &options);
    if context.profile == ProjectResolutionProfile::Knip
        && !declaration
        && matches!(
            resolved.result,
            Err(ResolveError::NotFound | ResolveError::DirectoryWithoutDefault)
        )
    {
        let browser_conditions: Vec<_> = conditions
            .iter()
            .map(|condition| {
                if *condition == "node" {
                    "browser"
                } else {
                    *condition
                }
            })
            .collect();
        let browser = resolve_with_trace(
            specifier,
            base,
            &proxy,
            &ResolveOptions {
                condition_names: &browser_conditions,
                ..options
            },
        );
        resolved.trace.events.extend(browser.trace.events);
        resolved.result = browser.result;
    }
    let trace = resolved.trace;
    if trace.events.iter().any(|event| matches!(event, ResolutionEvent::Failure(ResolveError::ErrorLoadingManifest | ResolveError::InvalidMappingTarget | ResolveError::BrokenSymlink)) || matches!(event, ResolutionEvent::Manifest { quality, .. } if !quality.known || quality.has_errors())) {
        return ProjectResolution::Unknown { reason: "incomplete resolution metadata".into(), trace };
    }
    match resolved.result {
        Ok(resolution) => {
            let kind = resolution.kind();
            ProjectResolution::Resolved {
                target: resolution.into_path(),
                kind,
                trace,
            }
        }
        Err(ResolveError::NodeBuiltIn | ResolveError::BunBuiltIn) => ProjectResolution::Builtin {
            name: specifier.strip_prefix("node:").unwrap_or(specifier).into(),
            trace,
        },
        Err(error) => ProjectResolution::Unresolved { error, trace },
    }
}
