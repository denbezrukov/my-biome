use super::{
    ProjectAnalysisRequest,
    paths::{self, Patterns},
    tsconfig,
};
use crate::{WorkspaceError, projects::ProjectKey, scanner::WorkspaceScannerBridge};
use biome_configuration::project_analysis::{
    ProjectAnalysisConfiguration, WorkspaceAnalysisConfiguration,
};
use biome_diagnostics::{Diagnostic as _, Error};
use biome_fs::{BiomePath, PathInterner, TraversalContext};
use biome_package::{
    MetadataValue, ProjectMetadata, WorkspaceMetadata, read_project_metadata,
    read_workspace_metadata,
};
use biome_project_analysis::{
    coverage::{CoverageGap, GapReason, TargetDomain},
    inventory::*,
    model::{FileId, Location, SourceContent, SourceRecord, WorkspaceId},
    options::AnalysisMode,
};
use biome_rowan::TextRange;
use camino::{Utf8Path, Utf8PathBuf};
use papaya::{HashSet, HashSetRef, LocalGuard};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    hash::RandomState,
    sync::Mutex,
};

pub(super) struct Capture<'a> {
    pub fs: &'a dyn biome_resolver::FsWithResolverProxy,
    pub root: Utf8PathBuf,
    pub inventory: Inventory,
    pub paths: BTreeMap<Utf8PathBuf, FileId>,
}
impl Capture<'_> {
    pub(super) fn gap(
        &mut self,
        producer: &str,
        message: impl Into<String>,
        location: Option<Location>,
    ) {
        self.inventory.gaps.push(CoverageGap {
            producer: producer.into(),
            domain: TargetDomain::Unbounded,
            reason: GapReason::UnavailableInput(message.into()),
            location,
        });
    }
    pub(super) fn capture(&mut self, path: &Utf8Path) -> FileId {
        let canonical = paths::canonical(self.fs, path).unwrap_or_else(|error| {
            self.gap("path identity", error, None);
            path.to_path_buf()
        });
        if let Some(file) = self.paths.get(&canonical) {
            return *file;
        }
        let file = FileId(self.paths.len() as u32);
        self.paths.insert(canonical.clone(), file);
        let logical_path = canonical
            .strip_prefix(&self.root)
            .unwrap_or(&canonical)
            .to_path_buf();
        let content = match self.fs.read_file_from_path(&canonical) {
            Ok(text) => SourceContent::Available {
                digest: format!("{:x}", Sha256::digest(text.as_bytes())),
                text,
            },
            Err(error) => {
                self.gap(
                    "source",
                    error.to_string(),
                    Some(Location {
                        file,
                        range: TextRange::default(),
                    }),
                );
                SourceContent::Unavailable {
                    reason: error.to_string(),
                    fallback: logical_path.clone(),
                }
            }
        };
        let canonical_uri = url::Url::from_file_path(canonical.as_std_path())
            .map_or_else(|_| canonical.to_string(), |url| url.to_string());
        self.inventory.sources.insert(
            file,
            SourceRecord {
                logical_path,
                canonical_uri,
                content,
            },
        );
        file
    }
    pub(super) fn text(&self, file: FileId) -> Option<&str> {
        match &self.inventory.sources[&file].content {
            SourceContent::Available { text, .. } => Some(text),
            SourceContent::Unavailable { .. } => None,
        }
    }
    pub(super) fn diagnostics(
        &mut self,
        file: FileId,
        diagnostics: &[biome_diagnostics::serde::Diagnostic],
    ) -> Vec<MetadataDiagnostic> {
        diagnostics
            .iter()
            .map(|diagnostic| {
                let location = Location {
                    file,
                    range: diagnostic.location().span.unwrap_or_default(),
                };
                let message = biome_diagnostics::PrintDescription(diagnostic).to_string();
                self.gap("metadata", message.clone(), Some(location));
                MetadataDiagnostic { location, message }
            })
            .collect()
    }
    pub(super) fn patterns(
        &mut self,
        values: &[String],
        mode: AnalysisMode,
        marked: bool,
        producer: &str,
    ) -> Patterns {
        let (patterns, errors) = Patterns::new(values, mode, marked);
        for error in errors {
            self.gap(producer, error, None);
        }
        patterns
    }
}

struct DiscoveryPaths {
    interner: PathInterner,
    paths: HashSet<BiomePath>,
    aliases: Mutex<BTreeSet<Utf8PathBuf>>,
    errors: Mutex<Vec<String>>,
}
impl TraversalContext for DiscoveryPaths {
    fn interner(&self) -> &PathInterner {
        &self.interner
    }
    fn evaluated_paths(&self) -> HashSetRef<'_, BiomePath, RandomState, LocalGuard<'_>> {
        self.paths.pin()
    }
    fn push_diagnostic(&self, error: Error) {
        self.errors
            .lock()
            .unwrap()
            .push(biome_diagnostics::PrintDescription(&error).to_string());
    }
    fn can_handle(&self, path: &BiomePath) -> bool {
        let accepted = !path
            .components()
            .any(|part| matches!(part.as_str(), "node_modules" | ".git"));
        if accepted {
            self.aliases.lock().unwrap().insert(path.to_path_buf());
        }
        accepted
    }
    fn handle_path(&self, path: BiomePath) {
        self.store_path(path);
    }
    fn store_path(&self, path: BiomePath) {
        self.paths.pin().insert(path);
    }
}

struct Package {
    path: Utf8PathBuf,
    file: FileId,
    metadata: ProjectMetadata,
    diagnostics: Vec<MetadataDiagnostic>,
    settings: WorkspaceAnalysisConfiguration,
    config: tsconfig::EffectiveConfig,
}

pub(super) fn discover(
    service: &(impl WorkspaceScannerBridge + crate::Workspace),
    project: ProjectKey,
    request: &ProjectAnalysisRequest,
) -> Result<Inventory, WorkspaceError> {
    Ok(discover_with_context(service, project, request, &[])?.inventory)
}

