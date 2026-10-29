#[cfg(all(feature = "module_graph", feature = "lang_js"))]
use super::scripts;
use super::{inventory::Capture, paths};
#[cfg(all(feature = "module_graph", feature = "lang_js"))]
use biome_module_graph::{
    ProjectResolutionContext, ProjectResolutionPurpose, resolve_project_specifier,
};
use biome_project_analysis::{
    coverage::{CoverageGap, GapReason, TargetDomain},
    dependencies::PublicOutput,
    inventory::{FileRecord, FileRole, MetadataValueRecord, WorkspaceRecord},
    model::Location,
};
#[cfg(all(feature = "module_graph", feature = "lang_js"))]
use biome_project_analysis::{
    dependencies::{
        DependencyProducer, DependencyUse, InstalledPackage, PackageProvider, derived_dependencies,
        package_name, provider_manifests, provider_uncertain,
    },
    facts::ResolutionOutcome,
    model::{FileId, ModuleFacts, WorkspaceId},
    scripts::ScriptResolution,
};
use camino::{Utf8Path, Utf8PathBuf};
#[cfg(all(feature = "module_graph", feature = "lang_js"))]
use std::collections::{BTreeMap, BTreeSet, HashSet};

pub(super) fn select_outputs(capture: &mut Capture) {
    for marker in [".pnp.cjs", ".pnp.js", ".pnp.data.json"] {
        let path = capture.root.join(marker);
        if capture.fs.path_is_file(&path) {
            let file = capture.capture(&path);
            capture.inventory.gaps.push(CoverageGap {
                producer: "package installation".into(),
                domain: TargetDomain::Unbounded,
                reason: GapReason::Capability("Yarn PnP package resolution is unsupported".into()),
                location: Some(Location {
                    file,
                    range: Default::default(),
                }),
            });
        }
    }
    for workspace in capture.inventory.workspaces.clone() {
        if capture
            .inventory
            .analysis_scope
            .as_ref()
            .is_some_and(|scope| !scope.contains(&workspace.workspace))
        {
            continue;
        }
        if field(&workspace, "private").is_some_and(|v| v.value == true) {
            continue;
        }
        if let Some(directory) =
            field(&workspace, "publishConfig").and_then(|v| member(v, "directory"))
        {
            capture.inventory.gaps.push(CoverageGap {
                producer: "public declaration outputs".into(),
                domain: TargetDomain::Bounded {
                    files: Vec::new(),
                    workspaces: vec![workspace.workspace],
                    packages: Vec::new(),
                },
                reason: GapReason::Capability("publishConfig.directory is unsupported".into()),
                location: Some(directory.location),
            });
            continue;
        }
        let base = paths::join(&capture.root, workspace.path.as_str());
        let mut selected = Vec::new();
        for key in ["types", "typings", "main"] {
            if let Some(value) = published_field(&workspace, key)
                && let Some(target) = value.value.as_str()
                && let Some(target) = declaration_target(target)
            {
                let path = paths::join(&base, &target);
                if capture.fs.path_is_file(&path) {
                    selected.push((path, value.location));
                    break;
                }
            }
        }
        if selected.is_empty() && capture.fs.path_is_file(&base.join("index.d.ts")) {
            selected.push((
                base.join("index.d.ts"),
                Location {
                    file: workspace.manifest,
                    range: Default::default(),
                },
            ));
        }
        if let Some(versions) = published_field(&workspace, "typesVersions") {
            for leaf in leaves(versions) {
                if let Some(target) = leaf.value.as_str().and_then(|target| {
                    declaration_target(target).or_else(|| {
                        (local_target(target) && target.contains('*')).then(|| target.to_string())
                    })
                }) {
                    select_pattern(capture, &base, &target, leaf.location, &mut selected);
                }
            }
        }
        if let Some(exports) = published_field(&workspace, "exports") {
            let mut entries = Vec::new();
            export_leaves(exports, &mut entries);
            for value in entries {
                if let Some(target) = value.value.as_str().and_then(declaration_target) {
                    let mut matches = Vec::new();
                    select_pattern(capture, &base, &target, value.location, &mut matches);
                    for (path, location) in matches {
                        if !hidden_export(
                            exports,
                            &target,
                            path.strip_prefix(&base).unwrap_or(&path).as_str(),
                        ) {
                            selected.push((path, location));
                        }
                    }
                }
            }
        }
        for (path, location) in selected {
            let file = capture.capture(&path);
            let output = PublicOutput {
                file,
                workspace: workspace.workspace,
                location,
            };
            if !capture.inventory.public_outputs.contains(&output) {
                capture.inventory.public_outputs.push(output);
            }
            if !capture.inventory.files.iter().any(|r| r.file == file) {
                capture.inventory.files.push(FileRecord {
                    file,
                    workspace: workspace.workspace,
                    role: FileRole::Context,
                    aliases: Vec::new(),
                    program: false,
                });
            }
        }
    }
}
fn field<'a>(workspace: &'a WorkspaceRecord, key: &str) -> Option<&'a MetadataValueRecord> {
    workspace
        .metadata
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v)
}
fn member<'a>(value: &'a MetadataValueRecord, key: &str) -> Option<&'a MetadataValueRecord> {
    value.members.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}
