use super::{inventory::Capture, paths};
use biome_package::{Manifest, MetadataValue, TsConfigJson};
use biome_project_analysis::{
    model::{FileId, Location},
    options::AnalysisMode,
};
use biome_rowan::TextRange;
use camino::{Utf8Path, Utf8PathBuf};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone)]
struct Selection {
    base: Utf8PathBuf,
    file: FileId,
    range: TextRange,
    patterns: Vec<String>,
}

#[derive(Clone, Default)]
pub(super) struct EffectiveConfig {
    pub exists: bool,
    valid: bool,
    path: Utf8PathBuf,
    file: Option<FileId>,
    files: Option<Selection>,
    include: Option<Selection>,
    exclude: Option<Selection>,
    root_dir: Option<Utf8PathBuf>,
    out_dir: Option<Utf8PathBuf>,
    references: Vec<Utf8PathBuf>,
    pub mappings: Vec<(Utf8PathBuf, Utf8PathBuf)>,
}

pub(super) fn load(
    capture: &mut Capture,
    path: &Utf8Path,
    visiting: &mut BTreeSet<Utf8PathBuf>,
    required: bool,
) -> EffectiveConfig {
    load_with_reference_policy(capture, path, visiting, required, false)
}

fn load_with_reference_policy(
    capture: &mut Capture,
    path: &Utf8Path,
    visiting: &mut BTreeSet<Utf8PathBuf>,
    required: bool,
    reference_child: bool,
) -> EffectiveConfig {
    if !capture.fs.path_exists(path) {
        if required {
            let file = capture.capture(path);
            capture.gap(
                "tsconfig",
                format!("Missing tsconfig {path}"),
                Some(Location {
                    file,
                    range: TextRange::default(),
                }),
            );
        }
        return EffectiveConfig::default();
    }
    let file = capture.capture(path);
    let mut config = EffectiveConfig {
        exists: true,
        valid: true,
        path: path.to_path_buf(),
        file: Some(file),
        ..Default::default()
    };
    if !visiting.insert(path.to_path_buf()) {
        capture.gap(
            "tsconfig",
            format!("Configuration cycle at {path}"),
            Some(Location {
                file,
                range: TextRange::default(),
            }),
        );
        config.valid = false;
        return config;
    }
    let Some(text) = capture.text(file) else {
        config.valid = false;
        visiting.remove(path);
        return config;
    };
    let parsed = biome_json_parser::parse_json(
        text,
        biome_json_parser::JsonParserOptions::default()
            .with_allow_comments()
            .with_allow_trailing_commas(),
    );
    let (metadata, _) = TsConfigJson::deserialize_manifest(&parsed.tree(), path).consume();
    let diagnostics: Vec<_> = parsed
        .diagnostics()
        .iter()
        .cloned()
        .map(biome_diagnostics::serde::Diagnostic::new)
        .collect();
    capture.diagnostics(file, &diagnostics);
    let Some(metadata) = metadata else {
        config.valid = false;
        visiting.remove(path);
        return config;
    };
    capture.diagnostics(file, &metadata.metadata_diagnostics);
    if parsed.has_errors() || !metadata.metadata_diagnostics.is_empty() {
        config.valid = false;
        visiting.remove(path);
        return config;
    }
    let Some(raw) = metadata.metadata else {
        config.valid = false;
        visiting.remove(path);
        return config;
    };
    let base = path.parent().unwrap();
    if let Some(extends) = raw.members.get("extends") {
        let values: Vec<_> = if let Some(value) = extends.value.as_string() {
            vec![value.as_str()]
        } else {
            extends
                .elements
                .iter()
                .filter_map(|v| v.value.as_string().map(|v| v.as_str()))
                .collect()
        };
        for specifier in values {
            let options = biome_resolver::ResolveOptions::new()
                .with_extensions(&["json"])
                .with_default_files(&["tsconfig"]);
            match biome_resolver::resolve(specifier, base, capture.fs, &options) {
                Ok(parent) => {
                    let inherited = load(capture, &parent, visiting, true);
                    config.valid &= inherited.valid;
                    if inherited.files.is_some() {
                        config.files = inherited.files;
                    }
                    if inherited.include.is_some() {
                        config.include = inherited.include;
                    }
                    if inherited.exclude.is_some() {
                        config.exclude = inherited.exclude;
                    }
                    if inherited.root_dir.is_some() {
                        config.root_dir = inherited.root_dir;
                    }
                    if inherited.out_dir.is_some() {
                        config.out_dir = inherited.out_dir;
                    }
                }
                Err(error) => {
                    capture.gap(
                        "tsconfig extends",
                        format!("Cannot resolve {specifier}: {error}"),
                        Some(Location {
                            file,
                            range: extends.range,
                        }),
                    );
                    config.valid = false;
                }
            }
        }
    }
    for (name, target) in [
        ("files", &mut config.files),
        ("include", &mut config.include),
        ("exclude", &mut config.exclude),
    ] {
        if let Some(value) = raw.members.get(name) {
            *target = Some(selection(base, file, value));
        }
    }
    if let Some(compiler) = raw.members.get("compilerOptions") {
        for (name, target) in [
            ("rootDir", &mut config.root_dir),
            ("outDir", &mut config.out_dir),
        ] {
            if let Some(value) = compiler.members.get(name).and_then(|v| v.value.as_string()) {
                *target = Some(paths::join(base, value.as_str()));
            }
        }
    }
    if let Some(references) = raw.members.get("references") {
        for reference in &references.elements {
            if let Some(value) = reference
                .members
                .get("path")
                .and_then(|v| v.value.as_string())
            {
                let mut target = paths::join(base, value.as_str());
                if target.extension() != Some("json") {
                    target = target.join("tsconfig.json");
                }
                config.references.push(target);
            } else {
                capture.gap(
                    "tsconfig references",
                    "Reference requires a path.",
                    Some(Location {
                        file,
                        range: reference.range,
                    }),
                );
            }
        }
    }
    if !reference_child || config.root_dir.is_none() || config.out_dir.is_none() {
        for reference in &config.references {
            let referenced = load_with_reference_policy(capture, reference, visiting, true, true);
            config.mappings.extend(referenced.mappings);
            if config.root_dir.is_none() {
                config.root_dir = referenced.root_dir;
            }
            if config.out_dir.is_none() {
                config.out_dir = referenced.out_dir;
            }
        }
    }
    let source = config.root_dir.clone().unwrap_or_else(|| {
        if config
            .out_dir
            .as_ref()
            .is_some_and(|out| capture.fs.path_is_dir(&out.join("src")))
        {
            base.to_path_buf()
        } else if capture.fs.path_is_dir(&base.join("src")) {
            base.join("src")
        } else {
            base.to_path_buf()
        }
    });
    config.mappings.push((
        config.out_dir.clone().unwrap_or_else(|| source.clone()),
        source,
    ));
    config.mappings.sort();
    config.mappings.dedup();
    visiting.remove(path);
    config
}

