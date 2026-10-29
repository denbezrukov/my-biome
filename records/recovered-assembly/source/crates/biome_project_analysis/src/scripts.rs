//! Static command evidence retains its producer independently of the evidence location.

use crate::{
    coverage::CoverageGap,
    facts::{ResolutionContext, ResolutionEvent, ResolutionOutcome},
    inventory::Inventory,
    model::{FileId, Location, ModuleFacts, WorkspaceId},
    options::AnalysisMode,
};
use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum ScriptProducer {
    Manifest {
        workspace: WorkspaceId,
        script: String,
    },
    Source {
        file: FileId,
    },
}
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
/// Extraction records are not live uses until their typed producer is activated.
/// Only selected manifest producers can seed reachability without a source edge.
pub struct ScriptUsage {
    pub targets: Vec<ScriptTarget>,
    pub packages: Vec<ScriptPackage>,
    pub binaries: Vec<BinaryUse>,
    pub catalogs: Vec<CatalogReference>,
    pub gaps: Vec<ScriptGap>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ScriptTarget {
    pub producer: ScriptProducer,
    pub workspace: WorkspaceId,
    pub location: Location,
    pub mode: AnalysisMode,
    pub cwd: Utf8PathBuf,
    pub specifier: String,
    pub resolutions: Vec<ScriptResolution>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ScriptResolution {
    pub context: ResolutionContext,
    pub outcome: ResolutionOutcome,
    pub trace: Vec<ResolutionEvent>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ScriptPackage {
    pub kind: PackageUseKind,
    pub producer: ScriptProducer,
    pub workspace: WorkspaceId,
    pub location: Location,
    pub mode: AnalysisMode,
    pub package_name: String,
    pub specifier: String,
    pub optional: bool,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum PackageUseKind {
    Preload,
    Binary,
    OnDemand,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CatalogReference {
    pub package_name: String,
    pub catalog_name: Option<String>,
    pub protocol: String,
    pub producer: ScriptProducer,
    pub location: Location,
    pub mode: AnalysisMode,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BinaryUse {
    pub optional: bool,
    pub producer: ScriptProducer,
    pub workspace: WorkspaceId,
    pub location: Location,
    pub mode: AnalysisMode,
    pub binary: String,
    pub providers: Vec<BinaryProvider>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
/// Captured installation identity, including declared aliases and unsuccessful lookup paths.
/// `available` denotes a valid bin declaration; it does not assert executable availability.
pub struct BinaryProvider {
    #[serde(default)]
    pub requested_package: Option<String>,
    #[serde(default)]
    pub owner: Option<WorkspaceId>,
    #[serde(default)]
    pub metadata_known: bool,
    pub workspace: WorkspaceId,
    pub binary: String,
    pub package_name: Option<String>,
    pub manifest: Option<FileId>,
    pub path: Utf8PathBuf,
    pub available: bool,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ScriptGap {
    pub producer: ScriptProducer,
    pub mode: AnalysisMode,
    pub gap: CoverageGap,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
/// Trusted Node calls retain shell text separately from literal argv and unresolved values.
pub struct SourceCommand {
    pub location: Location,
    pub method: String,
    pub command: Option<String>,
    pub args: Option<Vec<String>>,
    pub cwd: Option<String>,
    pub unsupported: bool,
}

/// Extracts bounded static commands without I/O, environment lookup or shell evaluation.
pub fn extract_script_uses(
    inventory: &Inventory,
    modules: &BTreeMap<FileId, ModuleFacts>,
    mode: AnalysisMode,
) -> ScriptUsage {
    let mut state = Extraction {
        inventory,
        mode,
        usage: ScriptUsage::default(),
        visited: Default::default(),
        budget: 4096,
    };
    for workspace in &inventory.workspaces {
        if let Some((_, scripts)) = workspace
            .metadata
            .iter()
            .find(|(name, _)| name == "scripts")
        {
            for (name, value) in &scripts.members {
                if mode != AnalysisMode::Default && name != "start" {
                    continue;
                }
                state.script(
                    Context {
                        producer: ScriptProducer::Manifest {
                            workspace: workspace.workspace,
                            script: name.clone(),
                        },
                        workspace: workspace.workspace,
                        location: value.location,
                        cwd: workspace.path.clone(),
                    },
                    name,
                    &[],
                    0,
                );
            }
        }
    }
    for (file, module) in modules {
        let Some(workspace) = inventory
            .files
            .iter()
            .find(|r| r.file == *file)
            .and_then(|r| {
                inventory
                    .workspaces
                    .iter()
                    .find(|w| w.workspace == r.workspace)
            })
        else {
            continue;
        };
        for command in &module.commands {
            let mut context = Context {
                producer: ScriptProducer::Source { file: *file },
                workspace: workspace.workspace,
                location: command.location,
                cwd: workspace.path.clone(),
            };
            if let Some(cwd) = &command.cwd {
                context.cwd = normalize_path(&context.cwd.join(cwd));
            }
            if command.method == "Bun" {
                state.gap(&context, "Bun command adapter is unsupported");
                continue;
            }
            if command.unsupported || command.command.is_none() || command.args.is_none() {
                state.gap(
                    &context,
                    "Unsupported Node command, argv, cwd or shell option",
                );
                continue;
            }
            let value = command.command.as_ref().unwrap();
            match command.method.as_str() {
                "exec" | "execSync" => state.shell(context, value, &[], 0),
                "fork" | "worker" => state.target(&context, relative(value)),
                _ => {
                    let mut args = vec![value.clone()];
                    args.extend(command.args.clone().unwrap());
                    state.command(&mut context, &args, 0);
                }
            }
        }
        if let Some(facts) = &module.collected {
            for import in facts
                .imports
                .iter()
                .filter(|i| matches!(i.specifier.as_str(), "execa" | "zx" | "zx/globals"))
            {
                state.gap(
                    &Context {
                        producer: ScriptProducer::Source { file: *file },
                        workspace: workspace.workspace,
                        location: import.location,
                        cwd: workspace.path.clone(),
                    },
                    "Imported command library requires a tool adapter",
                );
            }
            for import in facts.imports.iter().filter(|i| {
                matches!(
                    i.syntax,
                    crate::facts::ImportSyntax::NodeEntry | crate::facts::ImportSyntax::Worker
                )
            }) {
                state.usage.targets.push(ScriptTarget {
                    producer: ScriptProducer::Source { file: *file },
                    workspace: workspace.workspace,
                    location: import.location,
                    mode,
                    cwd: workspace.path.clone(),
                    specifier: import.specifier.clone(),
                    resolutions: facts
                        .resolutions
                        .iter()
                        .filter(|r| r.occurrence == import.ordinal)
                        .map(|r| ScriptResolution {
                            context: r.context.clone(),
                            outcome: r.outcome.clone(),
                            trace: r.trace.clone(),
                        })
                        .collect(),
                });
            }
        }
    }
    state.usage
}

#[derive(Clone)]
struct Context {
    producer: ScriptProducer,
    workspace: WorkspaceId,
    location: Location,
    cwd: Utf8PathBuf,
}
struct Extraction<'a> {
    inventory: &'a Inventory,
    mode: AnalysisMode,
    usage: ScriptUsage,
    visited: std::collections::BTreeSet<String>,
    budget: usize,
}
impl Extraction<'_> {
    fn gap(&mut self, context: &Context, reason: &str) {
        self.usage.gaps.push(ScriptGap {
            producer: context.producer.clone(),
            mode: self.mode,
            gap: CoverageGap {
                producer: "script analysis".into(),
                domain: crate::coverage::TargetDomain::Unbounded,
                reason: if reason.contains("adapter") || reason.contains("Tool-specific") {
                    crate::coverage::GapReason::Capability(reason.into())
                } else {
                    crate::coverage::GapReason::DynamicReference(reason.into())
                },
                location: Some(context.location),
            },
        });
    }
    fn target(&mut self, context: &Context, specifier: String) {
        self.usage.targets.push(ScriptTarget {
            producer: context.producer.clone(),
            workspace: context.workspace,
            location: context.location,
            mode: self.mode,
            cwd: context.cwd.clone(),
            specifier,
            resolutions: Vec::new(),
        });
    }
    fn package(
        &mut self,
        context: &Context,
        specifier: &str,
        optional: bool,
        kind: PackageUseKind,
    ) {
        let split = specifier.get(1..).and_then(|s| s.find('@')).map(|i| i + 1);
        let (name, version) = split.map_or((specifier, None), |i| {
            (&specifier[..i], Some(&specifier[i + 1..]))
        });
        if let Some(catalog) = version.and_then(|v| v.strip_prefix("catalog:")) {
            self.usage.catalogs.push(CatalogReference {
                package_name: name.into(),
                catalog_name: (!catalog.is_empty()).then(|| catalog.into()),
                protocol: "catalog:".into(),
                producer: context.producer.clone(),
                location: context.location,
                mode: self.mode,
            });
        }
        let package_name = if name.starts_with('@') {
            name.split('/').take(2).collect::<Vec<_>>().join("/")
        } else {
            name.split('/').next().unwrap_or(name).into()
        };
        if !package_name.is_empty() {
            self.usage.packages.push(ScriptPackage {
                kind,
                producer: context.producer.clone(),
                workspace: context.workspace,
                location: context.location,
                mode: self.mode,
                package_name,
                specifier: specifier.into(),
                optional,
            });
        }
    }
    fn script(&mut self, mut context: Context, name: &str, forwarded: &[String], depth: usize) {
        if depth > 64 {
            self.gap(&context, "Script recursion limit");
            return;
        }
        let key = serde_json::to_string(&(
            &context.producer,
            context.workspace,
            name,
            &context.cwd,
            forwarded,
        ))
        .unwrap();
        if !self.visited.insert(key) {
            return;
        }
        let script = self
            .inventory
            .workspaces
            .iter()
            .find(|w| w.workspace == context.workspace)
            .and_then(|w| w.metadata.iter().find(|(n, _)| n == "scripts"))
            .and_then(|(_, v)| v.members.iter().find(|(n, _)| n == name))
            .map(|(_, v)| v.clone());
        if let Some(value) = script {
            context.location = value.location;
            if let Some(body) = value.value.as_str() {
                self.shell(context, body, forwarded, depth + 1);
            } else {
                self.gap(&context, "Non-string script body");
            }
        } else {
            self.gap(&context, "Referenced script is unavailable");
        }
    }
    fn shell(&mut self, mut context: Context, body: &str, forwarded: &[String], depth: usize) {
        let (mut tokens, invalid) = tokenize(body);
        tokens.extend(forwarded.iter().map(|s| Token::Word(s.clone(), false)));
        if invalid {
            self.gap(&context, "Unsupported shell expansion or syntax");
        }
        let mut argv = Vec::new();
        let mut invalid_command = false;
        let mut redirect = false;
        let mut stack = Vec::new();
        let mut pipeline = false;
        for token in tokens
            .into_iter()
            .chain(std::iter::once(Token::Operator(";".into())))
        {
            match token {
                Token::Word(word, unknown) => {
                    invalid_command |= unknown;
                    if redirect {
                        redirect = false;
                    } else {
                        argv.push(word);
                    }
                }
                Token::Operator(op) if op.starts_with('>') || op.starts_with('<') => {
                    redirect = true;
                }
                Token::Operator(op) => {
                    if !argv.is_empty() && !invalid_command {
                        if pipeline || op == "|" {
                            self.command(&mut context.clone(), &argv, depth);
                        } else {
                            self.command(&mut context, &argv, depth);
                        }
                    }
                    argv.clear();
                    invalid_command = false;
                    if redirect {
                        self.gap(&context, "Missing redirection target");
                        redirect = false;
                    }
                    if op == "(" {
                        stack.push((context.clone(), pipeline));
                        pipeline = false;
                    } else if op == ")" {
                        if let Some((previous, previous_pipeline)) = stack.pop() {
                            context = previous;
                            pipeline = previous_pipeline;
                        } else {
                            self.gap(&context, "Unmatched shell parenthesis");
                        }
                    } else {
                        pipeline = op == "|";
                    }
                }
            }
        }
        if !stack.is_empty() {
            self.gap(&context, "Unmatched shell parenthesis");
        }
    }
    fn command(&mut self, context: &mut Context, argv: &[String], depth: usize) {
        if self.budget == 0 || depth > 64 {
            self.gap(context, "Static command limit");
            return;
        }
        self.budget -= 1;
        let assignments = argv.iter().take_while(|s| is_assignment(s)).count();
        for assignment in &argv[..assignments] {
            if let Some(value) = assignment.strip_prefix("NODE_OPTIONS=") {
                let (tokens, invalid) = tokenize(value);
                let words: Option<Vec<_>> = tokens
                    .into_iter()
                    .map(|token| match token {
                        Token::Word(word, false) => Some(word),
                        _ => None,
                    })
                    .collect();
                if let Some(words) = words.filter(|_| !invalid) {
                    self.node(context, &words, false);
                } else {
                    self.gap(context, "Unsupported NODE_OPTIONS expansion");
                }
            }
        }
        let argv = &argv[assignments..];
        let Some(binary) = argv.first().map(String::as_str) else {
            return;
        };
        let args = &argv[1..];
        match binary {
            "cd" if args.len() == 1 => {
                context.cwd = normalize_path(&context.cwd.join(&args[0]));
                if let Some(workspace) = self
                    .inventory
                    .workspaces
                    .iter()
                    .filter(|w| {
                        w.path.as_str() == "." || context.cwd.starts_with(normalize_path(&w.path))
                    })
                    .max_by_key(|w| w.path.components().count())
                {
                    context.workspace = workspace.workspace;
                }
            }
            "node" | "nodejs" => self.node(context, args, true),
            "npm" | "pnpm" | "yarn" | "npx" | "pnpx" => self.manager(context, binary, args, depth),
            "env" => self.command(context, args, depth + 1),
            _ => self.binary(context, binary, args),
        }
    }
    fn node(&mut self, context: &Context, args: &[String], entry_required: bool) {
        let mut i = 0;
        while i < args.len() {
            let arg = &args[i];
            i += 1;
            if matches!(arg.as_str(), "-e" | "--eval" | "-p" | "--print")
                || arg.starts_with("--eval=")
                || arg.starts_with("--print=")
                || arg.starts_with("-e")
                || arg.starts_with("-p")
            {
                self.gap(
                    context,
                    "Inline JavaScript requires a separate source adapter",
                );
                return;
            }
            let preload = [
                "--import",
                "--require",
                "-r",
                "--loader",
                "--experimental-loader",
            ]
            .iter()
            .find_map(|flag| {
                if arg == flag {
                    let value = args.get(i).cloned();
                    i += 1;
                    Some(value)
                } else {
                    arg.strip_prefix(&format!("{flag}="))
                        .map(|v| Some(v.into()))
                }
            });
            if let Some(value) = preload {
                if let Some(value) = value {
                    if !value.starts_with('.') && !value.starts_with('/') {
                        self.package(context, &value, false, PackageUseKind::Preload);
                    }
                    self.target(context, value);
                } else {
                    self.gap(context, "Missing Node preload argument");
                }
                continue;
            }
            if arg == "--" {
                if let Some(value) = args.get(i).filter(|value| value.as_str() != "-") {
                    self.target(context, relative(value));
                    return;
                }
                break;
            }
            if matches!(
                arg.as_str(),
                "--enable-source-maps"
                    | "--no-warnings"
                    | "--trace-warnings"
                    | "--watch"
                    | "--experimental-strip-types"
                    | "--inspect"
                    | "--inspect-brk"
            ) || arg.starts_with("--inspect=")
            {
                continue;
            }
            if arg.starts_with('-') {
                self.gap(context, "Unsupported Node option");
                return;
            }
            self.target(context, relative(arg));
            return;
        }
        if entry_required {
            self.gap(
                context,
                "Node stdin or interactive entry requires a source adapter",
            );
        }
    }
    fn manager(&mut self, context: &Context, manager: &str, args: &[String], depth: usize) {
        if depth > 64 {
            self.gap(context, "Static command limit");
            return;
        }
        let mut context = context.clone();
        let mut i = 0;
        while let Some(arg) = args.get(i) {
            if matches!(
                arg.as_str(),
                "--prefix" | "--dir" | "--cwd" | "-C" | "--workspace" | "-w" | "--filter"
            ) {
                let Some(value) = args.get(i + 1) else {
                    self.gap(&context, "Missing package manager scope");
                    return;
                };
                if matches!(arg.as_str(), "--workspace" | "-w" | "--filter") {
                    let Some(ws) = self
                        .inventory
                        .workspaces
                        .iter()
                        .find(|w| w.name.as_deref() == Some(value) || w.path.as_str() == value)
                    else {
                        self.gap(&context, "Unknown workspace selector");
                        return;
                    };
                    context.workspace = ws.workspace;
                    context.cwd = ws.path.clone();
                } else {
                    context.cwd = normalize_path(&context.cwd.join(value));
                    if let Some(ws) = self
                        .inventory
                        .workspaces
                        .iter()
                        .find(|w| normalize_path(&w.path) == context.cwd)
                    {
                        context.workspace = ws.workspace;
                    } else {
                        self.gap(&context, "Cwd has no owned workspace manifest");
                        return;
                    }
                }
                i += 2;
            } else {
                break;
            }
        }
        let args = &args[i..];
        let Some(first) = args.first() else {
            self.gap(&context, "Unsupported package manager command");
            return;
        };
        if manager == "yarn" && first == "workspace" {
            let Some(name) = args.get(1) else {
                self.gap(&context, "Missing workspace selector");
                return;
            };
            let Some(ws) = self
                .inventory
                .workspaces
                .iter()
                .find(|w| w.name.as_deref() == Some(name))
            else {
                self.gap(&context, "Unknown workspace selector");
                return;
            };
            context.workspace = ws.workspace;
            context.cwd = ws.path.clone();
            self.manager(&context, manager, &args[2..], depth + 1);
            return;
        }
        if matches!(manager, "npx" | "pnpx")
            || first == "dlx"
            || manager == "npm" && matches!(first.as_str(), "exec" | "x")
        {
            let args = if matches!(first.as_str(), "dlx" | "exec" | "x") {
                &args[1..]
            } else {
                args
            };
            self.on_demand(&mut context, manager, args, first == "dlx", depth);
            return;
        }
        if first == "exec" {
            let args = strip_separator(&args[1..]);
            self.command(&mut context, args, depth);
            return;
        }
        if first == "run"
            || first == "run-script"
            || manager == "npm" && matches!(first.as_str(), "rum" | "urn")
        {
            if let Some(name) = args.get(1) {
                self.script(context, name, strip_separator(&args[2..]), depth + 1);
            } else {
                self.gap(&context, "Missing script name");
            }
        } else if matches!(
            first.as_str(),
            "install"
                | "add"
                | "remove"
                | "update"
                | "publish"
                | "config"
                | "pack"
                | "version"
                | "list"
                | "audit"
        ) {
            self.gap(
                &context,
                "Package manager lifecycle and configuration adapters are unsupported",
            );
        } else if manager == "yarn" && first == "node" {
            self.command(&mut context, args, depth);
        } else if !first.starts_with('-')
            && (manager != "npm"
                || matches!(
                    first.as_str(),
                    "start" | "stop" | "restart" | "test" | "t" | "tst"
                ))
        {
            let name = if matches!(first.as_str(), "t" | "tst") {
                "test"
            } else {
                first
            };
            self.script(context, name, strip_separator(&args[1..]), depth + 1);
        } else {
            self.gap(&context, "Unsupported package manager command or option");
        }
    }
    fn on_demand(
        &mut self,
        context: &mut Context,
        manager: &str,
        args: &[String],
        dlx: bool,
        depth: usize,
    ) {
        let mut i = 0;
        let mut optional = true;
        let mut supplied = false;
        while let Some(arg) = args.get(i) {
            match arg.as_str() {
                "--no" | "--no-install" | "--no-yes" | "--yes=false" => {
                    optional = false;
                    i += 1;
                }
                "--yes" | "-y" => {
                    i += 1;
                }
                "--package" | "-p" => {
                    if let Some(spec) = args.get(i + 1) {
                        self.package(context, spec, true, PackageUseKind::OnDemand);
                        supplied = true;
                        i += 2;
                    } else {
                        self.gap(context, "Missing requested package");
                        return;
                    }
                }
                "--call" | "-c" => {
                    if let Some(body) = args.get(i + 1) {
                        self.shell(context.clone(), body, &[], depth + 1);
                    } else {
                        self.gap(context, "Missing package command");
                    }
                    return;
                }
                "--" => {
                    i += 1;
                    break;
                }
                _ if arg.starts_with("--package=") => {
                    self.package(context, &arg[10..], true, PackageUseKind::OnDemand);
                    supplied = true;
                    i += 1;
                }
                _ => break,
            }
        }
        let Some(command) = args.get(i) else {
            return;
        };
        if command.starts_with('-') {
            self.gap(context, "Unsupported on-demand package arguments");
            return;
        }
        if !supplied
            && (dlx || manager == "pnpx" || command.starts_with('@') || command.contains('@'))
        {
            self.package(context, command, optional, PackageUseKind::OnDemand);
            self.gap(
                context,
                "Tool-specific entry and configuration arguments are unsupported",
            );
        } else {
            let start = self.usage.packages.len();
            let bin_start = self.usage.binaries.len();
            self.command(context, &args[i..], depth + 1);
            for package in &mut self.usage.packages[start..] {
                package.optional = optional;
            }
            for binary in &mut self.usage.binaries[bin_start..] {
                binary.optional = optional;
            }
        }
    }

    fn binary(&mut self, context: &Context, binary: &str, args: &[String]) {
        if binary.starts_with('.') || binary.starts_with('/') {
            self.target(context, binary.into());
            self.gap(context, "Executable file arguments require a tool adapter");
            return;
        }
        let providers: Vec<_> = self
            .inventory
            .binary_providers
            .iter()
            .filter(|p| p.workspace == context.workspace && p.binary == binary)
            .cloned()
            .collect();
        for provider in &providers {
            if !provider.available
                && !provider.metadata_known
                && let Some(file) = provider.manifest
            {
                self.usage.gaps.push(ScriptGap {
                    producer: context.producer.clone(),
                    mode: self.mode,
                    gap: CoverageGap {
                        producer: "binary provider metadata".into(),
                        domain: crate::coverage::TargetDomain::Unbounded,
                        reason: crate::coverage::GapReason::UnavailableInput(
                            "Installed binary provider manifest is incomplete".into(),
                        ),
                        location: Some(Location {
                            file,
                            range: Default::default(),
                        }),
                    },
                });
            }
            if provider.available
                && let Some(name) = provider
                    .requested_package
                    .as_ref()
                    .or(provider.package_name.as_ref())
            {
                self.package(context, name, false, PackageUseKind::Binary);
            }
        }
        self.usage.binaries.push(BinaryUse {
            producer: context.producer.clone(),
            workspace: context.workspace,
            location: context.location,
            mode: self.mode,
            optional: false,
            binary: binary.into(),
            providers: providers.clone(),
        });
        let settings = self
            .inventory
            .workspaces
            .iter()
            .find(|w| w.workspace == context.workspace)
            .and_then(|w| serde_json::from_str::<serde_json::Value>(&w.configuration_json).ok())
            .unwrap_or_default();
        let ignored = settings
            .get("ignoreBinaries")
            .and_then(|v| v.as_array())
            .is_some_and(|values| values.iter().any(|v| v.as_str() == Some(binary)));
        let ignore_global = settings
            .get("ignoreGlobalBinaries")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let global = matches!(
            binary,
            "echo"
                | "printf"
                | "cat"
                | "cp"
                | "mv"
                | "rm"
                | "mkdir"
                | "rmdir"
                | "touch"
                | "git"
                | "sh"
                | "bash"
                | "true"
                | "false"
                | "exit"
                | "pwd"
                | "sleep"
                | "test"
        );
        if providers.iter().all(|p| !p.available) && !ignored && !(ignore_global && global) {
            self.gap(context, "Installed binary provider metadata is unavailable");
        }
        if !global || matches!(binary, "sh" | "bash" | "git") || !args.is_empty() && binary == "env"
        {
            self.gap(
                context,
                "Tool-specific entry and configuration arguments are unsupported",
            );
        }
    }
}

fn relative(value: &str) -> String {
    if value.starts_with('.') || value.starts_with('/') {
        value.into()
    } else {
        format!("./{value}")
    }
}
fn normalize_path(path: &camino::Utf8Path) -> Utf8PathBuf {
    let mut result = Utf8PathBuf::new();
    for component in path.components() {
        match component {
            camino::Utf8Component::CurDir => {}
            camino::Utf8Component::ParentDir => {
                if result.file_name().is_some_and(|name| name != "..") {
                    result.pop();
                } else if !result.is_absolute() {
                    result.push("..");
                }
            }
            _ => result.push(component.as_str()),
        }
    }
    if result.as_str().is_empty() {
        result.push(".");
    }
    result
}
fn is_assignment(value: &str) -> bool {
    value.split_once('=').is_some_and(|(key, _)| {
        !key.is_empty()
            && key
                .chars()
                .enumerate()
                .all(|(i, c)| c == '_' || c.is_ascii_alphabetic() || i > 0 && c.is_ascii_digit())
    })
}
#[derive(Debug)]
enum Token {
    Word(String, bool),
    Operator(String),
}
fn tokenize(body: &str) -> (Vec<Token>, bool) {
    if body.len() > 1_048_576 {
        return (Vec::new(), true);
    }
    let mut tokens = Vec::new();
    let mut word = String::new();
    let mut started = false;
    let mut unknown = false;
    let mut invalid = false;
    let mut quote = None;
    let mut chars = body.chars().peekable();
    while let Some(mut c) = chars.next() {
        if c == '\\' && quote != Some('\'') {
            if let Some(next) = chars.next() {
                if next != '\n' {
                    word.push(next);
                    started = true;
                }
            } else {
                invalid = true;
            }
            continue;
        }
        if quote == Some(c) {
            quote = None;
            started = true;
            continue;
        }
        if quote.is_none() && matches!(c, '\'' | '"') {
            quote = Some(c);
            started = true;
            continue;
        }
        if quote != Some('\'') && matches!(c, '$' | '`') {
            unknown = true;
            invalid = true;
            started = true;
            if c == '$' && chars.peek() == Some(&'(') {
                chars.next();
                let mut depth = 1;
                for next in chars.by_ref() {
                    if next == '(' {
                        depth += 1;
                    }
                    if next == ')' {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                }
            } else if c == '`' {
                for next in chars.by_ref() {
                    if next == '`' {
                        break;
                    }
                }
            }
            continue;
        }
        if quote.is_none() && c == '#' && !started {
            for next in chars.by_ref() {
                if next == '\n' {
                    break;
                }
            }
            tokens.push(Token::Operator(";".into()));
            continue;
        }
        if quote.is_none() && c == '\n' {
            c = ';';
        }
        if quote.is_none()
            && matches!(c, '<' | '>')
            && started
            && word.chars().all(|c| c.is_ascii_digit())
        {
            word.clear();
            started = false;
        }
        if quote.is_none() && (c.is_whitespace() || ";&|()<>".contains(c)) {
            if started {
                tokens.push(Token::Word(std::mem::take(&mut word), unknown));
                started = false;
                unknown = false;
            }
            if !c.is_whitespace() {
                let mut op = c.to_string();
                if chars.peek() == Some(&c) && matches!(c, '&' | '|' | '>' | '<') {
                    op.push(chars.next().unwrap());
                }
                if op == "<<" || op == "&" {
                    invalid = true;
                }
                tokens.push(Token::Operator(op));
            }
            continue;
        }
        if quote.is_none() && matches!(c, '*' | '?' | '{' | '}' | '~') {
            unknown = true;
            invalid = true;
        }
        word.push(c);
        started = true;
    }
    if started {
        tokens.push(Token::Word(word, unknown || quote.is_some()));
    }
    (tokens, invalid || quote.is_some())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Activation {
    Dead,
    Possible,
    Definite,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProducerActivation {
    pub producer: ScriptProducer,
    pub activation: Activation,
}
impl ScriptUsage {
    pub fn producers(&self, mode: AnalysisMode) -> Vec<ScriptProducer> {
        let mut result = Vec::new();
        for (producer, record_mode) in self
            .targets
            .iter()
            .map(|r| (&r.producer, r.mode))
            .chain(self.packages.iter().map(|r| (&r.producer, r.mode)))
            .chain(self.binaries.iter().map(|r| (&r.producer, r.mode)))
            .chain(self.catalogs.iter().map(|r| (&r.producer, r.mode)))
            .chain(self.gaps.iter().map(|r| (&r.producer, r.mode)))
        {
            if mode == record_mode && !result.contains(producer) {
                result.push(producer.clone());
            }
        }
        result
    }
}

fn strip_separator(args: &[String]) -> &[String] {
    if args.first().is_some_and(|s| s == "--") {
        &args[1..]
    } else {
        args
    }
}