fn published_field<'a>(
    workspace: &'a WorkspaceRecord,
    key: &str,
) -> Option<&'a MetadataValueRecord> {
    field(workspace, "publishConfig")
        .and_then(|p| member(p, key))
        .or_else(|| field(workspace, key))
}
fn leaves(value: &MetadataValueRecord) -> Vec<&MetadataValueRecord> {
    if value.value.is_string() {
        return vec![value];
    }
    value
        .members
        .iter()
        .map(|(_, v)| v)
        .chain(&value.elements)
        .flat_map(leaves)
        .collect()
}
fn export_leaves<'a>(value: &'a MetadataValueRecord, result: &mut Vec<&'a MetadataValueRecord>) {
    if value.value.is_string() {
        result.push(value);
        return;
    }
    let types: Vec<_> = value
        .members
        .iter()
        .filter(|(k, _)| k == "types" || k.starts_with("types@"))
        .collect();
    if types.is_empty() {
        for (_, v) in &value.members {
            export_leaves(v, result);
        }
        for v in &value.elements {
            export_leaves(v, result);
        }
    } else {
        for (_, v) in types {
            export_leaves(v, result);
        }
    }
}
fn local_target(target: &str) -> bool {
    !target.starts_with('/')
        && !target
            .split('/')
            .any(|p| matches!(p, ".." | "node_modules"))
}
fn declaration_target(target: &str) -> Option<String> {
    if !local_target(target) {
        return None;
    }
    if paths::is_declaration(Utf8Path::new(target)) {
        return Some(target.into());
    }
    for (extension, declaration) in [(".js", ".d.ts"), (".mjs", ".d.mts"), (".cjs", ".d.cts")] {
        if let Some(stem) = target.strip_suffix(extension) {
            return Some(format!("{stem}{declaration}"));
        }
    }
    None
}
fn select_pattern(
    capture: &mut Capture,
    base: &Utf8Path,
    target: &str,
    location: Location,
    result: &mut Vec<(Utf8PathBuf, Location)>,
) {
    if !target.contains('*') {
        let path = paths::join(base, target);
        if capture.fs.path_is_file(&path) {
            result.push((path, location));
        }
        return;
    }
    let target = target.trim_start_matches("./");
    for source in capture.inventory.sources.values() {
        let path = capture.root.join(&source.logical_path);
        if paths::is_declaration(&path)
            && let Ok(relative) = path.strip_prefix(base)
            && wildcard(target, relative.as_str()).is_some()
        {
            result.push((path, location));
        }
    }
}
fn wildcard<'a>(pattern: &str, value: &'a str) -> Option<&'a str> {
    let (prefix, suffix) = pattern.split_once('*')?;
    let rest = value.strip_prefix(prefix)?;
    if !suffix.contains('*') {
        return rest.strip_suffix(suffix);
    }
    for end in rest
        .char_indices()
        .map(|(i, _)| i)
        .chain(std::iter::once(rest.len()))
    {
        let replacement = &rest[..end];
        if pattern.replace('*', replacement) == value {
            return Some(replacement);
        }
    }
    None
}
fn hidden_export(exports: &MetadataValueRecord, target: &str, path: &str) -> bool {
    let Some(replacement) = wildcard(target.trim_start_matches("./"), path) else {
        return false;
    };
    let mut matched_alias = false;
    for (key, value) in &exports.members {
        let mut targets = Vec::new();
        export_leaves(value, &mut targets);
        if key.contains('*')
            && targets
                .iter()
                .any(|v| v.value.as_str().and_then(declaration_target).as_deref() == Some(target))
        {
            matched_alias = true;
            let subpath = key.replace('*', replacement);
            let best = exports
                .members
                .iter()
                .filter(|(k, _)| k == &subpath || wildcard(k, &subpath).is_some())
                .max_by_key(|(k, _)| {
                    (
                        usize::from(!k.contains('*')),
                        k.split('*').next().unwrap_or("").len(),
                        k.len(),
                    )
                });
            let hidden = best.is_some_and(|(best_key, value)| {
                if best_key == key {
                    return false;
                }
                let replacement = wildcard(best_key, &subpath).unwrap_or("");
                let mut selected = Vec::new();
                export_leaves(value, &mut selected);
                !selected
                    .iter()
                    .filter_map(|value| value.value.as_str().and_then(declaration_target))
                    .any(|target| target.replace('*', replacement).trim_start_matches("./") == path)
            });
            if !hidden {
                return false;
            }
        }
    }
    matched_alias
}

