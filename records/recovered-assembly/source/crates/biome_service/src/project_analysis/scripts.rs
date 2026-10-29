use super::{inventory::Capture, owned_facts, paths};
use biome_module_graph::{
    ProjectImportSyntax, ProjectResolution, ProjectResolutionContext, resolve_project_specifier,
};
use biome_project_analysis::{
    facts::{ManifestDiagnostic, ResolutionEvent, ResolutionOutcome},
    inventory::{FileRecord, FileRole},
    model::{FileId, Location, ModuleFacts},
    options::AnalysisMode,
    scripts::{BinaryProvider, ScriptGap, ScriptResolution, extract_script_uses},
};
use camino::Utf8PathBuf;
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn discover(
    capture: &mut Capture,
    modules: &BTreeMap<FileId, ModuleFacts>,
    db: &dyn biome_module_graph::ModuleDb,
    contexts: &[(Utf8PathBuf, Vec<ProjectResolutionContext>)],
    mode: AnalysisMode,
) -> BTreeSet<Utf8PathBuf> {
    let initial = extract_script_uses(&capture.inventory, modules, mode);
    for binary in initial.binaries {
        providers(capture, binary.workspace, &binary.binary, mode);
    }
    let mut usage = extract_script_uses(&capture.inventory, modules, mode);
    let mut targets = BTreeSet::new();
    for target in &mut usage.targets {
        if target.resolutions.is_empty() {
            let base = paths::join(&capture.root, target.cwd.as_str());
            let selected = contexts
                .iter()
                .filter(|(path, _)| base.starts_with(path))
                .max_by_key(|(path, _)| path.components().count())
                .map_or(&contexts[0].1, |(_, context)| context);
            for context in selected {
                let mut context = context.clone();
                context.syntax = ProjectImportSyntax::NodeEntry;
                let resolved =
                    resolve_project_specifier(db, &base, &target.specifier, context.clone());
                let before = capture.inventory.gaps.len();
                let resolution = convert(capture, &resolved, context);
                for gap in capture.inventory.gaps.drain(before..) {
                    usage.gaps.push(ScriptGap {
                        producer: target.producer.clone(),
                        mode,
                        gap,
                    });
                }
                target.resolutions.push(resolution);
            }
        }
        for resolution in &target.resolutions {
            if let ResolutionOutcome::Resolved { file, .. } = resolution.outcome {
                let path = capture
                    .root
                    .join(&capture.inventory.sources[&file].logical_path);
                targets.insert(path);
                if !capture.inventory.files.iter().any(|r| r.file == file) {
                    let workspace = capture
                        .inventory
                        .workspaces
                        .iter()
                        .filter(|w| {
                            capture.inventory.sources[&file]
                                .logical_path
                                .starts_with(&w.path)
                                || w.path.as_str() == "."
                        })
                        .max_by_key(|w| w.path.components().count())
                        .map_or(target.workspace, |w| w.workspace);
                    capture.inventory.files.push(FileRecord {
                        file,
                        workspace,
                        role: FileRole::Context,
                        aliases: Vec::new(),
                        program: false,
                    });
                }
            }
        }
    }
    usage.packages.retain(|package| {
        package.kind != biome_project_analysis::scripts::PackageUseKind::Preload
            || !usage.targets.iter().any(|target| {
                target.producer == package.producer
                    && target.location == package.location
                    && target.specifier == package.specifier
                    && !target.resolutions.is_empty()
                    && target.resolutions.iter().all(|resolution| {
                        matches!(resolution.outcome, ResolutionOutcome::Builtin { .. })
                    })
            })
    });
    capture.inventory.script_usage = usage;
    targets
}