fn selection(base: &Utf8Path, file: FileId, value: &MetadataValue) -> Selection {
    Selection {
        base: base.into(),
        file,
        range: value.range,
        patterns: value
            .elements
            .iter()
            .filter_map(|v| v.value.as_string().map(|v| v.as_str().to_string()))
            .collect(),
    }
}
fn directory_pattern(base: &Utf8Path, pattern: &str) -> String {
    let path = paths::join(base, pattern);
    if !pattern.contains('*') && path.extension().is_none() {
        format!("{path}/**/*")
    } else {
        path.to_string()
    }
}

pub(super) fn selected_files(
    capture: &mut Capture,
    config: &EffectiveConfig,
    program: bool,
    files: &[Utf8PathBuf],
) -> Vec<(Utf8PathBuf, Location)> {
    let mut result = BTreeMap::new();
    select(
        capture,
        config,
        program,
        files,
        &mut BTreeSet::new(),
        &mut result,
    );
    result.into_iter().collect()
}
fn select(
    capture: &mut Capture,
    config: &EffectiveConfig,
    program: bool,
    files: &[Utf8PathBuf],
    visited: &mut BTreeSet<Utf8PathBuf>,
    result: &mut BTreeMap<Utf8PathBuf, Location>,
) {
    if !config.exists || !config.valid || !visited.insert(config.path.clone()) {
        return;
    }
    let base = config.path.parent().unwrap();
    if let Some(explicit) = &config.files {
        for path in &explicit.patterns {
            let path = paths::join(&explicit.base, path);
            let location = Location {
                file: explicit.file,
                range: explicit.range,
            };
            if capture.fs.path_is_file(&path) {
                result.insert(path, location);
            } else {
                capture.gap(
                    "tsconfig files",
                    format!("Missing program input {path}"),
                    Some(location),
                );
            }
        }
    }
    let default = Selection {
        base: base.into(),
        file: config.file.unwrap(),
        range: TextRange::default(),
        patterns: vec!["**/*".into()],
    };
    let includes = config
        .include
        .as_ref()
        .or_else(|| config.files.is_none().then_some(&default));
    if let Some(includes) = includes {
        let includes_patterns: Vec<_> = includes
            .patterns
            .iter()
            .map(|value| directory_pattern(&includes.base, value))
            .collect();
        let includes_patterns = capture.patterns(
            &includes_patterns,
            AnalysisMode::Default,
            false,
            "tsconfig include",
        );
        let excludes: Vec<_> = config
            .exclude
            .as_ref()
            .map(|exclude| {
                exclude
                    .patterns
                    .iter()
                    .map(|value| directory_pattern(&exclude.base, value))
                    .collect()
            })
            .unwrap_or_default();
        let excludes =
            capture.patterns(&excludes, AnalysisMode::Default, false, "tsconfig exclude");
        for path in files {
            if paths::is_source(path)
                && includes_patterns.matches(path)
                && !excludes.matches(path)
                && !path.components().any(|part| {
                    matches!(
                        part.as_str(),
                        "node_modules" | "bower_components" | "jspm_packages"
                    )
                })
                && !config
                    .out_dir
                    .as_ref()
                    .is_some_and(|out| path.starts_with(out))
            {
                result.insert(
                    path.clone(),
                    Location {
                        file: includes.file,
                        range: includes.range,
                    },
                );
            }
        }
    }
    if program {
        for reference in &config.references {
            if reference
                .parent()
                .is_some_and(|parent| parent.starts_with(base))
            {
                let referenced = load(capture, reference, &mut BTreeSet::new(), true);
                select(capture, &referenced, program, files, visited, result);
            }
        }
    }
}