#[cfg(all(feature = "module_graph", feature = "lang_js"))]
pub(super) fn discover(
    capture: &mut Capture,
    modules: &BTreeMap<FileId, ModuleFacts>,
    db: &dyn biome_module_graph::ModuleDb,
) -> BTreeSet<Utf8PathBuf> {
    let mut pending = BTreeSet::new();
    capture.inventory.public_gaps.clear();
    let mut outputs = capture.inventory.public_outputs.clone();
    let mut visited = BTreeSet::new();
    let mut requirements = Vec::new();
    while let Some(output) = outputs.pop() {
        if !visited.insert(output.file) {
            continue;
        }
        let path = capture
            .root
            .join(&capture.inventory.sources[&output.file].logical_path);
        let Some(module) = modules.get(&output.file).and_then(|m| m.collected.as_ref()) else {
            capture.inventory.public_gaps.push(CoverageGap {
                producer: "public declaration acquisition".into(),
                domain: TargetDomain::Unbounded,
                reason: GapReason::UnavailableInput(
                    "Published declaration could not be collected".into(),
                ),
                location: Some(Location {
                    file: output.file,
                    range: Default::default(),
                }),
            });
            pending.insert(path);
            continue;
        };
        let Some(workspace) = capture
            .inventory
            .workspaces
            .iter()
            .find(|w| w.workspace == output.workspace)
            .cloned()
        else {
            continue;
        };
        let base = paths::join(&capture.root, workspace.path.as_str());
        if module.parse_status != biome_project_analysis::facts::ParseStatus::Valid
            || !module.unknown.is_empty()
        {
            capture.inventory.public_gaps.push(CoverageGap {
                producer: "public declaration requirements".into(),
                domain: TargetDomain::Unbounded,
                reason: GapReason::DynamicReference(
                    "Public declaration contains uncertain requirements".into(),
                ),
                location: Some(Location {
                    file: output.file,
                    range: Default::default(),
                }),
            });
        }
        for import in &module.imports {
            let context = ProjectResolutionContext {
                purpose: ProjectResolutionPurpose::PublicOutput,
                syntax: public_syntax(import.syntax),
                ..Default::default()
            };
            let resolved = resolve_project_specifier(
                db,
                path.parent().unwrap(),
                &import.specifier,
                context.clone(),
            );
            let resolution = scripts::convert(capture, &resolved, context);
            if matches!(resolution.outcome, ResolutionOutcome::Unknown { .. }) {
                capture.inventory.public_gaps.push(CoverageGap {
                    producer: "public declaration resolution".into(),
                    domain: TargetDomain::Unbounded,
                    reason: GapReason::UnavailableInput(
                        "Published requirement has incomplete resolution metadata".into(),
                    ),
                    location: Some(import.specifier_location),
                });
            }
            if let ResolutionOutcome::Resolved { file, .. } = resolution.outcome {
                let target = capture
                    .root
                    .join(&capture.inventory.sources[&file].logical_path);
                if target.starts_with(&base)
                    && paths::is_declaration(&target)
                    && !target.components().any(|p| p.as_str() == "node_modules")
                {
                    if !capture.inventory.files.iter().any(|r| r.file == file) {
                        capture.inventory.files.push(FileRecord {
                            file,
                            workspace: workspace.workspace,
                            role: FileRole::Context,
                            aliases: Vec::new(),
                            program: false,
                        });
                    }
                    outputs.push(PublicOutput {
                        file,
                        ..output.clone()
                    });
                    continue;
                }
            }
            if matches!(resolution.outcome, ResolutionOutcome::Builtin { .. }) {
                continue;
            }
            let Some(package) = package_name(&import.specifier) else {
                continue;
            };
            let mut requirement = DependencyUse {
                reason: biome_project_analysis::dependencies::DependencyReason::Direct,
                workspace: output.workspace,
                location: import.specifier_location,
                package: package.into(),
                specifier: import.specifier.clone(),
                producer: DependencyProducer::PublicOutput { file: output.file },
                definite: true,
                optional: import.optional,
                type_only: true,
                resolutions: vec![resolution],
                binary_providers: Vec::new(),
            };
            if let Some(name) = requirement
                .resolutions
                .iter()
                .flat_map(|r| &r.trace)
                .filter_map(|e| {
                    if let biome_project_analysis::facts::ResolutionEvent::Package {
                        name: Some(name),
                        ..
                    } = e
                    {
                        Some(name)
                    } else {
                        None
                    }
                })
                .find(|name| name.starts_with("@types/"))
            {
                requirement.package.clone_from(name);
            }
            let bundled = requirement
                .resolutions
                .iter()
                .any(|r| public_bundled(capture, r));
            if !bundled && !package.starts_with("@types/") {
                let companion = biome_project_analysis::dependencies::type_companion(package);
                let context = ProjectResolutionContext {
                    purpose: ProjectResolutionPurpose::PublicOutput,
                    syntax: public_syntax(import.syntax),
                    ..Default::default()
                };
                let resolved = resolve_project_specifier(
                    db,
                    path.parent().unwrap(),
                    &companion,
                    context.clone(),
                );
                let resolution = scripts::convert(capture, &resolved, context);
                if matches!(resolution.outcome, ResolutionOutcome::Resolved { .. })
                    || capture
                        .inventory
                        .declarations
                        .iter()
                        .any(|d| d.workspace == output.workspace && d.package == companion)
                {
                    requirement.package = companion;
                    requirement.resolutions.push(resolution);
                }
            }
            requirements.push(requirement);
        }
    }
    capture.inventory.public_requirements = requirements;
    capture.inventory.package_providers.clear();
    let mut owners = BTreeSet::new();
    for declaration in capture.inventory.declarations.clone() {
        if declaration.kind == biome_project_analysis::inventory::DependencyKind::Catalog
            || !owners.insert((declaration.workspace, declaration.package.clone()))
        {
            continue;
        }
        let Some(workspace) = capture
            .inventory
            .workspaces
            .iter()
            .find(|w| w.workspace == declaration.workspace)
        else {
            continue;
        };
        let base = paths::join(&capture.root, workspace.path.as_str());
        let context = ProjectResolutionContext::default();
        let resolved = resolve_project_specifier(db, &base, &declaration.package, context.clone());
        let before = capture.inventory.gaps.len();
        let resolution = scripts::convert(capture, &resolved, context);
        capture.inventory.gaps.truncate(before);
        capture.inventory.package_providers.push(
            biome_project_analysis::dependencies::PackageProvider {
                workspace: declaration.workspace,
                consumer: None,
                requested_package: declaration.package,
                resolutions: vec![resolution],
            },
        );
    }
    let mut consumers = Vec::new();
    for (file, module) in modules {
        let Some(module) = &module.collected else {
            continue;
        };
        let Some(record) = capture
            .inventory
            .files
            .iter()
            .find(|record| record.file == *file)
        else {
            continue;
        };
        let workspace = record.workspace;
        let path = capture
            .root
            .join(&capture.inventory.sources[file].logical_path);
        if path
            .components()
            .any(|part| part.as_str() == "node_modules")
        {
            continue;
        }
        for import in &module.imports {
            let Some(package) = package_name(&import.specifier) else {
                continue;
            };
            let resolutions: Vec<_> = module
                .resolutions
                .iter()
                .filter(|r| r.occurrence == import.ordinal)
                .map(|r| ScriptResolution {
                    context: r.context.clone(),
                    outcome: r.outcome.clone(),
                    trace: r.trace.clone(),
                })
                .collect();
            let effective = if import.type_only || module.declaration_file.is_some() {
                resolutions
                    .iter()
                    .filter(|r| {
                        r.context.purpose
                            == biome_project_analysis::facts::ResolutionPurpose::Declaration
                    })
                    .flat_map(|r| &r.trace)
                    .find_map(|e| match e {
                        biome_project_analysis::facts::ResolutionEvent::Package {
                            name: Some(name),
                            ..
                        } if name.starts_with("@types/") => Some(name.as_str()),
                        _ => None,
                    })
                    .unwrap_or(package)
            } else {
                package
            };
            consumers.push(ProviderConsumer {
                workspace,
                location: import.specifier_location,
                base: path.parent().unwrap().to_path_buf(),
                package: effective.to_string(),
                resolutions,
            });
        }
    }
    for requirement in capture.inventory.public_requirements.clone() {
        let path = capture
            .root
            .join(&capture.inventory.sources[&requirement.location.file].logical_path);
        consumers.push(ProviderConsumer {
            workspace: requirement.workspace,
            location: requirement.location,
            base: path.parent().unwrap().to_path_buf(),
            package: requirement.package,
            resolutions: requirement.resolutions,
        });
    }
    consumer_providers(capture, db, consumers);
    pending
}