fn providers(
    capture: &mut Capture,
    workspace: biome_project_analysis::model::WorkspaceId,
    binary: &str,
    mode: AnalysisMode,
) {
    if capture
        .inventory
        .binary_providers
        .iter()
        .any(|p| p.workspace == workspace && p.binary == binary)
    {
        return;
    }
    let Some(consumer) = capture
        .inventory
        .workspaces
        .iter()
        .find(|w| w.workspace == workspace)
    else {
        return;
    };
    let base = paths::join(&capture.root, consumer.path.as_str());
    let mut owners: Vec<_> = capture
        .inventory
        .workspaces
        .iter()
        .filter(|w| {
            (mode != AnalysisMode::Strict || w.workspace == workspace)
                && base.starts_with(paths::join(&capture.root, w.path.as_str()))
        })
        .map(|w| (w.workspace, paths::join(&capture.root, w.path.as_str())))
        .collect();
    owners.sort_by_key(|(_, path)| std::cmp::Reverse(path.components().count()));
    let mut records = Vec::new();
    for (owner, _) in owners {
        let packages: BTreeSet<_> = capture
            .inventory
            .declarations
            .iter()
            .filter(|d| d.workspace == owner)
            .map(|d| d.package.clone())
            .collect();
        let mut found = false;
        for package in packages {
            for ancestor in base.ancestors() {
                let manifest = ancestor
                    .join("node_modules")
                    .join(&package)
                    .join("package.json");
                if capture.fs.path_is_file(&manifest) {
                    let record = provider(
                        capture,
                        workspace,
                        Some(owner),
                        Some(package.clone()),
                        binary,
                        manifest,
                    );
                    found |= record.available;
                    records.push(record);
                    break;
                }
                records.push(BinaryProvider {
                    workspace,
                    owner: Some(owner),
                    requested_package: Some(package.clone()),
                    binary: binary.into(),
                    package_name: None,
                    manifest: None,
                    path: manifest,
                    available: false,
                    metadata_known: false,
                });
            }
        }
        if found {
            break;
        }
    }
    if !records.iter().any(|r| r.available) {
        for ancestor in base.ancestors().take(if mode == AnalysisMode::Strict {
            1
        } else {
            usize::MAX
        }) {
            let bin = ancestor.join("node_modules/.bin").join(binary);
            if capture.fs.path_exists(&bin) {
                if let Ok(canonical) = paths::canonical(capture.fs, &bin) {
                    for parent in canonical.ancestors().skip(1) {
                        let manifest = parent.join("package.json");
                        if capture.fs.path_is_file(&manifest) {
                            records
                                .push(provider(capture, workspace, None, None, binary, manifest));
                            break;
                        }
                    }
                }
                break;
            }
            records.push(BinaryProvider {
                workspace,
                owner: None,
                requested_package: None,
                binary: binary.into(),
                package_name: None,
                manifest: None,
                path: bin,
                available: false,
                metadata_known: false,
            });
        }
    }
    capture.inventory.binary_providers.extend(records);
}
fn provider(
    capture: &mut Capture,
    workspace: biome_project_analysis::model::WorkspaceId,
    owner: Option<biome_project_analysis::model::WorkspaceId>,
    requested_package: Option<String>,
    binary: &str,
    manifest: Utf8PathBuf,
) -> BinaryProvider {
    let before = capture.inventory.gaps.len();
    let file = capture.capture(&manifest);
    capture.inventory.gaps.truncate(before);
    let parsed = capture
        .text(file)
        .map(|text| biome_json_parser::parse_json(text, Default::default()));
    let metadata = parsed
        .as_ref()
        .map(|parsed| biome_package::read_project_metadata(&parsed.tree()));
    let name = metadata
        .as_ref()
        .and_then(|m| m.fields.get("name"))
        .and_then(|v| v.value.as_string())
        .map(|s| s.as_str().to_owned());
    let bin = metadata.as_ref().and_then(|m| m.fields.get("bin"));
    let supplies = bin.is_some_and(|value| {
        value.value.as_string().is_some()
            && name.as_deref().and_then(|n| n.rsplit('/').next()) == Some(binary)
            || value
                .members
                .iter()
                .any(|(key, value)| key == binary && value.value.as_string().is_some())
    });
    let metadata_known = parsed.as_ref().is_some_and(|parsed| !parsed.has_errors())
        && metadata.as_ref().is_some_and(|m| m.diagnostics.is_empty())
        && name.is_some();
    BinaryProvider {
        workspace,
        owner,
        requested_package,
        binary: binary.into(),
        package_name: name,
        manifest: Some(file),
        path: manifest,
        available: metadata_known && supplies,
        metadata_known,
    }
}

pub(super) fn convert(
    capture: &mut Capture,
    resolution: &ProjectResolution,
    context: ProjectResolutionContext,
) -> ScriptResolution {
    let (outcome, trace) = match resolution {
        ProjectResolution::Resolved {
            target,
            kind,
            trace,
        } => (
            ResolutionOutcome::Resolved {
                file: capture.capture(target),
                kind: format!("{kind:?}"),
            },
            trace,
        ),
        ProjectResolution::Unresolved { error, trace } => (
            ResolutionOutcome::Unresolved {
                error: format!("{error:?}"),
            },
            trace,
        ),
        ProjectResolution::Builtin { name, trace } => {
            (ResolutionOutcome::Builtin { name: name.clone() }, trace)
        }
        ProjectResolution::Unknown { reason, trace } => (
            ResolutionOutcome::Unknown {
                reason: reason.clone(),
            },
            trace,
        ),
    };
    ScriptResolution {
        outcome,
        context: owned_facts::context(&context),
        trace: trace
            .events
            .iter()
            .map(|event| match event {
                biome_resolver::ResolutionEvent::Manifest { path, quality } => {
                    let file = capture.capture(path);
                    ResolutionEvent::Manifest {
                        file,
                        known: quality.known,
                        diagnostics: quality
                            .diagnostics
                            .iter()
                            .map(|d| ManifestDiagnostic {
                                severity: format!("{:?}", d.severity),
                                message: d.message.clone(),
                                location: d.range.map(|range| Location { file, range }),
                            })
                            .collect(),
                    }
                }
                biome_resolver::ResolutionEvent::Package { path, name } => {
                    ResolutionEvent::Package {
                        path: path.clone(),
                        name: name.clone(),
                    }
                }
                biome_resolver::ResolutionEvent::Condition(value) => {
                    ResolutionEvent::Condition(value.clone())
                }
                biome_resolver::ResolutionEvent::Target(value) => {
                    ResolutionEvent::Target(value.clone())
                }
                biome_resolver::ResolutionEvent::Mapping(value) => {
                    ResolutionEvent::Mapping(biome_project_analysis::facts::SourceMapping {
                        from: value.from.clone(),
                        to: value.to.clone(),
                    })
                }
                biome_resolver::ResolutionEvent::Failure(error) => {
                    ResolutionEvent::Failure(format!("{error:?}"))
                }
            })
            .collect(),
    }
}
