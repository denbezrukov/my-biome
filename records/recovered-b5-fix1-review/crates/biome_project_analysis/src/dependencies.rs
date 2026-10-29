//! Declaration ownership is independent of installation identity and report selection.

use crate::{
    coverage::{CoverageGap, TargetDomain},
    exports::ExportAnalysis,
    facts::{ResolutionEvent, ResolutionOutcome, ResolutionPurpose},
    inventory::{DependencyDeclaration, DependencyKind},
    model::{AnalysisSnapshot, FileId, Location, WorkspaceId},
    options::{AnalysisMode, AnalysisOptions},
    reachability::{GapId, GapImpact, Reachability, owner, path, suppressed},
    result::{Evidence, EvidenceGraph, EvidenceId, IssueKind, ProjectIssue},
    scripts::{Activation, ScriptProducer, ScriptResolution},
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct DependencyAnalysis {
    pub declarations: Vec<DeclarationUsage>,
    pub uses: Vec<DependencyUse>,
    pub issues: Vec<ProjectIssue>,
    pub evidence: EvidenceGraph,
    pub gaps: Vec<CoverageGap>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DeclarationUsage {
    pub declaration: DependencyDeclaration,
    pub used: Vec<EvidenceId>,
    pub retained: Vec<EvidenceId>,
    pub affecting_gaps: Vec<GapId>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum DependencyProducer {
    Import { file: FileId },
    PublicOutput { file: FileId },
    Manifest,
    Script(ScriptProducer),
}
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum DependencyReason {
    #[default]
    Direct,
    TypeCompanion {
        package: String,
    },
    PeerHost {
        package: String,
    },
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DependencyUse {
    #[serde(default)]
    pub reason: DependencyReason,
    pub workspace: WorkspaceId,
    pub location: Location,
    pub package: String,
    pub specifier: String,
    pub producer: DependencyProducer,
    pub definite: bool,
    pub optional: bool,
    pub type_only: bool,
    pub resolutions: Vec<ScriptResolution>,
    pub binary_providers: Vec<crate::scripts::BinaryProvider>,
}

/// Credits captured package requests after shared producer activation, before suppression.
pub fn analyze_dependencies(
    snapshot: &AnalysisSnapshot,
    reachability: &Reachability,
    _exports: &ExportAnalysis,
    options: &AnalysisOptions,
) -> DependencyAnalysis {
    let mut analysis = DependencyAnalysis {
        gaps: snapshot.inventory.public_gaps.clone(),
        ..Default::default()
    };
    for declaration in &snapshot.inventory.declarations {
        analysis.declarations.push(DeclarationUsage {
            declaration: declaration.clone(),
            used: Vec::new(),
            retained: Vec::new(),
            affecting_gaps: Vec::new(),
        });
    }
    for (file, module) in &snapshot.modules {
        if snapshot.sources.get(file).is_some_and(|s| {
            s.logical_path
                .components()
                .any(|c| c.as_str() == "node_modules")
        }) {
            continue;
        }
        let definite = reachability.definite_files.contains(file);
        if !definite && !reachability.possible_files.contains(file) {
            continue;
        }
        let Some(module) = &module.collected else {
            continue;
        };
        for import in &module.imports {
            let resolutions: Vec<_> = module
                .resolutions
                .iter()
                .filter(|r| {
                    r.occurrence == import.ordinal
                        && r.context.purpose != ResolutionPurpose::PublicOutput
                })
                .map(|r| ScriptResolution {
                    context: r.context.clone(),
                    outcome: r.outcome.clone(),
                    trace: r.trace.clone(),
                })
                .collect();
            let internal=resolutions.iter().any(|r|matches!(r.outcome,ResolutionOutcome::Resolved{file,..} if snapshot.inventory.workspaces.iter().any(|w|w.name.as_deref()==package_name(&import.specifier) && snapshot.sources.get(&file).is_some_and(|s|s.logical_path.starts_with(&w.path)))));
            if options.mode == AnalysisMode::Strict && import.type_only && !internal {
                continue;
            }
            if resolutions
                .iter()
                .any(|r| matches!(r.outcome, ResolutionOutcome::Builtin { .. }))
            {
                continue;
            }
            let Some(package) = package_name(&import.specifier) else {
                continue;
            };
            if import.specifier.starts_with('#') {
                continue;
            }
            // Author mappings are source edges rather than external package requirements.
            if resolutions.iter().any(|r| {
                r.trace
                    .iter()
                    .any(|e| matches!(e, ResolutionEvent::Mapping(_)))
            }) {
                continue;
            }
            let effective = if import.type_only || module.declaration_file.is_some() {
                resolutions
                    .iter()
                    .filter(|r| r.context.purpose == ResolutionPurpose::Declaration)
                    .flat_map(|r| &r.trace)
                    .filter_map(|e| {
                        if let ResolutionEvent::Package {
                            name: Some(name), ..
                        } = e
                        {
                            Some(name)
                        } else {
                            None
                        }
                    })
                    .find(|name| name.starts_with("@types/"))
                    .map_or(package, String::as_str)
            } else {
                package
            };
            analysis.uses.push(DependencyUse {
                reason: DependencyReason::Direct,
                workspace: owner(snapshot, *file),
                location: import.specifier_location,
                package: effective.into(),
                specifier: import.specifier.clone(),
                producer: DependencyProducer::Import { file: *file },
                definite: definite && !import.possible,
                optional: import.optional,
                type_only: import.type_only || module.declaration_file.is_some(),
                resolutions,
                binary_providers: Vec::new(),
            });
        }
    }
    analysis
        .uses
        .extend(snapshot.inventory.public_requirements.clone());
    for workspace in &snapshot.inventory.workspaces {
        for usage in &workspace.dependency_uses {
            analysis.uses.push(DependencyUse {
                reason: DependencyReason::Direct,
                workspace: workspace.workspace,
                location: usage.location.unwrap_or(Location {
                    file: workspace.manifest,
                    range: Default::default(),
                }),
                package: usage.package.clone(),
                specifier: usage.specifier.clone(),
                producer: DependencyProducer::Manifest,
                definite: true,
                optional: true,
                type_only: false,
                resolutions: Vec::new(),
                binary_providers: Vec::new(),
            });
        }
    }
    for usage in snapshot
        .inventory
        .script_usage
        .packages
        .iter()
        .filter(|u| u.mode == options.mode)
    {
        let activation = activation(reachability, &usage.producer);
        if activation == Activation::Dead {
            continue;
        }
        let resolutions = snapshot
            .inventory
            .script_usage
            .targets
            .iter()
            .filter(|t| {
                t.producer == usage.producer
                    && t.location == usage.location
                    && t.specifier == usage.specifier
            })
            .flat_map(|t| t.resolutions.clone())
            .collect();
        let binary_providers = snapshot
            .inventory
            .script_usage
            .binaries
            .iter()
            .filter(|b| b.producer == usage.producer && b.location == usage.location)
            .flat_map(|b| b.providers.clone())
            .filter(|p| {
                p.available
                    && p.requested_package.as_ref().or(p.package_name.as_ref())
                        == Some(&usage.package_name)
            })
            .collect();
        analysis.uses.push(DependencyUse {
            reason: DependencyReason::Direct,
            workspace: usage.workspace,
            location: usage.location,
            package: usage.package_name.clone(),
            specifier: usage.specifier.clone(),
            producer: DependencyProducer::Script(usage.producer.clone()),
            definite: activation == Activation::Definite,
            optional: usage.optional,
            type_only: false,
            resolutions,
            binary_providers,
        });
    }
    let initial = analysis.uses.clone();
    for usage in &initial {
        credit(snapshot, options, &mut analysis, usage);
    }
    let mut pending = initial;
    let mut expanded = std::collections::BTreeSet::new();
    while let Some(usage) = pending.pop() {
        if usage.producer == DependencyProducer::Manifest {
            continue;
        }
        let runtime: Vec<_> = usage
            .resolutions
            .iter()
            .filter(|r| r.context.purpose == ResolutionPurpose::Runtime)
            .cloned()
            .collect();
        let provider = installed_package(
            snapshot,
            if !usage.package.starts_with("@types/") && !runtime.is_empty() {
                &runtime
            } else {
                &usage.resolutions
            },
        );
        if !expanded.insert((
            usage.workspace,
            usage.location.file,
            u32::from(usage.location.range.start()),
            usage.package.clone(),
            usage.definite,
            provider.map(|p| p.manifest),
            matches!(usage.producer, DependencyProducer::PublicOutput { .. }),
        )) {
            continue;
        }
        let bundled = provider.is_some_and(|p| p.has_types);
        let peers = provider.map(|p| p.peers.clone()).unwrap_or_default();
        let mut derived: Vec<_> = peers
            .into_iter()
            .map(|p| {
                (
                    p,
                    DependencyReason::PeerHost {
                        package: usage.package.clone(),
                    },
                )
            })
            .collect();
        if !bundled && !usage.package.starts_with("@types/") {
            derived.push((
                type_companion(&usage.package),
                DependencyReason::TypeCompanion {
                    package: usage.package.clone(),
                },
            ));
        }
        for (package, reason) in derived {
            if !snapshot
                .inventory
                .declarations
                .iter()
                .any(|d| d.package == package)
            {
                continue;
            }
            let consumer = matches!(
                usage.producer,
                DependencyProducer::Import { .. } | DependencyProducer::PublicOutput { .. }
            )
            .then_some(usage.location);
            let resolutions = snapshot
                .inventory
                .package_providers
                .iter()
                .filter(|p| {
                    p.workspace == usage.workspace
                        && p.requested_package == package
                        && p.consumer == consumer
                })
                .flat_map(|p| &p.resolutions)
                .filter(|r| {
                    consumer.is_none()
                        || usage
                            .resolutions
                            .iter()
                            .any(|origin| origin.context == r.context)
                })
                .cloned()
                .collect();
            let derived = DependencyUse {
                package,
                reason,
                optional: true,
                type_only: true,
                resolutions,
                ..usage.clone()
            };
            credit(snapshot, options, &mut analysis, &derived);
            analysis.uses.push(derived.clone());
            pending.push(derived);
        }
    }
    for binary in snapshot
        .inventory
        .script_usage
        .binaries
        .iter()
        .filter(|b| b.mode == options.mode)
    {
        if !binary.optional
            && binary.report_suppression.is_empty()
            && activation(reachability, &binary.producer) == Activation::Definite
            && !binary.providers.iter().any(|p| p.available)
        {
            issue(
                snapshot,
                options,
                &mut analysis,
                IssueKind::Binaries,
                &binary.binary,
                binary.workspace,
                binary.location,
                Vec::new(),
            );
        }
    }
    for gap in &analysis.gaps {
        let id = dependency_gap_id(snapshot, gap);
        analysis.evidence.nodes.insert(
            EvidenceId(id.0),
            Evidence {
                producer: gap.producer.clone(),
                location: gap.location,
                workspace: gap
                    .location
                    .map_or(WorkspaceId(0), |l| owner(snapshot, l.file)),
                explanation: format!("{:?}", gap.reason),
                related: Vec::new(),
            },
        );
    }
    for index in 0..analysis.declarations.len() {
        let declaration = analysis.declarations[index].declaration.clone();
        for gap in reachability
            .gaps
            .iter()
            .filter(|g| g.active && g.impact == GapImpact::FileTargets)
        {
            let affected = match &gap.gap.domain {
                TargetDomain::Unbounded => true,
                TargetDomain::Bounded {
                    files,
                    workspaces,
                    packages,
                } => {
                    packages.contains(&declaration.package)
                        || workspaces.contains(&declaration.workspace)
                        || files
                            .iter()
                            .any(|file| owner(snapshot, *file) == declaration.workspace)
                        || analysis.uses.iter().any(|u| {
                            u.package == declaration.package
                                && (!u.definite
                                    || reachability.possible_files.contains(&u.location.file))
                        })
                }
            };
            if affected {
                analysis.declarations[index]
                    .affecting_gaps
                    .push(gap.id.clone());
            }
        }
        for gap in &analysis.gaps {
            if matches!(&gap.domain, TargetDomain::Unbounded)
                || matches!(&gap.domain,TargetDomain::Bounded{workspaces,packages,..} if workspaces.contains(&declaration.workspace) || packages.contains(&declaration.package))
            {
                analysis.declarations[index]
                    .affecting_gaps
                    .push(dependency_gap_id(snapshot, gap));
            }
        }
        if matches!(
            declaration.package.as_str(),
            "@types/node" | "@types/bun" | "@types/jest"
        ) {
            continue;
        }
        let usage = &analysis.declarations[index];
        let duplicate = declaration.kind == DependencyKind::Development
            && analysis.declarations.iter().any(|other| {
                other.declaration.workspace == declaration.workspace
                    && other.declaration.package == declaration.package
                    && other.declaration.kind == DependencyKind::Production
            });
        let kind = match declaration.kind {
            DependencyKind::Production if usage.used.is_empty() && usage.retained.is_empty() => {
                Some(IssueKind::Dependencies)
            }
            DependencyKind::Development
                if usage.used.is_empty() && usage.retained.is_empty() || duplicate =>
            {
                Some(IssueKind::DevDependencies)
            }
            DependencyKind::Peer
                if options.mode == AnalysisMode::Strict
                    && usage.used.is_empty()
                    && usage.retained.is_empty() =>
            {
                Some(IssueKind::Dependencies)
            }
            DependencyKind::OptionalPeer
                if !usage.used.is_empty()
                    && !analysis.declarations.iter().any(|other| {
                        other.declaration.workspace == declaration.workspace
                            && other.declaration.package == declaration.package
                            && matches!(
                                other.declaration.kind,
                                DependencyKind::Production | DependencyKind::Development
                            )
                    }) =>
            {
                Some(IssueKind::OptionalPeerDependencies)
            }
            _ => None,
        };
        if let Some(kind) = kind
            && (kind == IssueKind::OptionalPeerDependencies
                || usage.affecting_gaps.is_empty()
                    && !matches!(snapshot.coverage, crate::coverage::Coverage::Failed(_)))
        {
            let related = usage.used.clone();
            issue(
                snapshot,
                options,
                &mut analysis,
                kind,
                &declaration.package,
                declaration.workspace,
                declaration.location,
                related,
            );
        }
    }
    analysis
}
fn activation(reachability: &Reachability, producer: &ScriptProducer) -> Activation {
    reachability
        .membership
        .script_producers
        .iter()
        .find(|p| &p.producer == producer)
        .map_or(Activation::Dead, |p| p.activation)
}
fn credit(
    snapshot: &AnalysisSnapshot,
    options: &AnalysisOptions,
    analysis: &mut DependencyAnalysis,
    usage: &DependencyUse,
) {
    let reason = match usage.producer {
        DependencyProducer::Import { .. } => "import",
        DependencyProducer::PublicOutput { .. } => "public output",
        DependencyProducer::Manifest => "manifest",
        DependencyProducer::Script(_) => "script",
    };
    let producer = match &usage.producer {
        DependencyProducer::Script(ScriptProducer::Manifest { workspace, script }) => format!(
            "manifest:{}:{script}",
            snapshot
                .inventory
                .workspaces
                .iter()
                .find(|w| w.workspace == *workspace)
                .map_or("<unavailable>", |w| w.path.as_str())
        ),
        DependencyProducer::Script(ScriptProducer::Source { file }) => {
            format!("source:{}", path(snapshot, *file))
        }
        _ => reason.to_string(),
    };
    let id = EvidenceId(format!(
        "dependency-use:{}",
        crate::reachability::hash(&(
            path(snapshot, usage.location.file),
            usage.location.range,
            &usage.package,
            &usage.specifier,
            &usage.reason,
            producer
        ))
    ));
    analysis.evidence.nodes.insert(
        id.clone(),
        Evidence {
            producer: "dependency use".into(),
            location: Some(usage.location),
            workspace: usage.workspace,
            explanation: format!(
                "{}; requested {}; definite={}",
                usage.package, usage.specifier, usage.definite
            ),
            related: Vec::new(),
        },
    );
    let metadata = usage.producer == DependencyProducer::Manifest;
    if !metadata && matches!(usage.package.as_str(), "node" | "bun" | "deno") {
        return;
    }
    let consumer = snapshot
        .inventory
        .workspaces
        .iter()
        .find(|w| w.workspace == usage.workspace);
    let nearest = analysis
        .declarations
        .iter()
        .filter(|d| {
            d.declaration.package == usage.package && d.declaration.kind != DependencyKind::Catalog
        })
        .filter(|d| {
            if metadata || options.mode == AnalysisMode::Strict {
                if d.declaration.workspace != usage.workspace {
                    return false;
                }
            } else if !snapshot
                .inventory
                .workspaces
                .iter()
                .find(|w| w.workspace == d.declaration.workspace)
                .is_some_and(|w| {
                    w.path.as_str() == "." || consumer.is_some_and(|c| c.path.starts_with(&w.path))
                })
            {
                return false;
            }
            metadata
                || options.mode != AnalysisMode::Strict
                || usage.type_only
                    && !matches!(usage.producer, DependencyProducer::PublicOutput { .. })
                || matches!(
                    d.declaration.kind,
                    DependencyKind::Production | DependencyKind::Peer
                )
        })
        .max_by_key(|d| {
            snapshot
                .inventory
                .workspaces
                .iter()
                .find(|w| w.workspace == d.declaration.workspace)
                .map_or(0, |w| {
                    if w.path.as_str() == "." {
                        0
                    } else {
                        w.path.components().count()
                    }
                })
        })
        .map(|d| d.declaration.workspace);
    if let Some(workspace) = nearest {
        for d in &mut analysis.declarations {
            if d.declaration.workspace == workspace
                && d.declaration.package == usage.package
                && d.declaration.kind != DependencyKind::Catalog
                && usage.definite
            {
                if matches!(usage.reason, DependencyReason::PeerHost { .. }) {
                    d.retained.push(id.clone());
                } else {
                    d.used.push(id.clone());
                }
            }
        }
    } else if !metadata && !usage.optional && usage.definite {
        let self_import = consumer.is_some_and(|w| w.name.as_ref() == Some(&usage.package));
        let private = consumer.is_some_and(|w| {
            w.metadata
                .iter()
                .any(|(k, v)| k == "private" && v.value == true)
        });
        let workspace_import = private
            && options.mode != AnalysisMode::Strict
            && snapshot
                .inventory
                .workspaces
                .iter()
                .any(|w| w.name.as_ref() == Some(&usage.package));
        let ancestor = |workspace: WorkspaceId| {
            snapshot
                .inventory
                .workspaces
                .iter()
                .find(|w| w.workspace == workspace)
                .is_some_and(|w| {
                    w.workspace == usage.workspace
                        || options.mode != AnalysisMode::Strict
                            && (w.path.as_str() == "."
                                || consumer.is_some_and(|c| c.path.starts_with(&w.path)))
                })
        };
        let companion = usage.type_only
            && !usage.package.starts_with("@types/")
            && analysis.declarations.iter().any(|d| {
                d.declaration.package == type_companion(&usage.package)
                    && ancestor(d.declaration.workspace)
                    && d.declaration.kind != DependencyKind::Catalog
                    && (options.mode != AnalysisMode::Strict
                        || !matches!(usage.producer, DependencyProducer::PublicOutput { .. })
                        || matches!(
                            d.declaration.kind,
                            DependencyKind::Production | DependencyKind::Peer
                        ))
            });
        let engine = consumer.is_some_and(|w| {
            w.metadata.iter().any(|(key, value)| {
                key == "engines"
                    && value.members.iter().any(|(name, value)| {
                        name == &usage.package
                            && value.value.as_str().is_some_and(|v| !v.is_empty())
                    })
            })
        });
        let host = options.mode != AnalysisMode::Strict
            && usage
                .resolutions
                .iter()
                .any(|r| matches!(r.outcome, ResolutionOutcome::Resolved { .. }))
            && snapshot.inventory.package_providers.iter().any(|p| {
                p.consumer.is_none()
                    && ancestor(p.workspace)
                    && installed_package(snapshot, &p.resolutions).is_some_and(|metadata| {
                        metadata.metadata_known && metadata.peers.contains(&usage.package)
                    })
            });
        if !self_import && !workspace_import && !companion && !engine && !host {
            issue(
                snapshot,
                options,
                analysis,
                IssueKind::Unlisted,
                &usage.package,
                usage.workspace,
                usage.location,
                vec![id],
            );
        }
    }
}
#[expect(clippy::too_many_arguments)]
fn issue(
    snapshot: &AnalysisSnapshot,
    options: &AnalysisOptions,
    analysis: &mut DependencyAnalysis,
    kind: IssueKind,
    package: &str,
    workspace: WorkspaceId,
    location: Location,
    mut related_evidence: Vec<EvidenceId>,
) {
    let Some(severity) = options.severity(kind) else {
        return;
    };
    if suppressed(snapshot, options, location.file, false) {
        return;
    }
    let identity = format!(
        "{kind:?}:{}:{}:{package}",
        path(snapshot, location.file),
        location.range.start()
    );
    if analysis.issues.iter().any(|i| i.identity == identity) {
        return;
    }
    let id = EvidenceId(format!("dependency-finding:{identity}"));
    analysis.evidence.nodes.insert(
        id.clone(),
        Evidence {
            producer: "dependency finding".into(),
            location: Some(location),
            workspace,
            explanation: format!("{kind:?}: {package}"),
            related: related_evidence.clone(),
        },
    );
    related_evidence.push(id);
    analysis.issues.push(ProjectIssue {
        kind,
        severity,
        identity,
        symbol: Some(package.into()),
        location,
        workspace,
        related_evidence,
        eligible_fixes: Vec::new(),
    });
}
/// Extracts a bare request identity without interpreting aliases or installation layout.
pub fn package_name(specifier: &str) -> Option<&str> {
    if specifier.is_empty()
        || specifier.starts_with(['.', '/', '#', '!'])
        || specifier.contains(':')
    {
        return None;
    }
    let mut split = specifier.split('/');
    let first = split.next()?;
    if first.starts_with('@') {
        let second = split.next()?;
        Some(&specifier[..first.len() + 1 + second.len()])
    } else {
        Some(first)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PublicOutput {
    pub file: FileId,
    pub workspace: WorkspaceId,
    pub location: Location,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct InstalledPackage {
    pub manifest: FileId,
    pub path: camino::Utf8PathBuf,
    pub name: Option<String>,
    pub has_types: bool,
    pub peers: Vec<String>,
    pub metadata_known: bool,
}
pub fn type_companion(package: &str) -> String {
    format!(
        "@types/{}",
        package.trim_start_matches('@').replace('/', "__")
    )
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PackageProvider {
    pub workspace: WorkspaceId,
    /// Source occurrence whose resolution context produced this provider; absent for owner metadata.
    #[serde(default)]
    pub consumer: Option<Location>,
    pub requested_package: String,
    pub resolutions: Vec<ScriptResolution>,
}
fn installed_package<'a>(
    snapshot: &'a AnalysisSnapshot,
    resolutions: &[ScriptResolution],
) -> Option<&'a InstalledPackage> {
    resolutions
        .iter()
        .flat_map(|r| &r.trace)
        .filter_map(|event| {
            if let ResolutionEvent::Manifest { file, .. } = event {
                snapshot
                    .inventory
                    .installed_packages
                    .iter()
                    .find(|p| p.manifest == *file)
            } else {
                None
            }
        })
        .next_back()
}

fn dependency_gap_id(snapshot: &AnalysisSnapshot, gap: &CoverageGap) -> GapId {
    GapId(format!(
        "dependency-gap:{}",
        crate::reachability::hash(&(
            &gap.producer,
            gap.location.map(|l| (path(snapshot, l.file), l.range)),
            &gap.reason
        ))
    ))
}