#[cfg(all(feature = "module_graph", feature = "lang_js"))]
fn public_syntax(
    syntax: biome_project_analysis::facts::ImportSyntax,
) -> biome_module_graph::ProjectImportSyntax {
    use biome_module_graph::ProjectImportSyntax as Graph;
    use biome_project_analysis::facts::ImportSyntax as Owned;
    match syntax {
        Owned::Import => Graph::Import,
        Owned::Require => Graph::Require,
        Owned::DynamicImport => Graph::DynamicImport,
        Owned::ImportType => Graph::ImportType,
        Owned::Reexport => Graph::Reexport,
        Owned::RequireResolve => Graph::RequireResolve,
        Owned::ImportMetaResolve => Graph::ImportMetaResolve,
        Owned::Url => Graph::Url,
        Owned::Worker => Graph::Worker,
        Owned::NodeEntry => Graph::NodeEntry,
    }
}
#[cfg(all(feature = "module_graph", feature = "lang_js"))]
fn public_bundled(
    capture: &Capture,
    resolution: &biome_project_analysis::scripts::ScriptResolution,
) -> bool {
    let ResolutionOutcome::Resolved { file, .. } = resolution.outcome else {
        return false;
    };
    if capture
        .inventory
        .sources
        .get(&file)
        .is_some_and(|s| paths::is_declaration(&s.logical_path))
    {
        return true;
    }
    let manifest = resolution.trace.iter().rev().find_map(|event| {
        if let biome_project_analysis::facts::ResolutionEvent::Manifest { file, .. } = event {
            capture
                .inventory
                .sources
                .get(file)
                .filter(|s| s.logical_path.file_name() == Some("package.json"))
                .map(|s| (*file, s))
        } else {
            None
        }
    });
    let Some((file, source)) = manifest else {
        return false;
    };
    if let Some(text) = capture.text(file) {
        let parsed = biome_json_parser::parse_json(text, Default::default());
        let metadata = biome_package::read_project_metadata(&parsed.tree());
        if ["types", "typings", "typesVersions"].iter().any(|key| {
            metadata
                .fields
                .get(*key)
                .is_some_and(|v| !v.value.is_null())
        }) {
            return true;
        }
    }
    let path = capture.root.join(&source.logical_path);
    path.parent()
        .is_some_and(|base| capture.fs.path_is_file(&base.join("index.d.ts")))
}

