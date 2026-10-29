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
    CommonJs
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
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct NamedDeclaration {
    pub id: DeclarationId,
    pub name: String,
    pub tags: Vec<String>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum NamespaceAccess {
    Known(String),
    Computed,
    Escape,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Use {
    pub kind: UseKind,
    pub declaration: DeclarationId,
    pub location: Location,
    pub write: bool,
    pub namespace_access: Option<NamespaceAccess>,
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
    pub bindings: Vec<ImportBinding>,
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