pub(super) struct Discovery {
    pub inventory: Inventory,
    pub mappings: Vec<(Utf8PathBuf, Vec<(Utf8PathBuf, Utf8PathBuf)>)>,
}

pub(super) fn discover_with_context(
    service: &(impl WorkspaceScannerBridge + crate::Workspace),
    project: ProjectKey,
    request: &ProjectAnalysisRequest,
    overlay_paths: &[Utf8PathBuf],
) -> Result<Discovery, WorkspaceError> {
    let root = service
        .get_project_path(project)
        .ok_or_else(WorkspaceError::no_project)?;
    let fs = crate::Workspace::fs(service);
    let root_identity = paths::canonical(fs, &root);
    let root = root_identity.as_ref().unwrap_or(&root).clone();
    let traversal = DiscoveryPaths {
        interner: PathInterner::new().0,
        paths: HashSet::new(),
        aliases: Mutex::new(BTreeSet::new()),
        errors: Mutex::new(Vec::new()),
    };
    fs.traversal(Box::new(|scope| scope.evaluate(&traversal, root.clone())));
    let mut discovered: BTreeSet<Utf8PathBuf> = traversal
        .paths
        .pin()
        .iter()
        .map(|p| p.to_path_buf())
        .collect();
    discovered.extend(
        overlay_paths
            .iter()
            .filter(|path| path.starts_with(&root))
            .cloned(),
    );
    let aliases = traversal.aliases.lock().unwrap();
    discovered.extend(aliases.iter().filter(|path| fs.path_is_file(path)).cloned());
    let canonical_files: Vec<_> = discovered.iter().cloned().collect();
    for alias in aliases
        .iter()
        .filter(|path| fs.path_is_dir(path) && fs.path_is_symlink(path))
    {
        if let Ok(target) = paths::canonical(fs, alias) {
            for path in &canonical_files {
                if let Ok(relative) = path.strip_prefix(&target) {
                    discovered.insert(alias.join(relative));
                }
            }
        }
    }
    drop(aliases);
    let mut capture = Capture {
        fs,
        root: root.clone(),
        inventory: Inventory::default(),
        paths: BTreeMap::new(),
    };
    if let Err(error) = root_identity {
        capture.gap("project identity", error, None);
    }
    capture.inventory.request_json = serde_json::to_string(request).unwrap();
    for error in traversal.errors.into_inner().unwrap() {
        capture.gap("filesystem traversal", error, None);
    }
    let mut aliases_by_file: BTreeMap<FileId, Vec<Utf8PathBuf>> = BTreeMap::new();
    for path in &discovered {
        if paths::is_source(path)
            || matches!(path.extension(), Some("json" | "jsonc" | "yaml" | "yml"))
            || path.file_name() == Some(".gitignore")
        {
            let file = capture.capture(path);
            aliases_by_file
                .entry(file)
                .or_default()
                .push(relative(&root, path));
        }
    }
    let mut manifests = BTreeMap::new();
    let mut uncertain_manifests = Vec::new();
    for path in discovered
        .iter()
        .filter(|p| p.file_name() == Some("package.json"))
    {
        let file = capture.capture(path);
        let Some(text) = capture.text(file) else {
            uncertain_manifests.push((path.parent().unwrap().to_path_buf(), file));
            continue;
        };
        let parsed = biome_json_parser::parse_json(text, Default::default());
        let metadata = read_project_metadata(&parsed.tree());
        let parser_diagnostics: Vec<_> = parsed
            .diagnostics()
            .iter()
            .cloned()
            .map(biome_diagnostics::serde::Diagnostic::new)
            .collect();
        let mut diagnostics = capture.diagnostics(file, &parser_diagnostics);
        diagnostics.extend(capture.diagnostics(file, &metadata.diagnostics));
        diagnostics.extend(capture.diagnostics(file, &metadata.catalogs.diagnostics));
        if !diagnostics.is_empty() {
            uncertain_manifests.push((path.parent().unwrap().to_path_buf(), file));
        }
        if parsed.has_errors() {
            continue;
        }
        manifests.insert(
            path.parent().unwrap().to_path_buf(),
            (file, metadata, diagnostics),
        );
    }
    let pnpm_path = root.join("pnpm-workspace.yaml");
    let yarn_path = root.join(".yarnrc.yml");
    let yaml = if fs.path_exists(&pnpm_path) {
        Some(load_yaml(&mut capture, &pnpm_path, WorkspaceYamlKind::Pnpm))
    } else if fs.path_exists(&yarn_path) {
        Some(load_yaml(&mut capture, &yarn_path, WorkspaceYamlKind::Yarn))
    } else {
        None
    };
    let manifest_patterns = manifests
        .get(&root)
        .and_then(|(_, metadata, _)| metadata.fields.get("workspaces"))
        .map(|value| value.members.get("packages").unwrap_or(value));
    let membership = if fs.path_exists(&pnpm_path) {
        yaml.as_ref()
            .and_then(|yaml| {
                yaml.metadata
                    .fields
                    .get("packages")
                    .filter(|value| !value.value.is_null())
            })
            .or(manifest_patterns)
    } else {
        manifest_patterns
    };
    let patterns = capture.patterns(
        &membership.map(strings).unwrap_or_default(),
        request.mode,
        false,
        "workspace membership",
    );
    let configured: Vec<_> = request
        .configuration
        .workspaces
        .as_ref()
        .map(|map| map.keys().cloned().collect())
        .unwrap_or_default();
    let configured_patterns =
        capture.patterns(&configured, request.mode, false, "workspace settings");
    for (path, file) in uncertain_manifests {
        let relative = relative(&root, &path);
        if path == root || patterns.matches(&relative) || configured_patterns.matches(&relative) {
            capture.inventory.catalog_consumer_gaps.push(CoverageGap {
                producer: "catalog consumer metadata".into(),
                domain: TargetDomain::Unbounded,
                reason: GapReason::UnavailableInput(
                    "Workspace manifest may contain uncaptured catalog references".into(),
                ),
                location: Some(Location {
                    file,
                    range: TextRange::default(),
                }),
            });
        }
    }
    let mut packages = Vec::new();
    for (path, (file, metadata, diagnostics)) in manifests {
        let relative = relative(&root, &path);
        if path != root && !patterns.matches(&relative) && !configured_patterns.matches(&relative) {
            continue;
        }
        let settings = workspace_settings(&request.configuration, &relative);
        let tsconfig_path = request
            .configuration
            .tsconfig
            .as_ref()
            .and_then(|v| v.0.first())
            .map_or_else(|| path.join("tsconfig.json"), |v| paths::join(&path, v));
        if request
            .configuration
            .tsconfig
            .as_ref()
            .is_some_and(|v| v.0.len() > 1)
        {
            capture.gap(
                "tsconfig",
                "Multiple initial tsconfig paths are unsupported.",
                None,
            );
        }
        let config = tsconfig::load(
            &mut capture,
            &tsconfig_path,
            &mut BTreeSet::new(),
            request.configuration.tsconfig.is_some(),
        );
        packages.push(Package {
            path,
            file,
            metadata,
            diagnostics,
            settings,
            config,
        });
    }
    let mut canonical_owners = BTreeMap::new();
    for (index, package) in packages.iter().enumerate() {
        let workspace = WorkspaceId(index as u32);
        let canonical =
            paths::canonical(fs, &package.path).unwrap_or_else(|_| package.path.clone());
        if canonical_owners.insert(canonical, workspace).is_some() {
            capture.gap(
                "workspace ownership",
                "Duplicate ownership through workspace aliases.",
                Some(Location {
                    file: package.file,
                    range: TextRange::default(),
                }),
            );
        }
        let name = package
            .metadata
            .fields
            .get("name")
            .and_then(string)
            .map(str::to_owned);
        capture.inventory.workspaces.push(WorkspaceRecord {
            workspace,
            path: relative(&root, &package.path),
            manifest: package.file,
            declaration_locations: Vec::new(),
            metadata_diagnostics: package.diagnostics.clone(),
            name,
            metadata: package
                .metadata
                .fields
                .iter()
                .map(|(key, value)| (key.clone(), owned(value, package.file)))
                .collect(),
            catalogs: Vec::new(),
            dependency_uses: Vec::new(),
            configuration_json: serde_json::to_string(&package.settings).unwrap(),
        });
    }
    let mut excluded = BTreeSet::new();
    if let Some(boundary) = &request.isolation
        && !boundary.excluded_consumers.is_empty()
    {
        let (selected, errors) = select_workspaces(
            &capture.root,
            &capture.inventory.workspaces,
            &boundary.excluded_consumers,
        );
        excluded.extend(selected);
        for error in errors {
            capture.gap("analysis boundary", error, None);
        }
    }
    if let Some(ignored) = &request.configuration.ignore_workspaces {
        let (patterns, errors) = Patterns::new(&ignored.0, request.mode, false);
        for error in errors {
            capture.gap("ignored workspaces", error, None);
        }
        excluded.extend(
            capture
                .inventory
                .workspaces
                .iter()
                .filter(|w| patterns.matches(&w.path))
                .map(|w| w.workspace),
        );
    }
    for workspace in &excluded {
        capture.gap(
            "analysis boundary",
            format!(
                "Excluded consumer {}",
                capture.inventory.workspaces[workspace.0 as usize].path
            ),
            None,
        );
    }
    capture.inventory.analysis_scope = Some(
        capture
            .inventory
            .workspaces
            .iter()
            .filter(|workspace| !excluded.contains(&workspace.workspace))
            .map(|workspace| workspace.workspace)
            .collect(),
    );
    let gitignore = gitignores(&mut capture);
    let source_paths: Vec<_> = capture
        .paths
        .keys()
        .filter(|path| paths::is_source(path))
        .cloned()
        .collect();
    let project_patterns: Vec<_> = packages
        .iter()
        .map(|package| {
            let mut values = package.settings.project_patterns();
            if request.mode != AnalysisMode::Default {
                let entries = package.settings.entry_patterns();
                if values.is_empty() {
                    values = entries;
                } else {
                    values.extend(entries.into_iter().filter(|value| {
                        !biome_configuration::project_analysis::ProjectPattern::new(value)
                            .production
                    }));
                }
            }
            capture.patterns(&values, request.mode, true, "project")
        })
        .collect();
    for path in &source_paths {
        if let Some(owner) = owner(&packages, path) {
            let workspace = WorkspaceId(owner as u32);
            if excluded.contains(&workspace) {
                continue;
            }
            let package = &packages[owner];
            let relative_path = relative(&package.path, path);
            let candidate =
                project_patterns[owner].matches(&relative_path) && !ignored(&gitignore, path);
            let file = capture.paths[path];
            let aliases = aliases_by_file.remove(&file).unwrap_or_default();
            capture.inventory.files.push(FileRecord {
                file,
                workspace,
                role: if candidate {
                    FileRole::ReportingCandidate
                } else {
                    FileRole::Context
                },
                aliases,
                program: false,
            });
        }
    }
    for (index, package) in packages.iter().enumerate() {
        let workspace = WorkspaceId(index as u32);
        retain_dependencies(&mut capture, workspace, package);
        if excluded.contains(&workspace) {
            continue;
        }
        if request.configuration.use_tsconfig_files == Some(true) && package.config.exists {
            for file in capture
                .inventory
                .files
                .iter_mut()
                .filter(|f| f.workspace == workspace)
            {
                file.role = FileRole::Context;
            }
        }
        let entry_values = package.settings.entry_patterns();
        let entry_patterns = capture.patterns(&entry_values, request.mode, true, "entry");
        let production_entries =
            capture.patterns(&entry_values, AnalysisMode::Production, true, "entry");
        for value in &entry_values {
            let pattern = biome_configuration::project_analysis::ProjectPattern::new(value);
            if pattern.negated
                || pattern.pattern.contains(['*', '{', '?', '['])
                || (request.mode != AnalysisMode::Default && !pattern.production)
            {
                continue;
            }
            let path = paths::join(&package.path, &pattern.pattern);
            if !fs.path_is_file(&path) {
                capture.gap("entry", format!("Missing entry {path}"), None);
            } else if !paths::is_source(&path) {
                let file = capture.capture(&path);
                capture.inventory.gaps.push(CoverageGap {
                    producer: "entry".into(),
                    domain: TargetDomain::Unbounded,
                    reason: GapReason::Capability(format!("Unsupported entry format {path}")),
                    location: Some(Location {
                        file,
                        range: TextRange::default(),
                    }),
                });
            }
        }
        let ordered_patterns: Vec<_> = entry_values
            .iter()
            .map(|value| capture.patterns(std::slice::from_ref(value), request.mode, true, "entry"))
            .collect();
        let mut ordered_aliases: Vec<_> = discovered.iter().collect();
        ordered_aliases.sort_by_key(|alias| {
            ordered_patterns
                .iter()
                .position(|pattern| pattern.matches(&relative(&package.path, alias)))
                .unwrap_or(usize::MAX)
        });
        for alias in ordered_aliases {
            if paths::is_source(alias)
                && owner(&packages, alias) == Some(index)
                && entry_patterns.matches(&relative(&package.path, alias))
                && let Ok(path) = paths::canonical(fs, alias)
            {
                let target_workspace =
                    owner(&packages, &path).map_or(workspace, |owner| WorkspaceId(owner as u32));
                if !excluded.contains(&target_workspace) {
                    let role = if production_entries.matches(&relative(&package.path, alias)) {
                        RootRole::Production
                    } else {
                        RootRole::Default
                    };
                    add_root(
                        &mut capture,
                        &path,
                        target_workspace,
                        "configuration entry",
                        None,
                        role,
                        ExportProtection::IncludeWhenRequested,
                    );
                }
            }
        }
        if let Some(public) = &request.configuration.public_entries {
            let patterns = capture.patterns(&public.0, request.mode, false, "public entry");
            for path in &source_paths {
                if owner(&packages, path) == Some(index)
                    && patterns.matches(&relative(&package.path, path))
                {
                    add_root(
                        &mut capture,
                        path,
                        workspace,
                        "public entry",
                        None,
                        RootRole::Both,
                        ExportProtection::Protected,
                    );
                }
            }
        }
        let selected = tsconfig::selected_files(
            &mut capture,
            &package.config,
            request.configuration.use_tsconfig_files.unwrap_or(false),
            &source_paths,
        );
        for (path, location) in selected {
            let declaration = paths::is_declaration(&path);
            let path = match paths::canonical(fs, &path) {
                Ok(path) => path,
                Err(error) => {
                    capture.gap("path identity", error, Some(location));
                    continue;
                }
            };
            if owner(&packages, &path) != Some(index) {
                continue;
            }
            if declaration {
                let mapped = mapped_patterns(&mut capture, &path, package, &request.configuration);
                let patterns =
                    capture.patterns(&mapped, request.mode, false, "tsconfig declaration");
                let matches: Vec<_> = source_paths
                    .iter()
                    .filter(|candidate| {
                        patterns.matches(candidate) && owner(&packages, candidate) == Some(index)
                    })
                    .collect();
                if matches.len() > 1 {
                    capture.gap(
                        "source mapping",
                        format!("Ambiguous declaration mapping for {path}"),
                        Some(location),
                    );
                }
                if matches.is_empty() {
                    capture.gap(
                        "tsconfig declaration",
                        format!("Missing declaration source for {path}"),
                        Some(location),
                    );
                }
                for source in matches {
                    if ignored(&gitignore, source) {
                        continue;
                    }
                    add_root(
                        &mut capture,
                        source,
                        workspace,
                        "tsconfig declaration",
                        Some(location),
                        RootRole::Production,
                        ExportProtection::IncludeWhenRequested,
                    );
                }
            } else if request.configuration.use_tsconfig_files == Some(true)
                && !ignored(&gitignore, &path)
            {
                add_root(
                    &mut capture,
                    &path,
                    workspace,
                    "tsconfig program",
                    Some(location),
                    RootRole::Both,
                    ExportProtection::Unprotected,
                );
                if let Some(file) = capture
                    .inventory
                    .files
                    .iter_mut()
                    .find(|file| capture.paths.get(&path) == Some(&file.file))
                {
                    file.program = true;
                }
            }
        }
        let entries = manifest_entries(&package.metadata);
        let hidden = hidden_exports(&package.metadata, &entries);
        for (specifier, range) in &entries {
            let target = paths::join(&package.path, &paths::expand_export(specifier));
            let target = paths::canonical(fs, &target).unwrap_or_else(|error| {
                capture.gap("path identity", error, None);
                target
            });
            let target_owner = owner(&packages, &target).unwrap_or(index);
            let mapped = mapped_patterns(
                &mut capture,
                &target,
                &packages[target_owner],
                &request.configuration,
            );
            let patterns = capture.patterns(&mapped, request.mode, false, "manifest entry");
            let mut matched = Vec::new();
            for path in &source_paths {
                if !patterns.matches(path) {
                    continue;
                }
                matched.push(path.clone());
                if ignored(&gitignore, path) {
                    continue;
                }
                let hidden = hidden.iter().any(|hidden| {
                    let target = paths::join(&package.path, &paths::expand_export(hidden));
                    let mapped = mapped_patterns(
                        &mut capture,
                        &target,
                        &packages[target_owner],
                        &request.configuration,
                    );
                    capture
                        .patterns(&mapped, request.mode, false, "null exports")
                        .matches(path)
                });
                if hidden {
                    continue;
                }
                let Some(file_owner) = owner(&packages, path) else {
                    continue;
                };
                if excluded.contains(&WorkspaceId(file_owner as u32)) {
                    continue;
                }
                add_root(
                    &mut capture,
                    path,
                    WorkspaceId(file_owner as u32),
                    "manifest entry",
                    Some(Location {
                        file: package.file,
                        range: *range,
                    }),
                    RootRole::Production,
                    ExportProtection::IncludeWhenRequested,
                );
            }
            if matched.is_empty()
                && !specifier.contains('*')
                && !is_bare_dependency(package, specifier)
            {
                capture.gap(
                    "manifest entry",
                    format!("Missing entry {specifier}"),
                    Some(Location {
                        file: package.file,
                        range: *range,
                    }),
                );
            }
            if !specifier.contains('*') && matched.len() > 1 {
                capture.gap(
                    "source mapping",
                    format!("Ambiguous source mapping for {specifier}"),
                    Some(Location {
                        file: package.file,
                        range: *range,
                    }),
                );
            }
        }
    }
    if let Some(root_index) = packages.iter().position(|p| p.path == root) {
        retain_catalogs(
            &mut capture,
            root_index,
            &packages[root_index],
            yaml.as_ref(),
        );
    }
    retain_configured_uses(&mut capture, request);
    super::dependencies::select_outputs(&mut capture);
    let mut roots = Vec::new();
    for root in std::mem::take(&mut capture.inventory.roots) {
        if !roots.contains(&root) {
            roots.push(root);
        }
    }
    capture.inventory.roots = roots;
    capture.inventory.files.sort_by_key(|file| file.file);
    capture
        .inventory
        .gaps
        .sort_by_cached_key(|gap| serde_json::to_string(gap).unwrap());
    capture.inventory.gaps.dedup();
    Ok(Discovery {
        inventory: capture.inventory,
        mappings: packages
            .into_iter()
            .map(|package| (package.path, package.config.mappings))
            .collect(),
    })
}