#[cfg(all(feature = "module_graph", feature = "lang_js"))]
struct ProviderConsumer {
    workspace: WorkspaceId,
    location: Location,
    base: Utf8PathBuf,
    package: String,
    resolutions: Vec<ScriptResolution>,
}

#[cfg(all(feature = "module_graph", feature = "lang_js"))]
fn consumer_providers(
    capture: &mut Capture,
    db: &dyn biome_module_graph::ModuleDb,
    consumers: Vec<ProviderConsumer>,
) {
    let mut declarations: BTreeMap<_, BTreeSet<_>> = BTreeMap::new();
    for declaration in &capture.inventory.declarations {
        if declaration.kind != biome_project_analysis::inventory::DependencyKind::Catalog {
            declarations
                .entry(declaration.workspace)
                .or_default()
                .insert(declaration.package.clone());
        }
    }
    let eligible: BTreeMap<_, BTreeSet<_>> = capture
        .inventory
        .workspaces
        .iter()
        .map(|consumer| {
            let packages = capture
                .inventory
                .workspaces
                .iter()
                .filter(|owner| {
                    owner.path.as_str() == "." || consumer.path.starts_with(&owner.path)
                })
                .filter_map(|owner| declarations.get(&owner.workspace))
                .flatten()
                .cloned()
                .collect();
            (consumer.workspace, packages)
        })
        .collect();
    let mut requested = HashSet::new();
    let mut records = BTreeMap::new();
    let mut metadata = BTreeMap::new();
    for consumer in consumers {
        let Some(eligible) = eligible.get(&consumer.workspace) else {
            continue;
        };
        let mut unique_contexts = HashSet::new();
        let contexts: Vec<_> = consumer
            .resolutions
            .iter()
            .map(|r| provider_context(&r.context))
            .filter(|context| unique_contexts.insert(context.clone()))
            .collect();
        let mut pending = vec![(consumer.package, consumer.resolutions)];
        let mut expanded = BTreeSet::new();
        while let Some((package, resolutions)) = pending.pop() {
            if !expanded.insert(package.clone()) {
                continue;
            }
            let provider = provider_manifests(&package, &resolutions)
                .into_iter()
                .rev()
                .find_map(|file| {
                    metadata
                        .entry(file)
                        .or_insert_with(|| installed_metadata(capture, file))
                        .clone()
                });
            if provider_uncertain(provider.as_ref(), &resolutions) {
                continue;
            }
            for (package, _) in derived_dependencies(&package, provider.as_ref()) {
                if !eligible.contains(&package) {
                    continue;
                }
                let key = (
                    consumer.workspace,
                    consumer.location.file,
                    u32::from(consumer.location.range.start()),
                    u32::from(consumer.location.range.end()),
                    package.clone(),
                );
                for context in &contexts {
                    if !requested.insert((key.clone(), context.clone())) {
                        continue;
                    }
                    let resolved =
                        resolve_project_specifier(db, &consumer.base, &package, context.clone());
                    let before = capture.inventory.gaps.len();
                    let resolution = scripts::convert(capture, &resolved, context.clone());
                    capture.inventory.gaps.truncate(before);
                    records
                        .entry(key.clone())
                        .or_insert_with(|| PackageProvider {
                            workspace: consumer.workspace,
                            consumer: Some(consumer.location),
                            requested_package: package.clone(),
                            resolutions: Vec::new(),
                        })
                        .resolutions
                        .push(resolution);
                }
                if let Some(provider) = records.get(&key) {
                    let resolutions = provider
                        .resolutions
                        .iter()
                        .filter(|r| unique_contexts.contains(&provider_context(&r.context)))
                        .cloned()
                        .collect();
                    pending.push((package, resolutions));
                }
            }
        }
    }
    capture
        .inventory
        .package_providers
        .extend(records.into_values());
    for (file, source) in &capture.inventory.sources {
        if source.logical_path.file_name() == Some("package.json") {
            metadata
                .entry(*file)
                .or_insert_with(|| installed_metadata(capture, *file));
        }
    }
    capture.inventory.installed_packages = metadata.into_values().flatten().collect();
}

