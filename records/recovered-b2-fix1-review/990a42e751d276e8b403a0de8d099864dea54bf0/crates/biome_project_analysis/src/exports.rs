//! Export consumption uses resolved snapshot surfaces, independently of module execution.

use crate::{
    coverage::{Coverage, CoverageGap, GapReason, TargetDomain},
    facts::{
        CollectedModule, DeclarationKind, ExportForm, ExportResolution, ExportSlotId,
        ExportSurface, ExportTarget, ImportSyntax, NamespaceAccess, ParseStatus,
        RegistryProvenance, ResolutionContext, ResolutionOutcome, ResolutionPurpose,
        ResolvedExport, UnknownKind, UseKind, UseRole,
    },
    inventory::ExportProtection,
    model::{AnalysisSnapshot, DeclarationId, FileId, Location, SymbolNamespace, WorkspaceId},
    options::AnalysisOptions,
    reachability::{self, GapImpact, Reachability},
    result::{Evidence, EvidenceGraph, EvidenceId, IssueKind, ProjectIssue},
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ExportAnalysis {
    /// Definite use or public protection, excluding possible consumption.
    pub consumed: Vec<DeclarationId>,
    pub used_slots: Vec<ExportSlotId>,
    pub protected_slots: Vec<ExportSlotId>,
    pub member_uses: Vec<MemberUse>,
    pub uncertainty: Vec<ExportUncertainty>,
    pub issues: Vec<ProjectIssue>,
    pub evidence: EvidenceGraph,
    pub gaps: Vec<CoverageGap>,
}

/// The syntactic use remains available for member checks even when whole exports are retained.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MemberUse {
    /// Namespace-import binding, or the selected origin of a named import.
    pub declaration: DeclarationId,
    pub location: Location,
    pub kind: UseKind,
    pub access: NamespaceAccess,
    pub target: FileId,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ExportUncertainty {
    pub location: Location,
    pub slots: Vec<ExportSlotId>,
    pub declarations: Vec<DeclarationId>,
    pub reason: String,
}

/// Consumes immutable origin identities; this pass never resolves paths or infers function execution.
pub fn analyze_exports(
    snapshot: &AnalysisSnapshot,
    reachability: &Reachability,
    options: &AnalysisOptions,
) -> ExportAnalysis {
    let mut state = Consumption {
        snapshot,
        output: ExportAnalysis::default(),
        consumed: HashSet::new(),
        used: HashSet::new(),
        protected: HashSet::new(),
        uncertain: HashSet::new(),
        uncertain_declarations: HashSet::new(),
        root_workspace: None,
    };
    for root in &reachability.membership.entry_roots {
        state.root_workspace = Some(root.workspace);
        if (root.exports == ExportProtection::Protected
            || (root.exports == ExportProtection::IncludeWhenRequested
                && !options.include_entry_exports))
            && let Some(module) = get_module(snapshot, root.file)
        {
            for surface in module
                .export_surfaces
                .iter()
                .filter(|surface| surface.local_context)
            {
                if !surface.complete {
                    state.unknown_namespace(
                        root.file,
                        &surface.context,
                        root.location.unwrap_or(Location {
                            file: root.file,
                            range: Default::default(),
                        }),
                    );
                }
                for slot in &surface.slots {
                    state.consume(
                        slot,
                        root.location.unwrap_or(Location {
                            file: root.file,
                            range: Default::default(),
                        }),
                        true,
                        &format!("public export: {}", root.producer),
                    );
                    if matches!(slot.resolution, ExportResolution::Found(_))
                        && let Some(target) = slot.namespace_target
                    {
                        state.whole(
                            target,
                            &surface.context,
                            root.location.unwrap_or(Location {
                                file: root.file,
                                range: Default::default(),
                            }),
                            true,
                            &format!("public export: {}", root.producer),
                        );
                    }
                }
            }
        }
    }
    state.root_workspace = None;
    for &file in &reachability.definite_files {
        let Some(module) = get_module(snapshot, file) else {
            continue;
        };
        if module.parse_status != ParseStatus::Valid {
            continue;
        }
        for export in module.exports.iter().filter(|export| {
            export.form == ExportForm::CommonJsObjectMember && export.name == "default"
        }) {
            for surface in module
                .export_surfaces
                .iter()
                .filter(|surface| surface.local_context)
            {
                if let Some(slot) = surface.slots.iter().find(|slot| slot.name == "default")
                    && matches!(slot.resolution, ExportResolution::Ambiguous(_))
                {
                    state.consume(
                        slot,
                        export.location,
                        false,
                        "CommonJS default-name collision in snapshot representation",
                    );
                }
            }
        }
        for import in &module.imports {
            if import.possible || import.syntax == ImportSyntax::Reexport {
                continue;
            }
            for resolution in module
                .resolutions
                .iter()
                .filter(|r| r.occurrence == import.ordinal)
            {
                let ResolutionOutcome::Resolved { file: target, .. } = resolution.outcome else {
                    continue;
                };
                if import.dynamic_result_used {
                    state.unknown_namespace(target, &resolution.context, import.location);
                }
                if let Some(usage) = &import.type_usage {
                    if resolution.context.purpose != ResolutionPurpose::Declaration {
                        continue;
                    }
                    if let Some(name) = usage.qualifier.first() {
                        state.select(
                            target,
                            name,
                            usage.namespace,
                            &resolution.context,
                            usage.location,
                            "inline import type",
                        );
                    } else {
                        state.whole(
                            target,
                            &resolution.context,
                            usage.location,
                            false,
                            "inline module type",
                        );
                    }
                }
                for binding in &import.bindings {
                    if binding.imported == "*" {
                        state.namespace_uses(
                            module,
                            binding.declaration,
                            target,
                            &resolution.context,
                        );
                    } else {
                        let facet = binding.declaration.namespace;
                        let purpose = if import.type_only || facet == SymbolNamespace::Type {
                            ResolutionPurpose::Declaration
                        } else {
                            ResolutionPurpose::Runtime
                        };
                        if resolution.context.purpose == purpose {
                            state.select(
                                target,
                                &binding.imported,
                                facet,
                                &resolution.context,
                                import.location,
                                "import binding",
                            );
                            if let Some(slot) =
                                surface(snapshot, target, facet, &resolution.context).and_then(
                                    |surface| {
                                        surface
                                            .slots
                                            .iter()
                                            .find(|slot| slot.name == binding.imported)
                                    },
                                )
                            {
                                if let Some(target) = slot.namespace_target {
                                    state.namespace_uses(
                                        module,
                                        binding.declaration,
                                        target,
                                        &resolution.context,
                                    );
                                } else if let ExportResolution::Found(origin) = slot.resolution {
                                    for usage in module.uses.iter().filter(|usage| {
                                        usage.declaration == binding.declaration
                                            && !usage.write
                                            && usage.role != UseRole::Export
                                    }) {
                                        if let Some(access) = &usage.namespace_access {
                                            let member = MemberUse {
                                                declaration: origin,
                                                location: usage.location,
                                                kind: usage.kind,
                                                access: access.clone(),
                                                target,
                                            };
                                            if !state.output.member_uses.contains(&member) {
                                                state.output.member_uses.push(member);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                if import.syntax == ImportSyntax::Require
                    && resolution.context.purpose == ResolutionPurpose::Runtime
                {
                    state.select(
                        target,
                        "default",
                        SymbolNamespace::Value,
                        &resolution.context,
                        import.location,
                        "require default",
                    );
                }
            }
        }
        for registration in &module.registrations {
            if registration.provenance != RegistryProvenance::Unknown
                && let Some(id) = registration.declaration
            {
                state.credit(
                    id,
                    registration.location,
                    "native custom-element registration",
                );
            } else if !registration.rejected {
                let declarations: Vec<_> = registration.declaration.into_iter().collect();
                state
                    .uncertain_declarations
                    .extend(declarations.iter().copied());
                let reason = "Possible custom-element registration".to_string();
                state.evidence(
                    registration.location,
                    "uncertain registration",
                    reason.clone(),
                );
                state.output.uncertainty.push(ExportUncertainty {
                    location: registration.location,
                    slots: Vec::new(),
                    declarations,
                    reason: reason.clone(),
                });
                state.output.gaps.push(CoverageGap {
                    producer: "registration consumption".into(),
                    domain: registration
                        .declaration
                        .map_or(TargetDomain::Unbounded, |id| TargetDomain::Bounded {
                            files: vec![id.location.file],
                            workspaces: Vec::new(),
                            packages: Vec::new(),
                        }),
                    reason: GapReason::DynamicReference(reason),
                    location: Some(registration.location),
                });
            }
        }
        for usage in &module.uses {
            if usage.write
                || usage.role == UseRole::Export
                || usage
                    .owner
                    .is_some_and(|owner| owner.location == usage.declaration.location)
            {
                continue;
            }
            if let Some(declaration) = module
                .declarations
                .iter()
                .find(|d| d.id.location == usage.declaration.location)
                && local_enabled(options, declaration.kind)
            {
                state.credit(usage.declaration, usage.location, "local reference policy");
            }
        }
    }
    loop {
        let before = state.consumed.len();
        for &file in &reachability.definite_files {
            if let Some(module) = get_module(snapshot, file) {
                for usage in &module.uses {
                    if usage.role == UseRole::Signature
                        && !usage.write
                        && usage.owner.is_some_and(|id| state.is_consumed(id))
                    {
                        state.credit(usage.declaration, usage.location, "consumed signature");
                    }
                }
            }
        }
        if before == state.consumed.len() {
            break;
        }
    }
    if !matches!(snapshot.coverage, Coverage::Failed(_))
        && !state
            .output
            .gaps
            .iter()
            .any(|gap| gap.domain == TargetDomain::Unbounded)
    {
        for member in &reachability.membership.files {
            if !member.definite
                || member.possible
                || !member.diagnostic_eligible
                || reporting_uncertain(snapshot, reachability, member.file)
            {
                continue;
            }
            let Some(module) = get_module(snapshot, member.file) else {
                continue;
            };
            if module.parse_status != ParseStatus::Valid {
                continue;
            }
            for export in &module.exports {
                if matches!(
                    export.form,
                    ExportForm::Star | ExportForm::CommonJsObjectMember
                ) || export.name == "*"
                {
                    continue;
                }
                let key = ExportSlotId {
                    file: member.file,
                    name: export.name.clone(),
                    namespace: export.namespace,
                };
                if state.used.contains(&key)
                    || state.protected.contains(&key)
                    || state.uncertain.contains(&key)
                {
                    continue;
                }
                let resolutions: Vec<_> = module
                    .export_surfaces
                    .iter()
                    .filter(|surface| {
                        surface.local_context && surface.namespace == export.namespace
                    })
                    .filter_map(|surface| {
                        surface.slots.iter().find(|slot| slot.name == export.name)
                    })
                    .collect();
                if resolutions.is_empty()
                    || resolutions.iter().any(|slot| {
                        matches!(
                            slot.resolution,
                            ExportResolution::Unknown(_) | ExportResolution::Ambiguous(_)
                        )
                    })
                {
                    continue;
                }
                let Some(origin) = resolutions.iter().find_map(|slot| {
                    if let ExportResolution::Found(id) = slot.resolution {
                        Some(id)
                    } else {
                        None
                    }
                }) else {
                    continue;
                };
                if matches!(
                    export.target,
                    ExportTarget::Local(_) | ExportTarget::Namespace { .. }
                ) && (state.is_consumed(origin)
                    || state
                        .uncertain_declarations
                        .iter()
                        .any(|id| id.location == origin.location))
                {
                    continue;
                }
                let declaration = get_module(snapshot, origin.location.file).and_then(|m| {
                    m.declarations
                        .iter()
                        .find(|d| d.id.location == origin.location)
                });
                let kind = if declaration.is_some_and(|d| {
                    matches!(
                        d.kind,
                        DeclarationKind::Enum | DeclarationKind::Interface | DeclarationKind::Type
                    )
                }) || (declaration.is_none_or(|d| d.kind == DeclarationKind::Other)
                    && origin.namespace == SymbolNamespace::Type)
                {
                    IssueKind::Types
                } else {
                    IssueKind::Exports
                };
                if declaration.is_some_and(|d| {
                    (!options.included_tags.is_empty()
                        && !d.tags.iter().any(|tag| options.included_tags.contains(tag)))
                        || d.tags.iter().any(|tag| options.excluded_tags.contains(tag))
                }) {
                    continue;
                }
                let Some(severity) = options.severity(kind) else {
                    continue;
                };
                let location = if matches!(export.target, ExportTarget::Local(_)) {
                    origin.location
                } else {
                    export.location
                };
                let identity = format!(
                    "{:?}:{}:{}:{}",
                    kind,
                    reachability::path(snapshot, member.file),
                    export.name,
                    u32::from(location.range.start())
                );
                if state
                    .output
                    .issues
                    .iter()
                    .any(|issue| issue.identity == identity)
                {
                    continue;
                }
                let evidence = state.evidence(
                    location,
                    "unused export",
                    format!(
                        "No consumption or public protection reaches {}",
                        export.name
                    ),
                );
                state.output.issues.push(ProjectIssue {
                    kind,
                    severity,
                    identity,
                    symbol: Some(export.name.clone()),
                    location,
                    workspace: workspace(snapshot, member.file),
                    related_evidence: vec![evidence],
                    eligible_fixes: Vec::new(),
                });
            }
        }
    }
    state.output.consumed = state.consumed.into_iter().collect();
    state.output.used_slots = state.used.into_iter().collect();
    state.output.protected_slots = state.protected.into_iter().collect();
    state
        .output
        .consumed
        .sort_by_cached_key(|id| serde_json::to_string(id).unwrap());
    state
        .output
        .used_slots
        .sort_by_cached_key(|id| serde_json::to_string(id).unwrap());
    state
        .output
        .protected_slots
        .sort_by_cached_key(|id| serde_json::to_string(id).unwrap());
    state
        .output
        .issues
        .sort_by(|a, b| a.identity.cmp(&b.identity));
    state.output
}

fn get_module(snapshot: &AnalysisSnapshot, file: FileId) -> Option<&CollectedModule> {
    snapshot.modules.get(&file)?.collected.as_ref()
}
fn workspace(snapshot: &AnalysisSnapshot, file: FileId) -> crate::model::WorkspaceId {
    snapshot
        .inventory
        .files
        .iter()
        .find(|record| record.file == file)
        .map(|record| record.workspace)
        .or_else(|| {
            snapshot
                .inventory
                .workspaces
                .iter()
                .find(|workspace| workspace.manifest == file)
                .map(|workspace| workspace.workspace)
        })
        .or_else(|| {
            snapshot.sources.get(&file).and_then(|source| {
                snapshot
                    .inventory
                    .workspaces
                    .iter()
                    .filter(|workspace| {
                        workspace.path == "." || source.logical_path.starts_with(&workspace.path)
                    })
                    .max_by_key(|workspace| workspace.path.components().count())
                    .map(|workspace| workspace.workspace)
            })
        })
        .unwrap_or(crate::model::WorkspaceId(0))
}
fn local_enabled(options: &AnalysisOptions, kind: DeclarationKind) -> bool {
    options.ignore_local_uses.contains("*")
        || options.ignore_local_uses.contains(match kind {
            DeclarationKind::Class => "class",
            DeclarationKind::Enum => "enum",
            DeclarationKind::Function => "function",
            DeclarationKind::Interface => "interface",
            DeclarationKind::Type => "type",
            DeclarationKind::Variable => "variable",
            DeclarationKind::Namespace => "namespace",
            DeclarationKind::Import | DeclarationKind::Other => "unknown",
        })
}
fn context_matches(left: &ResolutionContext, right: &ResolutionContext) -> bool {
    left.profile == right.profile
        && left.purpose == right.purpose
        && left.config_revision == right.config_revision
        && left.mapping_revision == right.mapping_revision
        && left.additional_conditions == right.additional_conditions
        && left.source_mappings == right.source_mappings
}
fn surface<'a>(
    snapshot: &'a AnalysisSnapshot,
    file: FileId,
    namespace: SymbolNamespace,
    context: &ResolutionContext,
) -> Option<&'a ExportSurface> {
    get_module(snapshot, file)?
        .export_surfaces
        .iter()
        .find(|surface| {
            surface.namespace == namespace && context_matches(&surface.context, context)
        })
}
struct Consumption<'a> {
    snapshot: &'a AnalysisSnapshot,
    output: ExportAnalysis,
    consumed: HashSet<DeclarationId>,
    used: HashSet<ExportSlotId>,
    protected: HashSet<ExportSlotId>,
    uncertain: HashSet<ExportSlotId>,
    uncertain_declarations: HashSet<DeclarationId>,
    root_workspace: Option<WorkspaceId>,
}
impl Consumption<'_> {
    fn namespace_uses(
        &mut self,
        module: &CollectedModule,
        binding: DeclarationId,
        target: FileId,
        context: &ResolutionContext,
    ) {
        let has_selection = module.uses.iter().any(|usage| {
            usage.declaration.location == binding.location
                && matches!(usage.namespace_access, Some(NamespaceAccess::Known(_)))
        });
        for usage in module.uses.iter().filter(|usage| {
            usage.declaration == binding && !usage.write && usage.role != UseRole::Export
        }) {
            let purpose = if matches!(usage.kind, UseKind::Type | UseKind::TypeQuery) {
                ResolutionPurpose::Declaration
            } else {
                ResolutionPurpose::Runtime
            };
            if context.purpose != purpose {
                continue;
            }
            let access = usage
                .namespace_access
                .clone()
                .unwrap_or(NamespaceAccess::Escape);
            let member = MemberUse {
                declaration: binding,
                location: usage.location,
                kind: usage.kind,
                access: access.clone(),
                target,
            };
            if !self.output.member_uses.contains(&member) {
                self.output.member_uses.push(member);
            }
            match access {
                NamespaceAccess::Known(name) => self.select(
                    target,
                    &name,
                    usage.declaration.namespace,
                    context,
                    usage.location,
                    "known namespace member",
                ),
                NamespaceAccess::Computed => {
                    self.unknown_namespace(target, context, usage.location)
                }
                NamespaceAccess::Enumerate => self.whole(
                    target,
                    context,
                    usage.location,
                    false,
                    "namespace enumeration",
                ),
                NamespaceAccess::Escape
                    if !has_selection && usage.kind != UseKind::RuntimeTypeof =>
                {
                    self.unknown_namespace(target, context, usage.location)
                }
                NamespaceAccess::Escape => {}
            }
        }
    }
    fn is_consumed(&self, id: DeclarationId) -> bool {
        self.consumed
            .iter()
            .any(|used| used.location == id.location)
    }
    fn evidence(&mut self, location: Location, producer: &str, explanation: String) -> EvidenceId {
        let workspace = self
            .root_workspace
            .unwrap_or_else(|| workspace(self.snapshot, location.file));
        let workspace_path = self
            .snapshot
            .inventory
            .workspaces
            .iter()
            .find(|record| record.workspace == workspace)
            .map(|record| record.path.as_str())
            .unwrap_or_default();
        let id = EvidenceId(format!(
            "export:{producer}:{workspace_path}:{}:{}:{}",
            reachability::path(self.snapshot, location.file),
            u32::from(location.range.start()),
            reachability::hash(&explanation)
        ));
        self.output.evidence.nodes.insert(
            id.clone(),
            Evidence {
                producer: producer.into(),
                location: Some(location),
                workspace,
                explanation,
                related: Vec::new(),
            },
        );
        id
    }
    fn credit(&mut self, id: DeclarationId, location: Location, reason: &str) {
        self.consumed.insert(id);
        self.evidence(
            location,
            reason,
            format!(
                "{}:{:?}:{:?}",
                reachability::path(self.snapshot, id.location.file),
                id.location.range,
                id.namespace
            ),
        );
    }
    fn consume(
        &mut self,
        slot: &ResolvedExport,
        location: Location,
        protected: bool,
        reason: &str,
    ) {
        match &slot.resolution {
            ExportResolution::Found(id) => {
                self.credit(*id, location, reason);
                for key in &slot.slots {
                    if protected {
                        self.protected.insert(key.clone());
                    } else {
                        self.used.insert(key.clone());
                    }
                }
            }
            ExportResolution::Missing => {}
            ExportResolution::Ambiguous(_) | ExportResolution::Unknown(_) => {
                self.uncertain.extend(slot.slots.iter().cloned());
                for candidate in &slot.candidates {
                    self.uncertain_declarations.insert(*candidate);
                }
                let reason = match &slot.resolution {
                    ExportResolution::Unknown(reason) => {
                        format!("Unavailable origin for {}: {reason}", slot.name)
                    }
                    _ => format!("Ambiguous origin for {}: {reason}", slot.name),
                };
                self.output.gaps.push(CoverageGap {
                    producer: "export origins".into(),
                    domain: if matches!(slot.resolution, ExportResolution::Unknown(_)) {
                        TargetDomain::Unbounded
                    } else {
                        TargetDomain::Bounded {
                            files: slot.candidates.iter().map(|id| id.location.file).collect(),
                            workspaces: Vec::new(),
                            packages: Vec::new(),
                        }
                    },
                    reason: GapReason::DynamicReference(reason.clone()),
                    location: Some(location),
                });
                self.evidence(location, "uncertain export origin", reason.clone());
                let uncertainty = ExportUncertainty {
                    location,
                    slots: slot.slots.clone(),
                    declarations: slot.candidates.clone(),
                    reason,
                };
                if !self.output.uncertainty.contains(&uncertainty) {
                    self.output.uncertainty.push(uncertainty);
                }
            }
        }
    }
    fn select(
        &mut self,
        file: FileId,
        name: &str,
        namespace: SymbolNamespace,
        context: &ResolutionContext,
        location: Location,
        reason: &str,
    ) {
        if let Some(surface) = surface(self.snapshot, file, namespace, context) {
            if let Some(slot) = surface.slots.iter().find(|slot| slot.name == name) {
                self.consume(slot, location, false, reason);
                if matches!(reason, "known namespace member" | "inline import type")
                    && let Some(target) = slot.namespace_target
                {
                    self.unknown_namespace(target, context, location);
                }
            } else if !surface.complete {
                self.unknown_namespace(file, context, location);
            }
        } else {
            self.unknown_namespace(file, context, location);
        }
    }
    fn whole(
        &mut self,
        file: FileId,
        context: &ResolutionContext,
        location: Location,
        protected: bool,
        reason: &str,
    ) {
        let mut pending = vec![file];
        let mut seen = HashSet::new();
        while let Some(file) = pending.pop() {
            if !seen.insert(file) {
                continue;
            }
            for namespace in [SymbolNamespace::Value, SymbolNamespace::Type] {
                if let Some(surface) = surface(self.snapshot, file, namespace, context) {
                    if !surface.complete {
                        self.unknown_namespace(file, context, location);
                    }
                    for slot in &surface.slots {
                        self.consume(slot, location, protected, reason);
                        if matches!(slot.resolution, ExportResolution::Found(_))
                            && let Some(target) = slot.namespace_target
                        {
                            pending.push(target);
                        }
                    }
                }
            }
        }
    }
    fn unknown_namespace(&mut self, file: FileId, context: &ResolutionContext, location: Location) {
        let mut slots = Vec::new();
        let mut declarations = Vec::new();
        let mut complete = true;
        let mut seen = std::collections::BTreeSet::new();
        let mut pending = vec![file];
        while let Some(file) = pending.pop() {
            if !seen.insert(file) {
                continue;
            }
            for namespace in [SymbolNamespace::Value, SymbolNamespace::Type] {
                if let Some(surface) = surface(self.snapshot, file, namespace, context) {
                    complete &= surface.complete;
                    for slot in &surface.slots {
                        complete &= !matches!(slot.resolution, ExportResolution::Unknown(_));
                        self.uncertain.extend(slot.slots.iter().cloned());
                        slots.extend(slot.slots.iter().cloned());
                        declarations.extend(slot.candidates.iter().copied());
                        self.uncertain_declarations
                            .extend(slot.candidates.iter().copied());
                        if let Some(target) = slot.namespace_target {
                            pending.push(target);
                        }
                    }
                } else {
                    complete = false;
                }
            }
        }
        let reason = "Namespace members are not individually known".to_string();
        self.evidence(
            location,
            "uncertain namespace use",
            reachability::path(self.snapshot, file),
        );
        let uncertainty = ExportUncertainty {
            location,
            slots,
            declarations,
            reason: reason.clone(),
        };
        if !self.output.uncertainty.contains(&uncertainty) {
            self.output.uncertainty.push(uncertainty);
        }
        let gap = CoverageGap {
            producer: "export consumption".into(),
            domain: if complete {
                TargetDomain::Bounded {
                    files: seen.into_iter().collect(),
                    workspaces: Vec::new(),
                    packages: Vec::new(),
                }
            } else {
                TargetDomain::Unbounded
            },
            reason: GapReason::DynamicReference(reason),
            location: Some(location),
        };
        if !self.output.gaps.contains(&gap) {
            self.output.gaps.push(gap);
        }
    }
}
fn reporting_uncertain(
    snapshot: &AnalysisSnapshot,
    reachability: &Reachability,
    file: FileId,
) -> bool {
    reachability.gaps.iter().any(|gap| {
        if !gap.active || gap.impact == GapImpact::FileReporting {
            return false;
        }
        if gap.impact == GapImpact::Symbols {
            return gap.gap.location.is_some_and(|location| {
                location.file == file
                    && get_module(snapshot, file).is_some_and(|module| {
                        module.unknown.iter().any(|unknown| {
                            unknown.location == location && unknown.kind != UnknownKind::Registry
                        })
                    })
            });
        }
        match &gap.gap.domain {
            TargetDomain::Unbounded => true,
            TargetDomain::Bounded {
                files, workspaces, ..
            } => files.contains(&file) || workspaces.contains(&workspace(snapshot, file)),
        }
    })
}
