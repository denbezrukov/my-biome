//! Owned collection and resolution evidence copied from the authoritative module graph.

use crate::model::{DeclarationId, FileId, Location, SymbolNamespace};
use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};

macro_rules! evidence_enum {
    ($name:ident { $($variant:ident),* $(,)? }) => {
        #[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
        pub enum $name { $($variant),* }
    };
}
evidence_enum!(ParseStatus {
    Valid,
    Invalid,
    Unknown
});
evidence_enum!(DeclarationFileKind {
    TypeScript,
    Module,
    CommonJs
});
evidence_enum!(UseKind {
    Runtime,
    Type,
    TypeQuery,
    RuntimeTypeof
});
evidence_enum!(ImportSyntax {
    Import,
    Require,
    DynamicImport,
    ImportType,
    Reexport,
    RequireResolve,
    ImportMetaResolve,
    Url,
    Worker,
    NodeEntry
});
evidence_enum!(ExportForm {
    Declaration,
    Specifier,
    DefaultDeclaration,
    DefaultExpression,
    NamedReexport,
    Star,
    Namespace,
    CommonJs,
    CommonJsObjectMember
});
evidence_enum!(RegistryProvenance {
    Global,
    Host,
    Constructor,
    ShadowRoot,
    Alias,
    Unknown
});
evidence_enum!(UnknownKind {
    Declaration,
    ModuleSpecifier,
    Exports,
    Registry,
    Entry
});
evidence_enum!(ResolutionProfile { Knip, Node });
evidence_enum!(ResolutionPurpose {
    Runtime,
    Declaration
});

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CollectedModule {
    pub parse_status: ParseStatus,
    pub declaration_file: Option<DeclarationFileKind>,
    pub has_static_exports: bool,
    pub has_module_marker: bool,
    pub declarations: Vec<NamedDeclaration>,
    pub uses: Vec<Use>,
    pub imports: Vec<ImportOccurrence>,
    pub exports: Vec<Export>,
    pub registrations: Vec<Registration>,
    pub unknown: Vec<UnknownFact>,
    pub resolutions: Vec<ResolvedOccurrence>,
    #[serde(default)]
    pub export_surfaces: Vec<ExportSurface>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct NamedDeclaration {
    pub id: DeclarationId,
    pub name: String,
    pub tags: Vec<String>,
    #[serde(default)]
    pub kind: DeclarationKind,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum NamespaceAccess {
    Known(String),
    Computed,
    Escape,
    Enumerate,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Use {
    pub kind: UseKind,
    pub declaration: DeclarationId,
    pub location: Location,
    pub write: bool,
    pub namespace_access: Option<NamespaceAccess>,
    #[serde(default)]
    pub owner: Option<DeclarationId>,
    #[serde(default)]
    pub role: UseRole,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ImportBinding {
    pub declaration: DeclarationId,
    pub imported: String,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ImportTypeUse {
    pub kind: UseKind,
    pub namespace: SymbolNamespace,
    pub qualifier: Vec<String>,
    pub location: Location,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ImportOccurrence {
    pub ordinal: usize,
    pub specifier: String,
    pub location: Location,
    pub specifier_location: Location,
    pub syntax: ImportSyntax,
    pub type_only: bool,
    pub optional: bool,
    #[serde(default)]
    pub possible: bool,
    pub bindings: Vec<ImportBinding>,
    /// Whether a dynamic import result may be used beyond loading the module.
    #[serde(default)]
    pub dynamic_result_used: bool,
    pub type_usage: Option<ImportTypeUse>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ExportTarget {
    Local(DeclarationId),
    Reexport {
        specifier: String,
        symbol: String,
    },
    Namespace {
        specifier: String,
        identity: DeclarationId,
    },
    Unknown(String),
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Export {
    pub name: String,
    pub namespace: SymbolNamespace,
    pub location: Location,
    pub form: ExportForm,
    pub target: ExportTarget,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Registration {
    pub location: Location,
    pub declaration: Option<DeclarationId>,
    pub provenance: RegistryProvenance,
    #[serde(default)]
    pub rejected: bool,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct UnknownFact {
    pub location: Location,
    pub kind: UnknownKind,
    pub reason: String,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SourceMapping {
    pub from: String,
    pub to: Utf8PathBuf,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ResolutionContext {
    pub profile: ResolutionProfile,
    pub purpose: ResolutionPurpose,
    pub syntax: ImportSyntax,
    pub config_revision: String,
    pub mapping_revision: String,
    pub additional_conditions: Vec<String>,
    pub source_mappings: Vec<SourceMapping>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ManifestDiagnostic {
    pub severity: String,
    pub message: String,
    pub location: Option<Location>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ResolutionEvent {
    Manifest {
        file: FileId,
        known: bool,
        diagnostics: Vec<ManifestDiagnostic>,
    },
    Package {
        path: Utf8PathBuf,
        name: Option<String>,
    },
    Condition(String),
    Target(Utf8PathBuf),
    Mapping(SourceMapping),
    Failure(String),
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ResolutionOutcome {
    Resolved { file: FileId, kind: String },
    Unresolved { error: String },
    Builtin { name: String },
    Unknown { reason: String },
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ResolvedOccurrence {
    pub occurrence: usize,
    pub context: ResolutionContext,
    pub outcome: ResolutionOutcome,
    pub trace: Vec<ResolutionEvent>,
}

/// Names and origins are resolved by the module graph before snapshot publication.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ExportSurface {
    #[serde(default)]
    /// The owning workspace uses this context for roots and reporting sites.
    pub local_context: bool,
    pub context: ResolutionContext,
    pub namespace: SymbolNamespace,
    pub complete: bool,
    pub slots: Vec<ResolvedExport>,
}
#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct ExportSlotId {
    pub file: FileId,
    pub name: String,
    pub namespace: SymbolNamespace,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ResolvedExport {
    pub name: String,
    pub resolution: ExportResolution,
    pub candidates: Vec<DeclarationId>,
    pub slots: Vec<ExportSlotId>,
    pub namespace_target: Option<FileId>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ExportResolution {
    Found(DeclarationId),
    Missing,
    Ambiguous(Vec<DeclarationId>),
    Unknown(String),
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum DeclarationKind {
    Class,
    Enum,
    Function,
    Interface,
    Type,
    Variable,
    Namespace,
    Import,
    #[default]
    Other,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum UseRole {
    #[default]
    Local,
    /// Syntactically collected exported signature or return-expression dependencies.
    Signature,
    TypeBody,
    Export,
}