#[cfg(all(feature = "module_graph", feature = "lang_js"))]
fn installed_metadata(capture: &Capture, manifest: FileId) -> Option<InstalledPackage> {
    let path = &capture.inventory.sources.get(&manifest)?.logical_path;
    if path.file_name() != Some("package.json") {
        return None;
    }
    let text = capture.text(manifest)?;
    let parsed = biome_json_parser::parse_json(text, Default::default());
    let metadata = biome_package::read_project_metadata(&parsed.tree());
    let name = metadata
        .fields
        .get("name")
        .and_then(|v| v.value.as_string())
        .map(|v| v.as_str().to_string());
    let has_types = ["types", "typings"].iter().any(|key| {
        metadata
            .fields
            .get(*key)
            .is_some_and(|v| v.value.as_string().is_some())
    });
    let peers = metadata
        .fields
        .get("peerDependencies")
        .map(|v| v.members.keys().cloned().collect())
        .unwrap_or_default();
    Some(InstalledPackage {
        manifest,
        path: path.clone(),
        name,
        has_types,
        peers,
        metadata_known: !parsed.has_errors() && metadata.diagnostics.is_empty(),
    })
}

#[cfg(all(feature = "module_graph", feature = "lang_js"))]
fn provider_context(
    context: &biome_project_analysis::facts::ResolutionContext,
) -> ProjectResolutionContext {
    use biome_project_analysis::facts::{ResolutionProfile, ResolutionPurpose};
    ProjectResolutionContext {
        profile: match context.profile {
            ResolutionProfile::Knip => biome_module_graph::ProjectResolutionProfile::Knip,
            ResolutionProfile::Node => biome_module_graph::ProjectResolutionProfile::Node,
        },
        purpose: match context.purpose {
            ResolutionPurpose::Runtime => ProjectResolutionPurpose::Runtime,
            ResolutionPurpose::Declaration => ProjectResolutionPurpose::Declaration,
            ResolutionPurpose::PublicOutput => ProjectResolutionPurpose::PublicOutput,
        },
        syntax: public_syntax(context.syntax),
        config_revision: context.config_revision.clone(),
        mapping_revision: context.mapping_revision.clone(),
        additional_conditions: context.additional_conditions.clone(),
        source_mappings: context
            .source_mappings
            .iter()
            .map(|m| biome_resolver::SourceMapping {
                from: m.from.clone(),
                to: m.to.clone(),
            })
            .collect(),
    }
}