fn relative(root: &Utf8Path, path: &Utf8Path) -> Utf8PathBuf {
    let relative = path.strip_prefix(root).unwrap_or(path);
    if relative.as_str().is_empty() {
        ".".into()
    } else {
        relative.into()
    }
}
fn owner(packages: &[Package], path: &Utf8Path) -> Option<usize> {
    packages
        .iter()
        .enumerate()
        .filter(|(_, package)| path.starts_with(&package.path))
        .max_by_key(|(_, package)| package.path.components().count())
        .map(|(index, _)| index)
}
fn workspace_settings(
    config: &ProjectAnalysisConfiguration,
    path: &Utf8Path,
) -> WorkspaceAnalysisConfiguration {
    if let Some(workspaces) = &config.workspaces {
        let mut matching: Vec<_> = workspaces
            .iter()
            .filter(|(pattern, _)| {
                pattern
                    .replace('\\', "/")
                    .parse::<biome_glob::NormalizedGlob>()
                    .is_ok_and(|pattern| pattern.is_raw_match(path))
            })
            .collect();
        matching.sort_by(|(left, _), (right, _)| {
            let left: Vec<_> = left.split('/').collect();
            let right: Vec<_> = right.split('/').collect();
            left.len().cmp(&right.len()).then_with(|| {
                left.iter()
                    .zip(&right)
                    .map(|(l, r)| (!l.contains('*')).cmp(&!r.contains('*')))
                    .find(|order| !order.is_eq())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
        });
        matching
            .last()
            .map(|(_, settings)| (*settings).clone())
            .unwrap_or_default()
    } else if path == Utf8Path::new(".") {
        WorkspaceAnalysisConfiguration {
            entry: config.entry.clone(),
            project: config.project.clone(),
            paths: config.paths.clone(),
            ignore: config.ignore.clone(),
            ignore_files: config.ignore_files.clone(),
            ignore_binaries: config.ignore_binaries.clone(),
            ignore_global_binaries: config.ignore_global_binaries,
            ignore_dependencies: config.ignore_dependencies.clone(),
            ignore_members: config.ignore_members.clone(),
            ignore_unresolved: config.ignore_unresolved.clone(),
            ignore_exports_used_in_file: config.ignore_exports_used_in_file.clone(),
            ignore_issues: config.ignore_issues.clone(),
            include_entry_exports: config.include_entry_exports,
        }
    } else {
        WorkspaceAnalysisConfiguration::default()
    }
}
fn string(value: &MetadataValue) -> Option<&str> {
    value.value.as_string().map(|value| value.as_str())
}
fn strings(value: &MetadataValue) -> Vec<String> {
    value
        .elements
        .iter()
        .filter_map(string)
        .map(str::to_owned)
        .collect()
}
fn leaves(value: &MetadataValue) -> Vec<(&str, TextRange)> {
    if let Some(value_string) = string(value) {
        return vec![(value_string, value.range)];
    }
    value
        .members
        .values()
        .chain(value.elements.iter())
        .flat_map(leaves)
        .collect()
}
fn manifest_entries(metadata: &ProjectMetadata) -> Vec<(String, TextRange)> {
    let mut entries = Vec::new();
    for key in ["main", "module", "browser", "bin", "types", "typings"] {
        if let Some(value) = metadata.fields.get(key) {
            if key == "bin" {
                entries.extend(
                    leaves(value)
                        .into_iter()
                        .filter(|(value, _)| !value.is_empty())
                        .map(|(value, range)| (value.to_string(), range)),
                );
            } else if let Some(text) = string(value).filter(|text| !text.is_empty()) {
                entries.push((text.to_string(), value.range));
            }
        }
    }
    if let Some(exports) = metadata.fields.get("exports") {
        entries.extend(
            leaves(exports)
                .into_iter()
                .filter(|(value, _)| *value != "./*" && !value.trim().is_empty())
                .map(|(value, range)| (value.to_string(), range)),
        );
    }
    if let Some(imports) = &metadata.imports {
        for (key, value) in &imports.members {
            if key.starts_with('#') {
                entries.extend(
                    leaves(value)
                        .into_iter()
                        .filter(|(value, _)| value.starts_with('.') && !value.contains('*'))
                        .map(|(value, range)| (value.to_string(), range)),
                );
            }
        }
    }
    entries
}
fn hidden_exports(metadata: &ProjectMetadata, entries: &[(String, TextRange)]) -> Vec<String> {
    let Some(exports) = metadata.fields.get("exports") else {
        return Vec::new();
    };
    let mut result = Vec::new();
    for (hidden, value) in &exports.members {
        if !value.value.is_null() || hidden == "." {
            continue;
        }
        for (key, target) in &exports.members {
            let Some((prefix, suffix)) = key.split_once('*') else {
                continue;
            };
            let replacement = if let Some((head, tail)) = hidden.split_once('*') {
                if !head.starts_with(prefix)
                    || (head.len() <= prefix.len() && hidden.len() <= key.len())
                {
                    continue;
                }
                if let Some(tail) = tail.strip_suffix(suffix) {
                    format!("{}*{}", &head[prefix.len()..], tail)
                } else if suffix.ends_with(tail) {
                    format!("{}*", &head[prefix.len()..])
                } else {
                    continue;
                }
            } else {
                let Some(middle) = hidden
                    .strip_prefix(prefix)
                    .and_then(|v| v.strip_suffix(suffix))
                else {
                    continue;
                };
                middle.to_string()
            };
            for (target, _) in leaves(target) {
                if !target.contains('*') {
                    continue;
                }
                let hidden = target.replace('*', &replacement);
                let (pattern, _) = Patterns::new(
                    &[paths::expand_export(&hidden)],
                    AnalysisMode::Default,
                    false,
                );
                if !entries.iter().any(|(entry, _)| {
                    !entry.contains('*')
                        && pattern.matches(Utf8Path::new(entry.trim_start_matches("./")))
                }) {
                    result.push(hidden);
                }
            }
        }
    }
    result
}
fn mapped_patterns(
    capture: &mut Capture,
    target: &Utf8Path,
    package: &Package,
    config: &ProjectAnalysisConfiguration,
) -> Vec<String> {
    let mut pairs = package.config.mappings.clone();
    if let Some(mappings) = &config.source_mappings {
        pairs.extend(mappings.iter().map(|mapping| {
            (
                paths::join(&package.path, &mapping.output),
                paths::join(&package.path, &mapping.source),
            )
        }));
    }
    if pairs.is_empty() {
        let source = if capture.fs.path_is_dir(&package.path.join("src")) {
            package.path.join("src")
        } else {
            package.path.clone()
        };
        pairs.push((source.clone(), source));
    }
    pairs.sort();
    pairs.dedup();
    let depth = pairs
        .iter()
        .filter(|(output, _)| target.starts_with(output))
        .map(|(output, _)| output.components().count())
        .max();
    let matches: Vec<_> = pairs
        .iter()
        .filter(|(output, _)| {
            target.starts_with(output) && Some(output.components().count()) == depth
        })
        .collect();
    if matches.len() > 1 {
        capture.gap(
            "source mapping",
            format!("Ambiguous output directory for {target}"),
            None,
        );
    }
    if matches.is_empty() {
        return vec![target.to_string()];
    }
    matches
        .into_iter()
        .map(|(output, source)| {
            if output == source {
                target.to_string()
            } else {
                paths::source_pattern(&source.join(target.strip_prefix(output).unwrap()))
            }
        })
        .collect()
}
fn add_root(
    capture: &mut Capture,
    path: &Utf8Path,
    workspace: WorkspaceId,
    producer: &str,
    location: Option<Location>,
    role: RootRole,
    exports: ExportProtection,
) {
    let file = capture.capture(path);
    if let Some(record) = capture
        .inventory
        .files
        .iter_mut()
        .find(|record| record.file == file)
    {
        record.role = FileRole::ReportingCandidate;
    } else {
        capture.inventory.files.push(FileRecord {
            file,
            workspace,
            role: FileRole::ReportingCandidate,
            aliases: vec![relative(&capture.root, path)],
            program: false,
        });
    }
    capture.inventory.roots.push(Root {
        file,
        workspace,
        producer: producer.into(),
        location,
        role,
        exports,
    });
}
#[derive(Eq, PartialEq)]
enum WorkspaceYamlKind {
    Pnpm,
    Yarn,
}
struct WorkspaceYaml {
    file: FileId,
    metadata: WorkspaceMetadata,
    kind: WorkspaceYamlKind,
}
fn load_yaml(capture: &mut Capture, path: &Utf8Path, kind: WorkspaceYamlKind) -> WorkspaceYaml {
    let file = capture.capture(path);
    let metadata = capture
        .text(file)
        .map(read_workspace_metadata)
        .unwrap_or_default();
    capture.diagnostics(file, &metadata.diagnostics);
    WorkspaceYaml {
        file,
        metadata,
        kind,
    }
}
fn owned(value: &MetadataValue, file: FileId) -> MetadataValueRecord {
    MetadataValueRecord {
        value: json_value(value),
        location: Location {
            file,
            range: value.range,
        },
        members: value
            .members
            .iter()
            .map(|(key, value)| (key.clone(), owned(value, file)))
            .collect(),
        elements: value
            .elements
            .iter()
            .map(|value| owned(value, file))
            .collect(),
    }
}
fn retain_dependencies(capture: &mut Capture, workspace: WorkspaceId, package: &Package) {
    for (field, kind) in [
        ("dependencies", DependencyKind::Production),
        ("devDependencies", DependencyKind::Development),
        ("peerDependencies", DependencyKind::Peer),
        ("optionalDependencies", DependencyKind::Optional),
        ("resolutions", DependencyKind::Catalog),
    ] {
        if let Some(values) = package.metadata.fields.get(field) {
            for (name, value) in &values.members {
                let optional = package
                    .metadata
                    .fields
                    .get("peerDependenciesMeta")
                    .and_then(|v| v.members.get(name))
                    .and_then(|v| v.members.get("optional"))
                    .is_some_and(|v| v.value.as_bool() == Some(true));
                let location = Location {
                    file: package.file,
                    range: value.range,
                };
                capture.inventory.declarations.push(DependencyDeclaration {
                    package: name.clone(),
                    workspace,
                    location,
                    kind: if kind == DependencyKind::Peer && optional {
                        DependencyKind::OptionalPeer
                    } else {
                        kind
                    },
                    specifier: string(value).unwrap_or_default().into(),
                    field: field.into(),
                });
                capture.inventory.workspaces[workspace.0 as usize]
                    .declaration_locations
                    .push(location);
            }
        }
    }
    if let Some(overrides) = package
        .metadata
        .fields
        .get("pnpm")
        .and_then(|value| value.members.get("overrides"))
    {
        retain_overrides(
            capture,
            workspace,
            package.file,
            "pnpm.overrides",
            overrides,
        );
    }
    if let Some(imports) = &package.metadata.imports {
        for (key, value) in &imports.members {
            if !key.starts_with('#') {
                continue;
            }
            for (specifier, range) in leaves(value) {
                if specifier.starts_with(['.', '!', '#']) {
                    continue;
                }
                let dependency_package = package_name(specifier);
                capture.inventory.workspaces[workspace.0 as usize]
                    .dependency_uses
                    .push(ManifestDependencyUse {
                        package: dependency_package,
                        specifier: specifier.into(),
                        producer: "manifest imports".into(),
                        location: Some(Location {
                            file: package.file,
                            range,
                        }),
                    });
            }
        }
    }
}
fn package_name(specifier: &str) -> String {
    specifier
        .split('/')
        .take(if specifier.starts_with('@') { 2 } else { 1 })
        .collect::<Vec<_>>()
        .join("/")
}
fn is_bare_dependency(package: &Package, specifier: &str) -> bool {
    !specifier.starts_with('.')
        && [
            "dependencies",
            "devDependencies",
            "peerDependencies",
            "optionalDependencies",
        ]
        .iter()
        .any(|field| {
            package
                .metadata
                .fields
                .get(*field)
                .is_some_and(|value| value.members.contains_key(&package_name(specifier)))
        })
}
fn retain_catalogs(
    capture: &mut Capture,
    index: usize,
    package: &Package,
    yaml: Option<&WorkspaceYaml>,
) {
    let container = yaml.map_or(package.file, |yaml| yaml.file);
    if let Some(yaml) = yaml
        && capture.text(yaml.file).is_none()
    {
        for field in ["catalog", "catalogs"] {
            capture.inventory.catalog_selections.push(CatalogSelection {
                field: field.into(),
                location: Location {
                    file: yaml.file,
                    range: TextRange::default(),
                },
                container: Location {
                    file: container,
                    range: TextRange::default(),
                },
                valid: false,
            });
        }
        return;
    }
    if let Some(yaml) = yaml
        && yaml.kind == WorkspaceYamlKind::Pnpm
        && let Some(overrides) = yaml.metadata.fields.get("overrides")
    {
        retain_overrides(
            capture,
            WorkspaceId(index as u32),
            yaml.file,
            "overrides",
            overrides,
        );
    }
    for key in ["catalog", "catalogs"] {
        let provider = yaml
            .and_then(|yaml| {
                yaml.metadata
                    .fields
                    .get(key)
                    .filter(|v| !v.value.is_null())
                    .map(|value| (yaml.file, value))
            })
            .or_else(|| {
                package
                    .metadata
                    .fields
                    .get(key)
                    .filter(|v| !v.value.is_null())
                    .map(|v| (package.file, v))
            })
            .or_else(|| {
                package
                    .metadata
                    .fields
                    .get("workspaces")
                    .and_then(|v| v.members.get(key))
                    .filter(|v| !v.value.is_null())
                    .map(|v| (package.file, v))
            });
        let metadata_valid = yaml.is_none_or(|yaml| yaml.metadata.diagnostics.is_empty())
            && package.diagnostics.is_empty();
        if let Some((file, value)) = provider {
            let valid = metadata_valid
                && value.value.as_object().is_some()
                && (key == "catalog"
                    || value
                        .members
                        .values()
                        .all(|v| v.value.as_object().is_some()));
            capture.inventory.catalog_selections.push(CatalogSelection {
                field: key.into(),
                location: Location {
                    file,
                    range: value.range,
                },
                container: Location {
                    file: container,
                    range: TextRange::default(),
                },
                valid,
            });
            let catalogs: Vec<_> = if key == "catalog" {
                vec![("", value)]
            } else {
                value
                    .members
                    .iter()
                    .map(|(name, value)| (name.as_str(), value))
                    .collect()
            };
            for (name, value) in catalogs {
                capture.inventory.workspaces[index]
                    .catalogs
                    .push(CatalogProvider {
                        name: name.into(),
                        container: Location {
                            file: container,
                            range: TextRange::default(),
                        },
                        value: owned(value, file),
                    });
            }
        } else if !metadata_valid {
            capture.inventory.catalog_selections.push(CatalogSelection {
                field: key.into(),
                location: Location {
                    file: container,
                    range: TextRange::default(),
                },
                container: Location {
                    file: container,
                    range: TextRange::default(),
                },
                valid: false,
            });
        }
    }
}
fn gitignores(capture: &mut Capture) -> Vec<ignore::gitignore::Gitignore> {
    let mut result = Vec::new();
    let files: Vec<_> = capture
        .paths
        .iter()
        .filter(|(path, _)| path.file_name() == Some(".gitignore"))
        .map(|(path, file)| (path.clone(), *file))
        .collect();
    for (path, file) in files {
        let mut builder = ignore::gitignore::GitignoreBuilder::new(path.parent().unwrap());
        if let Some(text) = capture.text(file) {
            let lines: Vec<_> = text.lines().map(str::to_owned).collect();
            for line in lines {
                if let Err(error) = builder.add_line(Some(path.as_std_path().to_path_buf()), &line)
                {
                    capture.gap(
                        "gitignore",
                        error.to_string(),
                        Some(Location {
                            file,
                            range: TextRange::default(),
                        }),
                    );
                }
            }
        }
        match builder.build() {
            Ok(ignore) => result.push(ignore),
            Err(error) => capture.gap("gitignore", error.to_string(), None),
        }
    }
    result
}
fn ignored(ignores: &[ignore::gitignore::Gitignore], path: &Utf8Path) -> bool {
    let mut ignored = false;
    for ignore in ignores {
        if !path.starts_with(ignore.path()) {
            continue;
        }
        let matched = ignore.matched_path_or_any_parents(path, false);
        if !matched.is_none() {
            ignored = matched.is_ignore();
        }
    }
    ignored
}

/// Resolves report selectors without changing the inventory or its coverage.
/// Positive selectors form a union, then all negative selectors are subtracted.
pub fn select_workspaces(
    root: &Utf8Path,
    workspaces: &[WorkspaceRecord],
    selectors: &[String],
) -> (Vec<WorkspaceId>, Vec<String>) {
    let mut errors = Vec::new();
    let has_positive = selectors.iter().any(|selector| !selector.starts_with('!'));
    let mut positive = Vec::new();
    let mut negative = Vec::new();
    for selector in selectors {
        let negated = selector.starts_with('!');
        let selector = selector.trim_start_matches('!').replace('\\', "/");
        let selector_path = Utf8Path::new(&selector);
        let selector = if selector_path.has_root() {
            if !selector_path.starts_with(root) {
                errors.push(format!(
                    "Workspace selector is outside the project: {selector}"
                ));
                continue;
            }
            relative(root, selector_path).to_string()
        } else {
            selector
        };
        match selector.parse::<biome_glob::NormalizedGlob>() {
            Ok(pattern) => {
                if negated {
                    negative.push(pattern)
                } else {
                    positive.push(pattern)
                }
            }
            Err(error) => errors.push(format!(
                "Unsupported workspace selector {selector}: {error}"
            )),
        }
    }
    let matches = |pattern: &biome_glob::NormalizedGlob, workspace: &WorkspaceRecord| {
        pattern.is_raw_match(&workspace.path)
            || workspace
                .name
                .as_ref()
                .is_some_and(|name| pattern.is_raw_match(name))
    };
    let selected = workspaces
        .iter()
        .filter(|workspace| {
            (!has_positive || positive.iter().any(|pattern| matches(pattern, workspace)))
                && !negative.iter().any(|pattern| matches(pattern, workspace))
        })
        .map(|workspace| workspace.workspace)
        .collect();
    (selected, errors)
}

fn json_value(value: &MetadataValue) -> serde_json::Value {
    if let Some(text) = string(value) {
        serde_json::Value::String(text.into())
    } else if let Some(value) = value.value.as_bool() {
        value.into()
    } else if let Some(value) = value.value.as_number() {
        serde_json::json!(value)
    } else if value.value.as_array().is_some() {
        serde_json::Value::Array(value.elements.iter().map(json_value).collect())
    } else if value.value.as_object().is_some() {
        serde_json::Value::Object(
            value
                .members
                .iter()
                .map(|(key, value)| (key.clone(), json_value(value)))
                .collect(),
        )
    } else {
        serde_json::Value::Null
    }
}

fn retain_overrides(
    capture: &mut Capture,
    workspace: WorkspaceId,
    file: FileId,
    field: &str,
    overrides: &MetadataValue,
) {
    for (package, value) in &overrides.members {
        if let Some(specifier) = string(value) {
            let location = Location {
                file,
                range: value.range,
            };
            capture.inventory.declarations.push(DependencyDeclaration {
                package: package.clone(),
                workspace,
                location,
                kind: DependencyKind::Catalog,
                specifier: specifier.into(),
                field: field.into(),
            });
            capture.inventory.workspaces[workspace.0 as usize]
                .declaration_locations
                .push(location);
        }
    }
}
fn retain_configured_uses(capture: &mut Capture, request: &ProjectAnalysisRequest) {
    for dependency in request.configuration.dependency_uses.iter().flatten() {
        let (workspaces, errors) = select_workspaces(
            &capture.root,
            &capture.inventory.workspaces,
            std::slice::from_ref(&dependency.workspace),
        );
        for error in errors {
            capture.gap("dependency use", error, None);
        }
        if workspaces.is_empty() {
            capture.gap(
                "dependency use",
                format!("Unknown workspace {}", dependency.workspace),
                None,
            );
        }
        let location = dependency.location.as_ref().and_then(|location| {
            location.source.as_ref().map(|source| {
                let path = paths::join(&capture.root, source);
                let file = capture.capture(&path);
                Location {
                    file,
                    range: location.range,
                }
            })
        });
        if location.is_none() {
            capture.gap(
                "dependency use location",
                format!("Source identity unavailable for {}", dependency.package),
                None,
            );
        }
        for workspace in workspaces {
            capture.inventory.workspaces[workspace.0 as usize]
                .dependency_uses
                .push(ManifestDependencyUse {
                    package: dependency.package.clone(),
                    specifier: dependency.package.clone(),
                    producer: dependency.reason.clone(),
                    location,
                });
        }
    }
}

#[cfg(test)]
#[path = "inventory.tests.rs"]
mod tests;

#[cfg(test)]
#[path = "b6.tests.rs"]
mod b6_tests;
