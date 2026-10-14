//! Module graph collection precedes resolution. Expansion never reaches the runtime.
use crate::{
    diagnostic::{CompileResult, Diagnostic, Severity, Span},
    frontend::parse_yl,
    ir::*,
    semantics::{
        MeaningAction, MeaningCall, MeaningCase, MeaningChain, MeaningDefinition, MeaningGroup,
        MeaningMatch, MeaningMessagePart, MeaningSelector, MeaningValue, OperationRegistry,
        ParameterKind,
    },
    syntax::*,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

type Result<T> = std::result::Result<T, Diagnostic>;
type Environment = BTreeMap<String, Value>;
#[derive(Clone)]
enum Value {
    Grammar(Term),
    Enum(String, String, Option<Span>),
}
#[derive(Clone)]
struct Symbol {
    module: String,
    name: String,
    scope: String,
    declaration: Declaration,
}
#[derive(Default)]
struct Compiler {
    registry: OperationRegistry,
    selected_patterns: BTreeSet<String>,
    modules: BTreeMap<String, Module>,
    dependencies: BTreeMap<(String, String), String>,
    imports: BTreeMap<(String, String), String>,
    namespaces: BTreeMap<(String, String), String>,
    symbols: BTreeMap<String, Symbol>,
    exports: BTreeMap<(String, String), String>,
    members: BTreeMap<(String, String), String>,
    extensions: Vec<(String, Declaration)>,
    expanding: Vec<String>,
    next_marker: u32,
    diagnostics: Vec<Diagnostic>,
}

pub fn compile_language(entry_file: impl AsRef<Path>) -> CompileResult<CompiledLanguage> {
    let absolute = std::fs::canonicalize(entry_file.as_ref()).map_err(|e| {
        vec![Diagnostic::error(
            "yl.io",
            e.to_string(),
            Span::new(&entry_file.as_ref().to_string_lossy(), 0, 0),
        )]
    })?;
    let root = absolute.parent().unwrap_or(Path::new(".")).to_path_buf();
    let mut compiler = Compiler::default();
    crate::semantics::queries::register(&mut compiler.registry).map_err(|error| {
        vec![Diagnostic::error(
            "yl.query_registry",
            error,
            Span::default(),
        )]
    })?;
    compiler
        .read_module(&absolute, &root)
        .map_err(|e| vec![e])?;
    compiler.compile().map_err(|e| vec![e])
}
/// In-memory graph API, also useful to hosts which do not use a filesystem.
/// Relative imports resolve against module IDs; omitted extensions become `.yl`.
pub fn compile_sources(
    entry: &str,
    sources: &BTreeMap<String, String>,
) -> CompileResult<CompiledLanguage> {
    compile_sources_with_registry(entry, sources, &OperationRegistry::new())
}
/// Compile a module graph with explicitly registered native operation libraries.
pub fn compile_sources_with_registry(
    entry: &str,
    sources: &BTreeMap<String, String>,
    registry: &OperationRegistry,
) -> CompileResult<CompiledLanguage> {
    for module in ["std/references", "std/variables", "std/types"] {
        if registry.contains_module(module) {
            return Err(vec![Diagnostic::error(
                "yl.reserved_module",
                format!("{module} is an authored standard module and cannot be replaced by native operations"),
                Span::new(entry, 0, 0),
            )]);
        }
    }
    let mut registry = registry.clone();
    if !registry.contains_module("std/query") {
        crate::semantics::queries::register(&mut registry).map_err(|error| {
            vec![Diagnostic::error(
                "yl.query_registry",
                error,
                Span::default(),
            )]
        })?;
    }
    let mut compiler = Compiler {
        registry,
        ..Compiler::default()
    };
    compiler.read_sources(entry, sources).map_err(|e| vec![e])?;
    compiler.compile().map_err(|e| vec![e])
}
fn module_id(path: &Path, root: &Path) -> String {
    let target: Vec<_> = path.components().collect();
    let base: Vec<_> = root.components().collect();
    let common = target.iter().zip(&base).take_while(|(a, b)| a == b).count();
    if common == 0 {
        return path.to_string_lossy().replace('\\', "/");
    }
    let mut relative = PathBuf::new();
    for _ in common..base.len() {
        relative.push("..");
    }
    for component in target.iter().skip(common) {
        relative.push(component.as_os_str());
    }
    relative.to_string_lossy().replace('\\', "/")
}
/// Reserved standard modules have stable IDs in filesystem and in-memory graphs.
/// The historical variables spelling denotes the same module, not a second copy.
fn standard_module(path: &str) -> Option<&'static str> {
    match path {
        "std/parser" => Some("std/parser"),
        "std/references" | "std/variables" => Some("std/references"),
        "std/types" => Some("std/types"),
        _ => None,
    }
}
fn standard_source(id: &str) -> Option<&'static str> {
    match id {
        "std/parser" => Some(include_str!("../stdlib/parser.yl")),
        "std/references" => Some(include_str!("../stdlib/references.yl")),
        "std/types" => Some(include_str!("../stdlib/types.yl")),
        _ => None,
    }
}
fn import_path(module: &str, path: &str) -> String {
    if let Some(id) = standard_module(path) {
        return id.into();
    }
    if path == "core/parser" {
        return path.into();
    }
    let mut parts = vec![];
    let parent = module.rsplit_once('/').map(|p| p.0).unwrap_or("");
    for part in format!("{parent}/{path}").split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if parts.last().is_some_and(|p| *p != "..") {
                    parts.pop();
                } else {
                    parts.push(part.to_owned());
                }
            }
            _ => parts.push(part.to_owned()),
        }
    }
    let mut path = parts.join("/");
    if !path.ends_with(".yl") {
        path.push_str(".yl");
    }
    path
}
impl Compiler {
    fn validate_declarations(&self) -> Result<()> {
        for symbol in self.symbols.values() {
            let (parameters, grammars) = match &symbol.declaration.kind {
                DeclKind::Pattern {
                    parameters,
                    grammar,
                    ..
                } => (parameters.as_slice(), vec![grammar]),
                DeclKind::Pipe {
                    parameters, arms, ..
                } => {
                    let mut names = BTreeSet::new();
                    for parameter in parameters {
                        if !names.insert(parameter.name.clone()) {
                            return Err(Diagnostic::error(
                                "yl.duplicate_member",
                                "Duplicate parameter",
                                parameter.span.clone(),
                            ));
                        }
                    }
                    for arm in arms {
                        let mut locals = names.clone();
                        locals.insert(arm.binding.clone());
                        if let Some(ty) = &arm.ty {
                            self.resolve(&symbol.module, &symbol.scope, ty, &arm.span)?;
                        }
                        match &arm.body {
                            RewriteBody::Direct(body) => {
                                self.validate_expr(symbol, body, &locals)?
                            }
                            RewriteBody::Match { cases, .. } => {
                                for case in cases {
                                    if let Some(body) = &case.replacement {
                                        self.validate_expr(symbol, body, &locals)?;
                                    }
                                }
                            }
                            RewriteBody::Cases(cases) => {
                                for (_, body) in cases {
                                    self.validate_expr(symbol, body, &locals)?;
                                }
                            }
                        }
                    }
                    (parameters.as_slice(), vec![])
                }
                DeclKind::Node {
                    grammar: Some(grammar),
                    ..
                } => ([].as_slice(), vec![grammar]),
                DeclKind::Constraint { parameters, .. } => (parameters.as_slice(), vec![]),
                _ => ([].as_slice(), vec![]),
            };
            let mut locals = BTreeSet::new();
            for parameter in parameters {
                if !locals.insert(parameter.name.clone()) {
                    return Err(Diagnostic::error(
                        "yl.duplicate_member",
                        "Duplicate parameter",
                        parameter.span.clone(),
                    ));
                }
                if let Some(ty) = &parameter.ty {
                    self.resolve(&symbol.module, &symbol.scope, ty, &parameter.span)?;
                }
                if let Some(default) = &parameter.default {
                    let condition_literal = matches!(
                        symbol.declaration.kind,
                        DeclKind::Constraint { .. }
                    ) && matches!(&default.kind,ExprKind::Ref(name) if matches!(name.as_str(),"true"|"false"|"absent"));
                    if !condition_literal {
                        self.validate_expr(symbol, default, &locals)?;
                    }
                }
            }
            for grammar in grammars {
                self.validate_expr(symbol, grammar, &locals)?;
            }
        }
        Ok(())
    }
    fn validate_pipe_matches(&mut self) -> Result<()> {
        for symbol in self.symbols.clone().values() {
            let DeclKind::Pipe {
                parameters, arms, ..
            } = &symbol.declaration.kind
            else {
                continue;
            };
            let mut bound = Environment::new();
            for p in parameters {
                if let Some(ty) = &p.ty {
                    let id = self.resolve(&symbol.module, &symbol.scope, ty, &p.span)?;
                    if let Some(Symbol {
                        declaration:
                            Declaration {
                                kind: DeclKind::Enum { variants, .. },
                                ..
                            },
                        ..
                    }) = self.symbols.get(&id)
                    {
                        let variant = variants.first().ok_or_else(|| {
                            Diagnostic::error(
                                "yl.enum_selector",
                                "A selected enum needs at least one variant",
                                p.span.clone(),
                            )
                        })?;
                        bound.insert(
                            p.name.clone(),
                            Value::Enum(id.clone(), variant.clone(), None),
                        );
                    }
                }
            }
            for arm in arms {
                match &arm.body {
                    RewriteBody::Match { selectors, cases } => {
                        self.select_case(
                            symbol, selectors, cases, &bound, parameters, &arm.span, false,
                        )?;
                    }
                    RewriteBody::Cases(original) => {
                        let cases: Vec<_> = original
                            .iter()
                            .map(|(pattern, body)| RewriteCase {
                                patterns: vec![Some(pattern.clone())],
                                replacement: Some(body.clone()),
                                diagnostics: vec![],
                                span: pattern.span.clone(),
                            })
                            .collect();
                        self.select_case(
                            symbol,
                            &[],
                            &cases,
                            &bound,
                            parameters,
                            &arm.span,
                            false,
                        )?;
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }
    fn validate_expr(&self, symbol: &Symbol, expr: &Expr, locals: &BTreeSet<String>) -> Result<()> {
        match &expr.kind {
            ExprKind::Ref(name) => {
                if !locals.contains(name) {
                    if let Some((parent, _)) = name.rsplit_once("::") {
                        if self
                            .resolve(&symbol.module, &symbol.name, parent, &expr.span)
                            .ok()
                            .and_then(|id| self.symbols.get(&id))
                            .is_some_and(|s| matches!(s.declaration.kind, DeclKind::Enum { .. }))
                        {
                            return Ok(());
                        }
                    }
                    self.resolve(&symbol.module, &symbol.name, name, &expr.span)?;
                }
            }
            ExprKind::Call(name, args) => {
                if !locals.contains(name) {
                    self.resolve(&symbol.module, &symbol.name, name, &expr.span)?;
                }
                for arg in args {
                    self.validate_expr(symbol, &arg.value, locals)?;
                }
            }
            ExprKind::Pipe(input, name, args) => {
                self.validate_expr(symbol, input, locals)?;
                self.resolve(&symbol.module, &symbol.name, name, &expr.span)?;
                for arg in args {
                    self.validate_expr(symbol, &arg.value, locals)?;
                }
            }
            ExprKind::Sequence(items) | ExprKind::Choice(items) => {
                for item in items {
                    self.validate_expr(symbol, item, locals)?;
                }
            }
            ExprKind::Capture(_, inner) | ExprKind::Repeat(inner, _) | ExprKind::Group(inner) => {
                self.validate_expr(symbol, inner, locals)?
            }
            ExprKind::Regex(regex) => {
                regex::Regex::new(regex).map_err(|e| {
                    Diagnostic::error("yl.invalid_regex", e.to_string(), expr.span.clone())
                })?;
            }
            _ => {}
        }
        Ok(())
    }
    fn read_sources(&mut self, id: &str, sources: &BTreeMap<String, String>) -> Result<()> {
        if self.modules.contains_key(id) || id == "core/parser" {
            return Ok(());
        }
        if self.modules.len() >= 256 {
            return Err(Diagnostic::error(
                "yl.module_limit",
                "Module graph exceeds resource limit",
                Span::new(id, 0, 0),
            ));
        }
        let source = if let Some(source) = standard_source(id) {
            source
        } else {
            sources.get(id).ok_or_else(|| {
                Diagnostic::error(
                    "yl.unresolved_import",
                    format!("Missing module {id}"),
                    Span::new(id, 0, 0),
                )
            })?
        };
        let module = parse_yl(id, source).map_err(|e| {
            e.into_iter().next().unwrap_or_else(|| {
                Diagnostic::error("yl.parse", "YL parser failed", Span::new(id, 0, 0))
            })
        })?;
        let dependencies: Vec<_> = module
            .declarations
            .iter()
            .filter_map(|d| {
                if let DeclKind::Import { path, .. } | DeclKind::NamespaceImport { path, .. } =
                    &d.kind
                {
                    Some((path.clone(), import_path(id, path), d.span.clone()))
                } else {
                    None
                }
            })
            .collect();
        self.modules.insert(id.into(), module);
        for (specifier, dependency, span) in dependencies {
            let dependency = if self.registry.contains_module(&specifier) {
                specifier.clone()
            } else {
                dependency
            };
            self.dependencies
                .insert((id.into(), specifier), dependency.clone());
            if self.registry.contains_module(&dependency) {
                continue;
            }
            self.read_sources(&dependency, sources).map_err(|mut e| {
                e.secondary.push(*e.primary.clone());
                e.primary = Box::new(span);
                e
            })?;
        }
        Ok(())
    }
    fn read_module(&mut self, path: &Path, root: &Path) -> Result<()> {
        let id = module_id(path, root);
        if self.modules.contains_key(&id) {
            return Ok(());
        }
        if self.modules.len() >= 256 {
            return Err(Diagnostic::error(
                "yl.module_limit",
                "Module graph exceeds resource limit",
                Span::new(&id, 0, 0),
            ));
        }
        let source = std::fs::read_to_string(path).map_err(|e| {
            Diagnostic::error("yl.unresolved_import", e.to_string(), Span::new(&id, 0, 0))
        })?;
        let module = parse_yl(&id, &source).map_err(|e| {
            e.into_iter().next().unwrap_or_else(|| {
                Diagnostic::error("yl.parse", "YL parser failed", Span::new(&id, 0, 0))
            })
        })?;
        let dependencies: Vec<_> = module
            .declarations
            .iter()
            .filter_map(|d| {
                if let DeclKind::Import { path, .. } | DeclKind::NamespaceImport { path, .. } =
                    &d.kind
                {
                    Some((path.clone(), d.span.clone()))
                } else {
                    None
                }
            })
            .collect();
        self.modules.insert(id.clone(), module);
        for (dependency, span) in dependencies {
            if let Some(standard) = standard_module(&dependency) {
                self.dependencies
                    .insert((id.clone(), dependency), standard.into());
                self.read_sources(standard, &BTreeMap::new())
                    .map_err(|mut error| {
                        error.secondary.push(*error.primary.clone());
                        error.primary = Box::new(span);
                        error
                    })?;
                continue;
            }
            if self.registry.contains_module(&dependency) {
                self.dependencies
                    .insert((id.clone(), dependency.clone()), dependency);
                continue;
            }
            if dependency == "core/parser" {
                self.dependencies
                    .insert((id.clone(), dependency.clone()), dependency);
                continue;
            }
            let mut dep = path.parent().unwrap_or(root).join(&dependency);
            if dep.extension().is_none() {
                dep.set_extension("yl");
            }
            let dep = std::fs::canonicalize(dep).map_err(|e| {
                Diagnostic::error("yl.unresolved_import", format!("{dependency}: {e}"), span)
            })?;
            self.dependencies
                .insert((id.clone(), dependency), module_id(&dep, root));
            self.read_module(&dep, root)?;
        }
        Ok(())
    }
    fn register(
        &mut self,
        module: &str,
        scope: &str,
        declaration: &Declaration,
    ) -> Result<Option<String>> {
        let name = match &declaration.kind {
            DeclKind::Operation { name, .. }
            | DeclKind::Symbol { name }
            | DeclKind::Node { name, .. }
            | DeclKind::Pattern { name, .. }
            | DeclKind::Pipe { name, .. }
            | DeclKind::Enum { name, .. }
            | DeclKind::Constraint { name, .. } => name,
            DeclKind::Extend { .. } => {
                self.extensions.push((module.into(), declaration.clone()));
                return Ok(None);
            }
            _ => return Ok(None),
        };
        if let DeclKind::Operation { parameters, .. } = &declaration.kind {
            let mut names = BTreeSet::new();
            for parameter in parameters {
                if !names.insert(&parameter.name) {
                    return Err(Diagnostic::error(
                        "yl.duplicate_parameter",
                        "Duplicate operation parameter",
                        parameter.span.clone(),
                    ));
                }
            }
        }
        let local = if scope.is_empty() {
            name.clone()
        } else {
            format!("{scope}::{name}")
        };
        let id = format!("{module}#{local}");
        if let Some(previous) = self.symbols.get(&id) {
            let mut error = Diagnostic::error(
                "yl.duplicate_declaration",
                format!("Duplicate declaration {local}"),
                declaration.span.clone(),
            );
            error.secondary.push(previous.declaration.span.clone());
            return Err(error);
        }
        self.symbols.insert(
            id.clone(),
            Symbol {
                module: module.into(),
                name: local.clone(),
                scope: scope.into(),
                declaration: declaration.clone(),
            },
        );
        if declaration.export
            && self
                .exports
                .insert((module.into(), name.clone()), id.clone())
                .is_some()
        {
            return Err(Diagnostic::error(
                "yl.duplicate_declaration",
                "Duplicate exported name",
                declaration.span.clone(),
            ));
        }
        if let DeclKind::Node {
            grammar, members, ..
        } = &declaration.kind
        {
            if grammar.is_some() && !members.is_empty() {
                return Err(Diagnostic::error(
                    "yl.invalid_node",
                    "Concrete nodes cannot declare members",
                    declaration.span.clone(),
                ));
            }
            let mut names = BTreeSet::new();
            for member in members {
                let DeclKind::Node {
                    name: member_name,
                    grammar,
                    members,
                    ..
                } = &member.kind
                else {
                    return Err(Diagnostic::error(
                        "yl.invalid_member",
                        "Only nodes/trivia can be abstract members",
                        member.span.clone(),
                    ));
                };
                if !names.insert(member_name.clone()) {
                    return Err(Diagnostic::error(
                        "yl.duplicate_member",
                        format!("Duplicate member {member_name}"),
                        member.span.clone(),
                    ));
                }
                if grammar.is_some() || !members.is_empty() {
                    if let Some(child) = self.register(module, &local, member)? {
                        self.members
                            .insert((id.clone(), member_name.clone()), child);
                    }
                }
            }
        }
        if let DeclKind::Enum { variants, .. } = &declaration.kind {
            let mut seen = BTreeSet::new();
            for v in variants {
                if !seen.insert(v) {
                    return Err(Diagnostic::error(
                        "yl.duplicate_member",
                        format!("Duplicate enum variant {v}"),
                        declaration.span.clone(),
                    ));
                }
            }
        }
        Ok(Some(id))
    }
    fn resolve(&self, module: &str, scope: &str, name: &str, span: &Span) -> Result<String> {
        self.resolve_depth(module, scope, name, span, 0)
    }
    fn resolve_depth(
        &self,
        module: &str,
        scope: &str,
        name: &str,
        span: &Span,
        depth: usize,
    ) -> Result<String> {
        if depth > 128 {
            return Err(Diagnostic::error(
                "yl.membership_cycle",
                "Cyclic membership path",
                span.clone(),
            ));
        }
        // Canonical names are generated only by hygienic wrapper expansion.
        if self.symbols.contains_key(name)
            || self.registry.signature(name).is_some()
            || self.registry.symbols.values().any(|id| id == name)
        {
            return Ok(name.into());
        }
        if let Some((namespace, member)) = name.split_once("::") {
            if let Some(from) = self.namespaces.get(&(module.into(), namespace.into())) {
                if self.registry.contains_module(from) {
                    return self
                        .registry
                        .operation_id(from, member)
                        .or_else(|| self.registry.symbol_id(from, member))
                        .map(str::to_owned)
                        .ok_or_else(|| {
                            Diagnostic::error(
                                "yl.non_exported_import",
                                format!("{from} does not export {member}"),
                                span.clone(),
                            )
                        });
                }
                let (head, tail) = member
                    .split_once("::")
                    .map_or((member, None), |(head, tail)| (head, Some(tail)));
                let root = self
                    .exports
                    .get(&(from.clone(), head.into()))
                    .ok_or_else(|| {
                        Diagnostic::error(
                            "yl.non_exported_import",
                            format!("{from} does not export {head}"),
                            span.clone(),
                        )
                    })?;
                return if let Some(tail) = tail {
                    self.member_path(root, tail, span, depth + 1)
                } else {
                    Ok(root.clone())
                };
            }
        }
        let original_scope = scope;
        let mut scope = scope.to_owned();
        loop {
            let id = if scope.is_empty() {
                format!("{module}#{name}")
            } else {
                format!("{module}#{scope}::{name}")
            };
            if self.symbols.contains_key(&id) {
                return Ok(id);
            }
            if scope.is_empty() {
                break;
            }
            scope = scope
                .rsplit_once("::")
                .map(|x| x.0.to_owned())
                .unwrap_or_default();
        }
        if let Some(id) = self.imports.get(&(module.into(), name.into())) {
            return Ok(id.clone());
        }
        if let Some((head, tail)) = name.split_once("::") {
            let parent = self.resolve_depth(module, original_scope, head, span, depth + 1)?;
            return self.member_path(&parent, tail, span, depth + 1);
        }
        Err(Diagnostic::error(
            "yl.unknown_reference",
            format!("Unknown declaration {name}"),
            span.clone(),
        ))
    }
    fn member_path(&self, parent: &str, tail: &str, span: &Span, depth: usize) -> Result<String> {
        if depth > 128 {
            return Err(Diagnostic::error(
                "yl.membership_cycle",
                "Cyclic membership",
                span.clone(),
            ));
        }
        let (head, rest) = tail
            .split_once("::")
            .map(|(a, b)| (a, Some(b)))
            .unwrap_or((tail, None));
        let child = if let Some(child) = self.members.get(&(parent.into(), head.into())) {
            child.clone()
        } else {
            let symbol = self.symbols.get(parent).ok_or_else(|| {
                Diagnostic::error("yl.unknown_reference", "Unknown parent", span.clone())
            })?;
            let DeclKind::Node { members, .. } = &symbol.declaration.kind else {
                return Err(Diagnostic::error(
                    "yl.unknown_reference",
                    "Not an abstract node",
                    span.clone(),
                ));
            };
            let Some(member) = members
                .iter()
                .find(|d| matches!(&d.kind,DeclKind::Node { name,.. } if name==head))
            else {
                return Err(Diagnostic::error(
                    "yl.unknown_reference",
                    format!("Unknown member {head}"),
                    span.clone(),
                ));
            };
            self.resolve_depth(&symbol.module, &symbol.scope, head, &member.span, depth + 1)?
        };
        if let Some(rest) = rest {
            self.member_path(&child, rest, span, depth + 1)
        } else {
            Ok(child)
        }
    }
    fn compile(&mut self) -> Result<CompiledLanguage> {
        for (module, ast) in self.modules.clone() {
            for declaration in &ast.declarations {
                self.register(&module, "", declaration)?;
            }
        }
        for (module, ast) in &self.modules {
            for declaration in &ast.declarations {
                if let DeclKind::NamespaceImport { name, path } = &declaration.kind {
                    let from = self.dependencies[&(module.clone(), path.clone())].clone();
                    if self.symbols.contains_key(&format!("{module}#{name}"))
                        || self
                            .namespaces
                            .insert((module.clone(), name.clone()), from)
                            .is_some()
                    {
                        return Err(Diagnostic::error(
                            "yl.duplicate_declaration",
                            "Namespace import conflicts with a declaration",
                            declaration.span.clone(),
                        ));
                    }
                }
            }
        }
        // Import cycles are safe: exports exist before any import is resolved.
        let mut pending = vec![];
        for (module, ast) in &self.modules {
            for d in &ast.declarations {
                if let DeclKind::Import { names, path } = &d.kind {
                    for (name, alias) in names {
                        pending.push((
                            module.clone(),
                            self.dependencies
                                .get(&(module.clone(), path.clone()))
                                .cloned()
                                .ok_or_else(|| {
                                    Diagnostic::error(
                                        "yl.unresolved_import",
                                        "Module graph has no resolved import edge",
                                        d.span.clone(),
                                    )
                                })?,
                            name.clone(),
                            alias.clone(),
                            d.span.clone(),
                            d.export,
                        ));
                    }
                }
            }
        }
        // Root imports first; qualified imports can depend on those memberships.
        pending.sort_by_key(|(_, _, name, _, _, _)| name.matches("::").count());
        while !pending.is_empty() {
            let previous = pending.len();
            let mut delayed = vec![];
            for (module, from, name, alias, span, exported) in pending {
                if !self.registry.contains_module(&from) && from != "core/parser" {
                    let head = name.split("::").next().unwrap_or(&name);
                    if !self.exports.contains_key(&(from.clone(), head.into())) {
                        delayed.push((module, from, name, alias, span, exported));
                        continue;
                    }
                }
                let id = if self.registry.contains_module(&from) {
                    self.registry
                        .operation_id(&from, &name)
                        .or_else(|| self.registry.symbol_id(&from, &name))
                        .ok_or_else(|| {
                            Diagnostic::error(
                                "yl.non_exported_import",
                                format!("{from} does not export {name}"),
                                span.clone(),
                            )
                        })?
                        .to_owned()
                } else if from == "core/parser" {
                    if !["notAhead", "notBehind"].contains(&name.as_str()) {
                        return Err(Diagnostic::error(
                            "yl.unknown_reference",
                            "Unknown core primitive",
                            span,
                        ));
                    }
                    format!("core/parser#{name}")
                } else {
                    let (head, tail) = name
                        .split_once("::")
                        .map(|(h, t)| (h, Some(t)))
                        .unwrap_or((&name, None));
                    let root = self
                        .exports
                        .get(&(from.clone(), head.into()))
                        .ok_or_else(|| {
                            Diagnostic::error(
                                "yl.non_exported_import",
                                format!("{from} does not export {head}"),
                                span.clone(),
                            )
                        })?
                        .clone();
                    if let Some(tail) = tail {
                        self.member_path(&root, tail, &span, 0)?
                    } else {
                        root
                    }
                };
                if exported {
                    self.exports
                        .insert((module.clone(), alias.clone()), id.clone());
                }
                if self
                    .namespaces
                    .contains_key(&(module.clone(), alias.clone()))
                    || self.symbols.contains_key(&format!("{module}#{alias}"))
                    || self.imports.insert((module, alias), id).is_some()
                {
                    return Err(Diagnostic::error(
                        "yl.duplicate_declaration",
                        "Import conflicts with a local/imported declaration",
                        span,
                    ));
                }
            }
            if delayed.len() == previous {
                let (_, from, name, _, span, _) = &delayed[0];
                return Err(Diagnostic::error(
                    "yl.non_exported_import",
                    format!("{from} does not export {name}"),
                    span.clone(),
                ));
            }
            pending = delayed;
        }
        self.validate_declarations()?;
        self.validate_pipe_matches()?;
        self.validate_semantic_wrappers()?;
        let mut entries = vec![];
        for (module, ast) in &self.modules {
            for d in &ast.declarations {
                if let DeclKind::Entry { node, trivia } = &d.kind {
                    let ExprKind::Ref(name) = &node.kind else {
                        return Err(Diagnostic::error(
                            "yl.entrypoint",
                            "Expected entry node",
                            node.span.clone(),
                        ));
                    };
                    let entry = self.resolve(module, "", name, &node.span)?;
                    let mut selected = vec![];
                    for matcher in trivia {
                        let ExprKind::Ref(name) = &matcher.kind else {
                            return Err(Diagnostic::error(
                                "yl.entry_trivia",
                                "Expected trivia reference",
                                matcher.span.clone(),
                            ));
                        };
                        let id = self.resolve(module, "", name, &matcher.span)?;
                        if !self.symbols.get(&id).is_some_and(|s| {
                            matches!(s.declaration.kind, DeclKind::Node { trivia: true, .. })
                        }) {
                            return Err(Diagnostic::error(
                                "yl.entry_trivia",
                                "Entry trivia selection must name a trivia declaration",
                                matcher.span.clone(),
                            ));
                        }
                        if !selected.contains(&id) {
                            selected.push(id);
                        }
                    }
                    entries.push((entry, selected));
                }
            }
        }
        if entries.len() != 1 {
            return Err(Diagnostic::error(
                "yl.entrypoint",
                format!("Expected exactly one entrypoint, found {}", entries.len()),
                self.modules
                    .values()
                    .next()
                    .map(|m| m.span.clone())
                    .unwrap_or_default(),
            ));
        }
        let (entry, trivia) = entries.remove(0);
        let mut language = CompiledLanguage {
            version: 1,
            entry,
            rules: BTreeMap::new(),
            trivia,
            meanings: BTreeMap::new(),
            query_meanings: BTreeMap::new(),
            examples: BTreeMap::new(),
            diagnostics: vec![],
        };
        for (id, symbol) in &self.symbols {
            let meanings = match &symbol.declaration.kind {
                DeclKind::Node { meanings, .. } | DeclKind::Pattern { meanings, .. } => meanings,
                _ => continue,
            };
            if let Some(meanings) = meanings {
                let query_groups = self.lower_query_meanings(symbol, meanings)?;
                if !query_groups.is_empty() {
                    language.query_meanings.insert(id.clone(), query_groups);
                }
                language
                    .meanings
                    .insert(id.clone(), self.lower_meanings(symbol, meanings)?);
            }
        }
        // Every named pattern is a real occurrence level in semantic languages,
        // including wrappers not themselves mentioned by any selector. Syntax-only
        // artifacts retain their existing sugar-free representation.
        if !language.meanings.is_empty() {
            for (id, symbol) in &self.symbols {
                if matches!(symbol.declaration.kind, DeclKind::Pattern { .. }) {
                    self.selected_patterns.insert(id.clone());
                    language.meanings.entry(id.clone()).or_default();
                }
            }
        }
        for (id, symbol) in self.symbols.clone() {
            if let DeclKind::Node {
                trivia,
                grammar,
                members,
                precedence,
                constraints,
                ..
            } = &symbol.declaration.kind
            {
                let body = if let Some(grammar) = grammar {
                    RuleBody::Concrete(self.lower(
                        &symbol.module,
                        &symbol.name,
                        grammar,
                        &Environment::new(),
                        0,
                    )?)
                } else {
                    if members.is_empty() {
                        return Err(Diagnostic::error(
                            "yl.incomplete_declaration",
                            "Top-level node needs a grammar or members",
                            symbol.declaration.span.clone(),
                        ));
                    }
                    let mut bases = vec![];
                    let mut visiting = BTreeSet::new();
                    self.leaves(&id, &mut visiting, &mut bases)?;
                    let all_leaves = bases;
                    let mut bases = members
                        .iter()
                        .filter_map(|m| {
                            if let DeclKind::Node { name, .. } = &m.kind {
                                Some(self.member_path(&id, name, &m.span, 0))
                            } else {
                                None
                            }
                        })
                        .collect::<Result<Vec<_>>>()?;
                    let mut operators = vec![];
                    let mut listed = BTreeSet::new();
                    if precedence.is_empty() {
                        for member in &all_leaves {
                            let member_symbol = self.symbols[member].clone();
                            let DeclKind::Node {
                                grammar: Some(grammar),
                                ..
                            } = &member_symbol.declaration.kind
                            else {
                                continue;
                            };
                            // Candidate discovery does not emit a runtime term. Keep
                            // projection IDs and warnings owned by actual lowering.
                            let next_marker = self.next_marker;
                            let diagnostic_count = self.diagnostics.len();
                            let term = self.lower(
                                &member_symbol.module,
                                &member_symbol.name,
                                grammar,
                                &Environment::new(),
                                0,
                            );
                            self.next_marker = next_marker;
                            self.diagnostics.truncate(diagnostic_count);
                            let term = term?;
                            let left = edge_ref(&term, false) == Some(id.as_str());
                            let right = edge_ref(&term, true) == Some(id.as_str());
                            if left || right {
                                listed.insert(member.clone());
                                operators.push(Operator {
                                    rule: member.clone(),
                                    binding_power: 1,
                                    associativity: Associativity::Left,
                                    left,
                                    right,
                                });
                            }
                        }
                        if operators.len() > 1 {
                            let mut diagnostic = Diagnostic::error(
                                "yl.missing_precedence",
                                "Multiple recursive operators require explicit precedence",
                                symbol.declaration.span.clone(),
                            );
                            diagnostic.secondary = operators
                                .iter()
                                .map(|operator| {
                                    self.symbols[&operator.rule].declaration.span.clone()
                                })
                                .collect();
                            diagnostic.help = Some(
                                "Add a precedence block listing the recursive operators in binding order"
                                    .into(),
                            );
                            return Err(diagnostic);
                        }
                    }
                    for (level_index, level) in precedence.iter().enumerate() {
                        for name in &level.members {
                            let member = self.member_path(&id, name, &level.span, 0).map_err(
                                |mut error| {
                                    error.code = "yl.invalid_precedence_member".into();
                                    error
                                },
                            )?;
                            if !all_leaves.contains(&member) || !listed.insert(member.clone()) {
                                return Err(Diagnostic::error(
                                    "yl.invalid_precedence_member",
                                    format!("Invalid/duplicate concrete precedence member {name}"),
                                    level.span.clone(),
                                ));
                            }
                            let member_symbol =
                                self.symbols.get(&member).cloned().ok_or_else(|| {
                                    Diagnostic::error(
                                        "yl.invalid_precedence_member",
                                        "Unknown member",
                                        level.span.clone(),
                                    )
                                })?;
                            let DeclKind::Node {
                                grammar: Some(grammar),
                                ..
                            } = &member_symbol.declaration.kind
                            else {
                                return Err(Diagnostic::error(
                                    "yl.invalid_precedence_member",
                                    "Precedence member needs concrete grammar",
                                    level.span.clone(),
                                ));
                            };
                            let term = self.lower(
                                &member_symbol.module,
                                &member_symbol.name,
                                grammar,
                                &Environment::new(),
                                0,
                            )?;
                            let left = edge_ref(&term, false) == Some(id.as_str());
                            let right = edge_ref(&term, true) == Some(id.as_str());
                            if !left && !right {
                                return Err(Diagnostic::error(
                                    "yl.invalid_precedence_member",
                                    "Precedence member must recurse at an edge",
                                    level.span.clone(),
                                ));
                            }
                            operators.push(Operator {
                                rule: member,
                                binding_power: precedence.len() - level_index,
                                associativity: level.associativity,
                                left,
                                right,
                            });
                        }
                    }
                    bases.retain(|b| !listed.contains(b));
                    RuleBody::Abstract { bases, operators }
                };
                let checks = self.lower_checks(
                    &symbol.module,
                    &symbol.name,
                    constraints,
                    &BTreeMap::new(),
                    0,
                )?;
                language.rules.insert(
                    id.clone(),
                    Rule {
                        name: symbol.name.clone(),
                        trivia: *trivia,
                        body,
                        constraints: checks,
                        span: symbol.declaration.span.clone(),
                    },
                );
            }
        }
        for (module, extension) in self.extensions.clone() {
            if let DeclKind::Extend { name, constraints } = extension.kind {
                let id = self.resolve(&module, "", &name, &extension.span)?;
                let checks = self.lower_checks(&module, "", &constraints, &BTreeMap::new(), 0)?;
                let rule = language.rules.get_mut(&id).ok_or_else(|| {
                    Diagnostic::error(
                        "yl.extension_conflict",
                        "Extension target is not a node",
                        extension.span.clone(),
                    )
                })?;
                rule.constraints.extend(checks);
            }
        }
        if !language.rules.contains_key(&language.entry) {
            return Err(Diagnostic::error(
                "yl.entrypoint",
                "Entrypoint must be a node",
                Span::default(),
            ));
        }
        // Pattern surface sugar is retained only as tooling templates, never runtime rules.
        for (id, symbol) in self.symbols.clone() {
            let (term, example) = match &symbol.declaration.kind {
                DeclKind::Node { example, .. } => (Term::Ref(id.clone()), example.clone()),
                DeclKind::Pattern {
                    parameters,
                    grammar,
                    example,
                    ..
                } if parameters.is_empty() => (
                    self.lower(
                        &symbol.module,
                        &symbol.name,
                        grammar,
                        &Environment::new(),
                        0,
                    )?,
                    example.clone(),
                ),
                DeclKind::Pattern {
                    example: Some(example),
                    ..
                } => {
                    return Err(Diagnostic::error(
                        "yl.example_parameters",
                        "Experimental examples require a parameterless pattern",
                        example.span.clone(),
                    ))
                }
                _ => continue,
            };
            // Keep node-only artifacts unchanged unless metadata is supplied.
            if example.is_some() || matches!(symbol.declaration.kind, DeclKind::Pattern { .. }) {
                language.examples.insert(
                    id,
                    crate::examples::ExampleDefinition {
                        term,
                        explicit: example,
                        span: symbol.declaration.span.clone(),
                    },
                );
            }
        }
        crate::examples::validate_explicit(&language)?;
        language.diagnostics = std::mem::take(&mut self.diagnostics);
        for (id, rule) in &language.rules {
            if let Some(meanings) = language.meanings.get(id) {
                let captures = rule_capture_sets(rule, &language, &mut BTreeSet::new()).0;
                validate_meaning_captures(meanings, &captures).map_err(|mut error| {
                    error.code = "yl.unknown_capture".into();
                    error
                })?;
            }
        }
        language.version = if !language.query_meanings.is_empty() {
            4
        } else if language.meanings.is_empty() {
            1
        } else {
            2
        };
        validate_language(&language)?;
        Ok(language)
    }
    fn leaves(
        &self,
        id: &str,
        visiting: &mut BTreeSet<String>,
        result: &mut Vec<String>,
    ) -> Result<()> {
        let symbol = self.symbols.get(id).ok_or_else(|| {
            Diagnostic::error("yl.unknown_reference", "Unknown member", Span::default())
        })?;
        if !visiting.insert(id.into()) {
            return Err(Diagnostic::error(
                "yl.membership_cycle",
                "Cyclic abstract membership",
                symbol.declaration.span.clone(),
            ));
        }
        let DeclKind::Node {
            grammar, members, ..
        } = &symbol.declaration.kind
        else {
            return Err(Diagnostic::error(
                "yl.invalid_member",
                "Member is not a node",
                symbol.declaration.span.clone(),
            ));
        };
        if grammar.is_some() {
            if !result.contains(&id.to_string()) {
                result.push(id.into());
            }
        } else {
            for member in members {
                if let DeclKind::Node { name, .. } = &member.kind {
                    let child = self.member_path(id, name, &member.span, 0)?;
                    self.leaves(&child, visiting, result)?;
                }
            }
        }
        visiting.remove(id);
        Ok(())
    }
    fn lower(
        &mut self,
        module: &str,
        scope: &str,
        expr: &Expr,
        env: &Environment,
        depth: usize,
    ) -> Result<Term> {
        if depth > 128 {
            return Err(Diagnostic::error(
                "yl.expansion_cycle",
                "Recursive grammar expansion exceeds resource limit",
                expr.span.clone(),
            ));
        }
        if let Some((head, tail)) = self.juxtaposition(module, scope, expr, env)? {
            let sequence = Expr {
                kind: ExprKind::Sequence(vec![head, tail]),
                span: expr.span.clone(),
            };
            return self.lower(module, scope, &sequence, env, depth + 1);
        }
        Ok(match &expr.kind {
            ExprKind::Lambda { .. } | ExprKind::Local(_) | ExprKind::SemanticCapture(_) => {
                return Err(Diagnostic::error(
                    "yl.invalid_grammar",
                    "Callbacks are only valid in semantic queries",
                    expr.span.clone(),
                ))
            }
            ExprKind::Literal(s) => Term::Literal(s.clone()),
            ExprKind::Group(inner) => self.lower(module, scope, inner, env, depth + 1)?,
            ExprKind::Regex(s) => {
                regex::Regex::new(s).map_err(|e| {
                    Diagnostic::error("yl.invalid_regex", e.to_string(), expr.span.clone())
                })?;
                Term::Regex(s.clone())
            }
            ExprKind::Ref(name) => {
                if let Some(value) = env.get(name) {
                    if let Value::Grammar(term) = value {
                        return Ok(term.clone());
                    }
                    return Err(Diagnostic::error(
                        "yl.invalid_argument",
                        "Enum value used as grammar",
                        expr.span.clone(),
                    ));
                }
                let id = self.resolve(module, scope, name, &expr.span)?;
                let symbol = self.symbols.get(&id).cloned().ok_or_else(|| {
                    Diagnostic::error(
                        "yl.invalid_argument",
                        "Core primitive must be called",
                        expr.span.clone(),
                    )
                })?;
                match &symbol.declaration.kind {
                    DeclKind::Node { .. } => Term::Ref(id),
                    DeclKind::Pattern {
                        parameters,
                        grammar,
                        ..
                    } => {
                        let env =
                            self.bind(&symbol, parameters, &[], module, scope, env, depth + 1)?;
                        self.expand(&id, &symbol, grammar, &env, depth + 1)?
                    }
                    _ => {
                        return Err(Diagnostic::error(
                            "yl.invalid_argument",
                            "Reference is not a grammar value",
                            expr.span.clone(),
                        ))
                    }
                }
            }
            ExprKind::Call(name, args) => {
                if env.contains_key(name) {
                    return Err(Diagnostic::error(
                        "yl.invalid_argument",
                        "Grammar/enum bindings are not callable declarations",
                        expr.span.clone(),
                    ));
                }
                let id = self.resolve(module, scope, name, &expr.span)?;
                if id.starts_with("core/parser#") {
                    if args.len() != 1 || args[0].name.is_some() {
                        return Err(Diagnostic::error(
                            "yl.invalid_argument",
                            "Core lookaround requires one positional pattern",
                            expr.span.clone(),
                        ));
                    }
                    let value =
                        Box::new(self.lower(module, scope, &args[0].value, env, depth + 1)?);
                    if id.ends_with("#notAhead") {
                        Term::NotAhead(value)
                    } else {
                        Term::NotBehind(value)
                    }
                } else {
                    let symbol = self.symbols.get(&id).cloned().ok_or_else(|| {
                        Diagnostic::error(
                            "yl.unknown_reference",
                            "Unknown callable",
                            expr.span.clone(),
                        )
                    })?;
                    let DeclKind::Pattern {
                        parameters,
                        grammar,
                        ..
                    } = &symbol.declaration.kind
                    else {
                        return Err(Diagnostic::error(
                            "yl.invalid_argument",
                            "Expected parameterized pattern",
                            expr.span.clone(),
                        ));
                    };
                    let bound =
                        self.bind(&symbol, parameters, args, module, scope, env, depth + 1)?;
                    self.expand(&id, &symbol, grammar, &bound, depth + 1)?
                }
            }
            ExprKind::Sequence(items) => Term::Sequence(
                items
                    .iter()
                    .map(|e| self.lower(module, scope, e, env, depth + 1))
                    .collect::<Result<_>>()?,
            ),
            ExprKind::Choice(items) => Term::Choice(
                items
                    .iter()
                    .map(|e| self.lower(module, scope, e, env, depth + 1))
                    .collect::<Result<_>>()?,
            ),
            ExprKind::Repeat(value, q) => Term::Repeat(
                Box::new(self.lower(module, scope, value, env, depth + 1)?),
                *q,
            ),
            ExprKind::Capture(name, value) => Term::Capture(
                name.clone(),
                Box::new(self.lower(module, scope, value, env, depth + 1)?),
            ),
            ExprKind::Pipe(input, name, args) => {
                let input = self.lower(module, scope, input, env, depth + 1)?;
                let id = self.resolve(module, scope, name, &expr.span)?;
                let symbol = self.symbols.get(&id).cloned().ok_or_else(|| {
                    Diagnostic::error("yl.unknown_reference", "Unknown pipe", expr.span.clone())
                })?;
                let DeclKind::Pipe {
                    parameters, arms, ..
                } = &symbol.declaration.kind
                else {
                    return Err(Diagnostic::error(
                        "yl.invalid_rewrite",
                        "Pipeline target is not a pipe",
                        expr.span.clone(),
                    ));
                };
                let mut bound =
                    self.bind(&symbol, parameters, args, module, scope, env, depth + 1)?;
                let mut matched = None;
                for arm in arms {
                    let value = if let Some(q) = arm.quantifier {
                        if let Term::Repeat(value, actual) = &input {
                            if q != *actual {
                                continue;
                            }
                            *value.clone()
                        } else {
                            continue;
                        }
                    } else {
                        input.clone()
                    };
                    if let Some(ty) = &arm.ty {
                        let ty = self.resolve(&symbol.module, &symbol.scope, ty, &arm.span)?;
                        if !self.accepts(&value, &ty, &mut BTreeSet::new())? {
                            continue;
                        }
                    }
                    matched = Some((arm, value));
                    break;
                }
                let (arm, value) = matched.ok_or_else(|| {
                    Diagnostic::error(
                        "yl.invalid_rewrite",
                        "No rewrite arm matches supplied grammar structure/type",
                        expr.span.clone(),
                    )
                })?;
                let output_quantifier = value_shape(&input);
                let collect_lists = arm.quantifier.is_none()
                    && matches!(output_quantifier, Some(Quantifier::Star | Quantifier::Plus));
                let marker = self.next_marker;
                self.next_marker = self.next_marker.checked_add(1).ok_or_else(|| {
                    Diagnostic::error(
                        "yl.resource_limit",
                        "Too many grammar projections",
                        expr.span.clone(),
                    )
                })?;
                bound.insert(
                    arm.binding.clone(),
                    Value::Grammar(Term::Mark {
                        id: marker,
                        term: Box::new(value),
                    }),
                );
                let body = match &arm.body {
                    RewriteBody::Direct(body) => body,
                    RewriteBody::Cases(original) => {
                        let enums: Vec<_> = parameters
                            .iter()
                            .filter(|p| matches!(bound.get(&p.name), Some(Value::Enum(..))))
                            .collect();
                        if enums.len() != 1 {
                            return Err(Diagnostic::error(
                                "yl.enum_selector",
                                "Multiple enum parameters require an explicit match selector",
                                arm.span.clone(),
                            ));
                        }
                        let selector = Expr {
                            kind: ExprKind::Ref(enums[0].name.clone()),
                            span: arm.span.clone(),
                        };
                        let cases: Vec<_> = original
                            .iter()
                            .map(|(pattern, body)| RewriteCase {
                                patterns: vec![Some(pattern.clone())],
                                replacement: Some(body.clone()),
                                diagnostics: vec![],
                                span: pattern.span.clone(),
                            })
                            .collect();
                        let chosen = self.select_case(
                            &symbol,
                            &[selector],
                            &cases,
                            &bound,
                            parameters,
                            &expr.span,
                            true,
                        )?;
                        // The selected source body remains borrowed from the original arm.
                        &original[chosen].1
                    }
                    RewriteBody::Match { selectors, cases } => {
                        let chosen = self.select_case(
                            &symbol, selectors, cases, &bound, parameters, &expr.span, true,
                        )?;
                        let case = &cases[chosen];
                        let Some(body) = &case.replacement else {
                            return Err(Diagnostic::error(
                                "yl.invalid_rewrite",
                                "A successful case requires a replacement grammar",
                                case.span.clone(),
                            ));
                        };
                        body
                    }
                };
                let term = self.expand(&id, &symbol, body, &bound, depth + 1)?;
                let cardinality = marker_value_cardinality(&term, marker, collect_lists);
                if !valid_cardinality(output_quantifier, cardinality) {
                    return Err(Diagnostic::error(
                        "yl.projection_cardinality",
                        "Rewrite output does not preserve the input cardinality",
                        expr.span.clone(),
                    ));
                }
                Term::Project {
                    id: marker,
                    term: Box::new(term),
                    quantifier: output_quantifier,
                    collect_lists,
                }
            }
            ExprKind::Tuple(_)
            | ExprKind::List(_)
            | ExprKind::EnumConstant(_, _)
            | ExprKind::Bool(_)
            | ExprKind::Absent
            | ExprKind::Not(_)
            | ExprKind::Binary(_, _, _)
            | ExprKind::Variant(_) => {
                return Err(Diagnostic::error(
                    "yl.ambiguous_enum_variant",
                    "Contextual enum needs an expected enum type",
                    expr.span.clone(),
                ))
            }
        })
    }
    #[allow(clippy::too_many_arguments)]
    fn select_case(
        &mut self,
        symbol: &Symbol,
        selectors: &[Expr],
        cases: &[RewriteCase],
        bound: &Environment,
        parameters: &[Parameter],
        call: &Span,
        emit: bool,
    ) -> Result<usize> {
        let inferred;
        let selectors = if selectors.is_empty() {
            let enums: Vec<_> = parameters
                .iter()
                .filter(|p| matches!(bound.get(&p.name), Some(Value::Enum(..))))
                .collect();
            if enums.len() != 1 {
                return Err(Diagnostic::error(
                    "yl.enum_selector",
                    "Multiple enum parameters require an explicit match selector",
                    symbol.declaration.span.clone(),
                ));
            }
            inferred = vec![Expr {
                kind: ExprKind::Ref(enums[0].name.clone()),
                span: symbol.declaration.span.clone(),
            }];
            &inferred
        } else {
            selectors
        };
        let mut selected = vec![];
        let mut argument_spans = vec![];
        for selector in selectors {
            let ExprKind::Ref(name) = &selector.kind else {
                return Err(Diagnostic::error(
                    "yl.enum_selector",
                    "Expected enum parameter selector",
                    selector.span.clone(),
                ));
            };
            let Some(Value::Enum(ty, variant, origin)) = bound.get(name) else {
                return Err(Diagnostic::error(
                    "yl.enum_selector",
                    "Selector must name an enum parameter",
                    selector.span.clone(),
                ));
            };
            selected.push((ty.clone(), variant.clone()));
            let span = origin.clone().unwrap_or_else(|| call.clone());
            argument_spans.push(span);
        }
        let mut normalized = vec![];
        for case in cases {
            let wildcard;
            let patterns = if case.patterns.len() == 1 && case.patterns[0].is_none() {
                wildcard = vec![None; selected.len()];
                &wildcard
            } else {
                &case.patterns
            };
            if patterns.len() != selected.len() {
                return Err(Diagnostic::error(
                    "yl.enum_case",
                    "Case tuple does not match selector arity",
                    case.span.clone(),
                ));
            }
            let pattern = patterns
                .iter()
                .zip(&selected)
                .map(|(p, (ty, _))| {
                    p.as_ref()
                        .map(|p| {
                            self.enum_value(&symbol.module, &symbol.scope, p, Some(ty), bound)
                                .map(|(_, v)| v)
                        })
                        .transpose()
                })
                .collect::<Result<Vec<_>>>()?;
            if normalized.iter().any(|earlier: &Vec<Option<String>>| {
                earlier
                    .iter()
                    .zip(&pattern)
                    .all(|(a, b)| a.is_none() || a == b)
            }) {
                return Err(Diagnostic::error(
                    "yl.unreachable_case",
                    "Rewrite case is completely shadowed by an earlier case",
                    case.span.clone(),
                ));
            }
            let has_error = case
                .diagnostics
                .iter()
                .any(|d| matches!(&d.kind,ConstraintKind::Call(name,_) if name=="error"));
            if !has_error && case.replacement.is_none() {
                return Err(Diagnostic::error(
                    "yl.invalid_rewrite",
                    "Case requires a replacement or error",
                    case.span.clone(),
                ));
            }
            for emission in &case.diagnostics {
                validate_case_emission(emission)?;
            }
            normalized.push(pattern);
        }
        let mut combinations = vec![vec![]];
        for (ty, _) in &selected {
            let DeclKind::Enum { variants, .. } = &self.symbols[ty].declaration.kind else {
                return Err(Diagnostic::error(
                    "yl.enum_selector",
                    "Expected enum",
                    call.clone(),
                ));
            };
            if combinations.len().saturating_mul(variants.len()) > 65536 {
                return Err(Diagnostic::error(
                    "yl.resource_limit",
                    "Enum match exhaustiveness exceeds resource limit",
                    call.clone(),
                ));
            }
            combinations = combinations
                .into_iter()
                .flat_map(|prefix| {
                    variants.iter().map(move |v| {
                        let mut p = prefix.clone();
                        p.push(v.clone());
                        p
                    })
                })
                .collect();
        }
        let matches = |pattern: &[Option<String>], values: &[String]| {
            pattern
                .iter()
                .zip(values)
                .all(|(p, v)| p.as_ref().is_none_or(|p| p == v))
        };
        if combinations
            .iter()
            .any(|values| !normalized.iter().any(|p| matches(p, values)))
        {
            return Err(Diagnostic::error(
                "yl.nonexhaustive_match",
                "Rewrite cases must cover every enum combination",
                symbol.declaration.span.clone(),
            ));
        }
        for (index, pattern) in normalized.iter().enumerate() {
            if !combinations.iter().any(|values| {
                matches(pattern, values)
                    && !normalized[..index]
                        .iter()
                        .any(|earlier| matches(earlier, values))
            }) {
                return Err(Diagnostic::error(
                    "yl.unreachable_case",
                    "Rewrite case is completely shadowed by earlier cases",
                    cases[index].span.clone(),
                ));
            }
        }
        let values: Vec<_> = selected.iter().map(|(_, v)| v.clone()).collect();
        let index = normalized
            .iter()
            .position(|p| matches(p, &values))
            .ok_or_else(|| {
                Diagnostic::error("yl.enum_case", "No case matches arguments", call.clone())
            })?;
        if !emit {
            return Ok(index);
        }
        for emission in &cases[index].diagnostics {
            let ConstraintKind::Call(name, args) = &emission.kind else {
                return Err(Diagnostic::error(
                    "yl.invalid_constraint",
                    "Expected diagnostic emission",
                    emission.span.clone(),
                ));
            };
            let [argument] = args.as_slice() else {
                return Err(Diagnostic::error(
                    "yl.invalid_argument",
                    "Diagnostic takes one string",
                    emission.span.clone(),
                ));
            };
            let ExprKind::Literal(message) = &argument.value.kind else {
                return Err(Diagnostic::error(
                    "yl.invalid_argument",
                    "Diagnostic takes one string",
                    argument.span.clone(),
                ));
            };
            let severity = match name.as_str() {
                "error" => Severity::Error,
                "warning" => Severity::Warning,
                "help" => Severity::Help,
                _ => {
                    return Err(Diagnostic::error(
                        "yl.invalid_constraint",
                        "Expected diagnostic emission",
                        emission.span.clone(),
                    ))
                }
            };
            let mut diagnostic = Diagnostic::error(
                "yl.pipe_case",
                message,
                argument_spans.first().unwrap_or(call).clone(),
            );
            diagnostic.severity = severity;
            diagnostic
                .secondary
                .extend(argument_spans.iter().skip(1).cloned());
            diagnostic.secondary.push(emission.span.clone());
            if argument_spans.iter().any(|s| s == call) {
                diagnostic.help =
                    Some("One or more selected enum arguments use their defaults".into());
            }
            if diagnostic.severity == Severity::Error {
                return Err(diagnostic);
            }
            self.diagnostics.push(diagnostic);
        }
        Ok(index)
    }
    /// Resolve name-plus-parentheses after forward references/imports are known.
    /// Bindings and nodes denote grammar; parameterized definitions denote calls.
    /// Explicit Group nodes prevent postfix/capture reassociation across parentheses.
    fn juxtaposition(
        &self,
        module: &str,
        scope: &str,
        expr: &Expr,
        env: &Environment,
    ) -> Result<Option<(Expr, Expr)>> {
        let make = |kind| Expr {
            kind,
            span: expr.span.clone(),
        };
        Ok(match &expr.kind {
            ExprKind::Call(name, args) if args.len() == 1 && args[0].name.is_none() => {
                let grammar = if let Some(value) = env.get(name) {
                    matches!(value, Value::Grammar(_))
                } else {
                    let id = self.resolve(module, scope, name, &expr.span)?;
                    self.symbols
                        .get(&id)
                        .is_some_and(|s| matches!(s.declaration.kind, DeclKind::Node { .. }))
                };
                if grammar {
                    Some((
                        make(ExprKind::Ref(name.clone())),
                        make(ExprKind::Group(Box::new(args[0].value.clone()))),
                    ))
                } else {
                    None
                }
            }
            ExprKind::Repeat(inner, q) => self
                .juxtaposition(module, scope, inner, env)?
                .map(|(head, tail)| (head, make(ExprKind::Repeat(Box::new(tail), *q)))),
            ExprKind::Pipe(inner, name, args) => self
                .juxtaposition(module, scope, inner, env)?
                .map(|(head, tail)| {
                    (
                        head,
                        make(ExprKind::Pipe(Box::new(tail), name.clone(), args.clone())),
                    )
                }),
            ExprKind::Capture(name, inner) => self
                .juxtaposition(module, scope, inner, env)?
                .map(|(head, tail)| (make(ExprKind::Capture(name.clone(), Box::new(head))), tail)),
            _ => None,
        })
    }
    fn expand(
        &mut self,
        id: &str,
        symbol: &Symbol,
        expr: &Expr,
        env: &Environment,
        depth: usize,
    ) -> Result<Term> {
        if self.expanding.iter().any(|v| v == id) {
            return Err(Diagnostic::error(
                "yl.expansion_cycle",
                "Recursive patterns/pipes cannot be finitely lowered",
                expr.span.clone(),
            ));
        }
        self.expanding.push(id.into());
        let result = self.lower(&symbol.module, &symbol.name, expr, env, depth);
        self.expanding.pop();
        result.and_then(|term| {
            let term = if matches!(
                &symbol.declaration.kind,
                DeclKind::Pattern {
                    example: Some(_),
                    ..
                }
            ) {
                Term::Example {
                    definition: id.into(),
                    term: Box::new(term),
                }
            } else {
                term
            };
            if let DeclKind::Pattern {
                meanings: Some(meanings),
                ..
            } = &symbol.declaration.kind
            {
                validate_meaning_captures(
                    &self.lower_meanings(symbol, meanings)?,
                    &capture_sets(&term).0,
                )
                .map_err(|mut error| {
                    error.code = "yl.unknown_capture".into();
                    error
                })?;
                Ok(Term::Meaning {
                    definition: id.into(),
                    term: Box::new(term),
                })
            } else if self.selected_patterns.contains(id) {
                Ok(Term::Meaning {
                    definition: id.into(),
                    term: Box::new(term),
                })
            } else {
                Ok(term)
            }
        })
    }
    fn validate_semantic_wrappers(&self) -> Result<()> {
        for wrapper in self.symbols.values() {
            let DeclKind::Operation {
                parameters, body, ..
            } = &wrapper.declaration.kind
            else {
                continue;
            };
            let mut bindings = BTreeMap::new();
            for parameter in parameters {
                if let Some(default) = &parameter.default {
                    self.expand_semantic_expr(wrapper, default, &bindings, &mut vec![])?;
                }
                bindings.insert(
                    parameter.name.clone(),
                    Expr {
                        kind: ExprKind::Absent,
                        span: parameter.span.clone(),
                    },
                );
            }
            let mut stack = vec![format!("{}#{}", wrapper.module, wrapper.name)];
            match body {
                OperationBody::Conditional(actions) => {
                    self.lower_query_actions(wrapper, actions, &bindings, &mut stack)?;
                }
                OperationBody::Actions(calls) => {
                    for call in calls {
                        let probe = MeaningSyntaxGroup {
                            name: None,
                            calls: vec![call.clone()],
                            matching: None,
                            actions: vec![],
                            span: call.span.clone(),
                        };
                        if self.is_query_group(wrapper, &probe)? {
                            self.lower_query_actions(
                                wrapper,
                                &[MeaningSyntaxAction::Call(call.clone())],
                                &bindings,
                                &mut stack,
                            )?;
                        } else {
                            self.expand_semantic_actions(wrapper, call, &bindings, &mut stack)?;
                        }
                    }
                }
                OperationBody::Query(expr) => {
                    let value = self.expand_semantic_expr(wrapper, expr, &bindings, &mut stack)?;
                    if !matches!(value.kind, ExprKind::Call(_, _)) {
                        self.query_expr_normalized(wrapper, &value)?;
                    }
                }
            }
        }
        Ok(())
    }
    fn expand_semantic_expr(
        &self,
        context: &Symbol,
        expr: &Expr,
        bindings: &BTreeMap<String, Expr>,
        stack: &mut Vec<String>,
    ) -> Result<Expr> {
        let mut result = expr.clone();
        result.kind = match &expr.kind {
            ExprKind::Lambda { parameter, body } => {
                let local = format!("{parameter}@{}:{}", expr.span.file, expr.span.start);
                let mut env = bindings.clone();
                env.insert(
                    parameter.clone(),
                    Expr {
                        kind: ExprKind::Local(local.clone()),
                        span: expr.span.clone(),
                    },
                );
                ExprKind::Lambda {
                    parameter: local,
                    body: Box::new(self.expand_semantic_expr(context, body, &env, stack)?),
                }
            }
            ExprKind::Not(inner) => ExprKind::Not(Box::new(
                self.expand_semantic_expr(context, inner, bindings, stack)?,
            )),
            ExprKind::Ref(name) => {
                if let Some(value) = bindings.get(name) {
                    return Ok(value.clone());
                }
                if let Some((root, suffix)) = name.split_once('.') {
                    if let Some(value) = bindings.get(root) {
                        let projected = match &value.kind {
                            ExprKind::Ref(actual) => {
                                Some(ExprKind::Ref(format!("{actual}.{suffix}")))
                            }
                            ExprKind::SemanticCapture(actual) => {
                                Some(ExprKind::SemanticCapture(format!("{actual}.{suffix}")))
                            }
                            _ => None,
                        };
                        if let Some(kind) = projected {
                            return Ok(Expr {
                                kind,
                                span: value.span.clone(),
                            });
                        }
                        if matches!(value.kind, ExprKind::Absent) {
                            return Ok(value.clone());
                        }
                    }
                }
                if name == "self" {
                    ExprKind::Ref(name.clone())
                } else if symbol_has_capture(context, name.split('.').next().unwrap_or(name)) {
                    ExprKind::SemanticCapture(name.clone())
                } else if name == "absent" {
                    ExprKind::Absent
                } else if matches!(name.as_str(), "true" | "false") {
                    ExprKind::Bool(name == "true")
                } else {
                    match self.resolve(&context.module, &context.name, name, &expr.span) {
                        Ok(id) => ExprKind::Ref(id),
                        Err(error)
                            if matches!(context.declaration.kind, DeclKind::Operation { .. }) =>
                        {
                            return Err(error)
                        }
                        Err(_) => ExprKind::Ref(name.clone()),
                    }
                }
            }

            ExprKind::Call(name, arguments) => {
                let id = self.resolve(&context.module, &context.name, name, &expr.span)?;
                let arguments = arguments
                    .iter()
                    .map(|argument| {
                        Ok(Argument {
                            value: self.expand_semantic_expr(
                                context,
                                &argument.value,
                                bindings,
                                stack,
                            )?,
                            ..argument.clone()
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                if let Some(wrapper) = self.symbols.get(&id) {
                    if let DeclKind::Operation {
                        parameters, body, ..
                    } = &wrapper.declaration.kind
                    {
                        let OperationBody::Query(body) = body else {
                            return Err(Diagnostic::error(
                                "yl.invalid_operation_query",
                                "Action operation cannot be used as a value",
                                expr.span.clone(),
                            ));
                        };
                        let env = self.bind_semantic_wrapper(
                            wrapper, parameters, &arguments, &expr.span, stack,
                        )?;
                        let expanded = self.expand_semantic_expr(wrapper, body, &env, stack);
                        stack.pop();
                        return expanded.map(|mut value| {
                            value.span = expr.span.clone();
                            value
                        });
                    }
                }
                ExprKind::Call(id, arguments)
            }
            ExprKind::Tuple(values) => ExprKind::Tuple(
                values
                    .iter()
                    .map(|value| self.expand_semantic_expr(context, value, bindings, stack))
                    .collect::<Result<Vec<_>>>()?,
            ),
            ExprKind::List(values) => ExprKind::List(
                values
                    .iter()
                    .map(|value| self.expand_semantic_expr(context, value, bindings, stack))
                    .collect::<Result<Vec<_>>>()?,
            ),
            ExprKind::Group(value) => {
                return self.expand_semantic_expr(context, value, bindings, stack)
            }
            _ => expr.kind.clone(),
        };
        Ok(result)
    }
    fn bind_semantic_wrapper(
        &self,
        wrapper: &Symbol,
        parameters: &[Parameter],
        arguments: &[Argument],
        span: &Span,
        stack: &mut Vec<String>,
    ) -> Result<BTreeMap<String, Expr>> {
        let id = format!("{}#{}", wrapper.module, wrapper.name);
        if stack.contains(&id) || stack.len() >= 128 {
            return Err(Diagnostic::error(
                "yl.operation_cycle",
                "Recursive semantic operation expansion",
                span.clone(),
            ));
        }
        stack.push(id);
        let mut env = BTreeMap::new();
        let mut position = 0;
        let mut named = false;
        for argument in arguments {
            let parameter = if let Some(name) = &argument.name {
                named = true;
                parameters.iter().find(|parameter| &parameter.name == name)
            } else {
                if named {
                    return Err(Diagnostic::error(
                        "yl.invalid_argument",
                        "Positional argument follows named argument",
                        argument.span.clone(),
                    ));
                }
                let parameter = parameters.get(position);
                position += 1;
                parameter
            }
            .ok_or_else(|| {
                Diagnostic::error(
                    "yl.invalid_argument",
                    "Unknown or excess wrapper argument",
                    argument.span.clone(),
                )
            })?;
            if env
                .insert(parameter.name.clone(), argument.value.clone())
                .is_some()
            {
                return Err(Diagnostic::error(
                    "yl.invalid_argument",
                    "Duplicate wrapper argument",
                    argument.span.clone(),
                ));
            }
        }
        for parameter in parameters {
            if !env.contains_key(&parameter.name) {
                let default = parameter.default.as_ref().ok_or_else(|| {
                    Diagnostic::error(
                        "yl.invalid_argument",
                        format!("Missing operation argument {}", parameter.name),
                        span.clone(),
                    )
                })?;
                let value = self.expand_semantic_expr(wrapper, default, &env, stack)?;
                env.insert(parameter.name.clone(), value);
            }
        }
        Ok(env)
    }
    fn expand_semantic_actions(
        &self,
        context: &Symbol,
        call: &MeaningSyntaxCall,
        bindings: &BTreeMap<String, Expr>,
        stack: &mut Vec<String>,
    ) -> Result<Vec<MeaningSyntaxCall>> {
        let id = self.resolve(&context.module, &context.name, &call.name, &call.span)?;
        let arguments = call
            .arguments
            .iter()
            .map(|argument| {
                Ok(Argument {
                    value: self.expand_semantic_expr(context, &argument.value, bindings, stack)?,
                    ..argument.clone()
                })
            })
            .collect::<Result<Vec<_>>>()?;
        if let Some(wrapper) = self.symbols.get(&id) {
            if let DeclKind::Operation {
                parameters, body, ..
            } = &wrapper.declaration.kind
            {
                let OperationBody::Actions(calls) = body else {
                    return Err(Diagnostic::error(
                        "yl.invalid_operation_action",
                        "Query operation cannot be used as an action",
                        call.span.clone(),
                    ));
                };
                let env =
                    self.bind_semantic_wrapper(wrapper, parameters, &arguments, &call.span, stack)?;
                let mut expanded = vec![];
                for nested in calls {
                    expanded.extend(self.expand_semantic_actions(wrapper, nested, &env, stack)?);
                }
                stack.pop();
                for nested in &mut expanded {
                    nested.span = call.span.clone();
                }
                return Ok(expanded);
            }
        }
        Ok(vec![MeaningSyntaxCall {
            name: id,
            arguments,
            span: call.span.clone(),
        }])
    }
    fn lower_meaning_actions(
        &self,
        symbol: &Symbol,
        call: &MeaningSyntaxCall,
    ) -> Result<Vec<MeaningCall>> {
        self.expand_semantic_actions(symbol, call, &BTreeMap::new(), &mut vec![])?
            .iter()
            .map(|call| self.lower_meaning_call(symbol, call))
            .collect()
    }
    fn lower_meaning_call(&self, symbol: &Symbol, call: &MeaningSyntaxCall) -> Result<MeaningCall> {
        let id = self.resolve(&symbol.module, &symbol.name, &call.name, &call.span)?;
        let contract = self
            .registry
            .signature(&id)
            .ok_or_else(|| {
                Diagnostic::error(
                    "yl.unknown_operation",
                    format!("{} is not a registered semantic operation", call.name),
                    call.span.clone(),
                )
            })?
            .clone();
        let mut arguments = BTreeMap::new();
        let mut position = 0;
        let mut named = false;
        for argument in &call.arguments {
            let parameter = if let Some(name) = &argument.name {
                named = true;
                contract
                    .parameters
                    .iter()
                    .find(|parameter| &parameter.name == name)
            } else {
                if named {
                    return Err(Diagnostic::error(
                        "yl.invalid_argument",
                        "Positional operation argument follows a named argument",
                        argument.span.clone(),
                    ));
                }
                let parameter = contract.parameters.get(position);
                position += 1;
                parameter
            }
            .ok_or_else(|| {
                Diagnostic::error(
                    "yl.invalid_argument",
                    "Unknown or excess operation argument",
                    argument.span.clone(),
                )
            })?;
            let value = match (&parameter.kind, &argument.value.kind) {
                (_, ExprKind::Call(name, args)) => {
                    MeaningValue::Call(Box::new(self.lower_meaning_query(
                        symbol,
                        &MeaningSyntaxCall {
                            name: name.clone(),
                            arguments: args.clone(),
                            span: argument.value.span.clone(),
                        },
                    )?))
                }
                (ParameterKind::Value, _) => {
                    self.lower_meaning_value(symbol, &argument.value, false)?
                }
                (ParameterKind::Capture, ExprKind::SemanticCapture(name)) => {
                    MeaningValue::Capture(name.clone())
                }
                (ParameterKind::Capture, ExprKind::Ref(name)) => {
                    if name == "self" {
                        MeaningValue::SelfValue
                    } else {
                        MeaningValue::Capture(name.clone())
                    }
                }
                (ParameterKind::Text, ExprKind::Literal(text)) => MeaningValue::Text(text.clone()),
                (ParameterKind::Bool, ExprKind::Bool(value)) => MeaningValue::Bool(*value),
                (ParameterKind::Bool, ExprKind::Ref(value))
                    if matches!(value.as_str(), "true" | "false") =>
                {
                    MeaningValue::Bool(value == "true")
                }
                (ParameterKind::Symbol, ExprKind::Ref(name)) => {
                    let id =
                        self.resolve(&symbol.module, &symbol.name, name, &argument.value.span)?;
                    if !self.registry.symbols.values().any(|symbol| symbol == &id)
                        && !self
                            .symbols
                            .get(&id)
                            .is_some_and(|s| matches!(s.declaration.kind, DeclKind::Symbol { .. }))
                    {
                        return Err(Diagnostic::error(
                            "yl.invalid_argument",
                            "Expected a semantic identity symbol",
                            argument.span.clone(),
                        ));
                    }
                    MeaningValue::Symbol(id)
                }
                _ => {
                    return Err(Diagnostic::error(
                        "yl.invalid_argument",
                        format!(
                            "Incompatible value for operation parameter {}",
                            parameter.name
                        ),
                        argument.span.clone(),
                    ))
                }
            };
            if arguments.insert(parameter.name.clone(), value).is_some() {
                return Err(Diagnostic::error(
                    "yl.invalid_argument",
                    "Duplicate operation argument",
                    argument.span.clone(),
                ));
            }
        }
        for parameter in &contract.parameters {
            if !arguments.contains_key(&parameter.name) {
                let value = parameter.default.clone().ok_or_else(|| {
                    Diagnostic::error(
                        "yl.invalid_argument",
                        format!("Missing operation argument {}", parameter.name),
                        call.span.clone(),
                    )
                })?;
                arguments.insert(parameter.name.clone(), value);
            }
        }
        if contract.provides_context && arguments.values().any(semantic_has_query) {
            return Err(Diagnostic::error(
                "yl.deferred_context",
                "Context providers cannot depend on deferred value expressions",
                call.span.clone(),
            ));
        }
        Ok(MeaningCall {
            operation: id,
            contract,
            arguments,
            span: call.span.clone(),
        })
    }
    fn lower_meaning_query(
        &self,
        symbol: &Symbol,
        syntax: &MeaningSyntaxCall,
    ) -> Result<MeaningCall> {
        let expr = Expr {
            kind: ExprKind::Call(syntax.name.clone(), syntax.arguments.clone()),
            span: syntax.span.clone(),
        };
        let expanded = self.expand_semantic_expr(symbol, &expr, &BTreeMap::new(), &mut vec![])?;
        let ExprKind::Call(name, arguments) = expanded.kind else {
            return Err(Diagnostic::error(
                "yl.invalid_operation_query",
                "Query wrapper must expand to a native query",
                syntax.span.clone(),
            ));
        };
        let call = self.lower_meaning_call(
            symbol,
            &MeaningSyntaxCall {
                name,
                arguments,
                span: syntax.span.clone(),
            },
        )?;
        if call.contract.provides_context {
            return Err(Diagnostic::error(
                "yl.deferred_context",
                "Value expressions cannot provide lifecycle contexts",
                call.span,
            ));
        }
        Ok(call)
    }
    fn lower_meaning_value(
        &self,
        symbol: &Symbol,
        expr: &Expr,
        pattern: bool,
    ) -> Result<MeaningValue> {
        match &expr.kind {
            ExprKind::SemanticCapture(name) if !pattern => Ok(MeaningValue::Capture(name.clone())),
            ExprKind::Ref(name) if name == "self" && !pattern => Ok(MeaningValue::SelfValue),
            ExprKind::Absent if !pattern => Ok(MeaningValue::Absent),
            ExprKind::Group(value) if !pattern => self.lower_meaning_value(symbol, value, false),
            ExprKind::Tuple(values) if !pattern => Ok(MeaningValue::Tuple(
                values
                    .iter()
                    .map(|value| self.lower_meaning_value(symbol, value, false))
                    .collect::<Result<Vec<_>>>()?,
            )),
            ExprKind::List(values) if !pattern => Ok(MeaningValue::List(
                values
                    .iter()
                    .map(|value| self.lower_meaning_value(symbol, value, false))
                    .collect::<Result<Vec<_>>>()?,
            )),
            ExprKind::Call(name, arguments) if !pattern => {
                Ok(MeaningValue::Call(Box::new(self.lower_meaning_query(
                    symbol,
                    &MeaningSyntaxCall {
                        name: name.clone(),
                        arguments: arguments.clone(),
                        span: expr.span.clone(),
                    },
                )?)))
            }
            ExprKind::Literal(value) => Ok(MeaningValue::Text(value.clone())),
            ExprKind::Bool(value) => Ok(MeaningValue::Bool(*value)),
            ExprKind::Ref(name)
                if matches!(name.as_str(), "true" | "false")
                    && !symbol_has_capture(symbol, name) =>
            {
                Ok(MeaningValue::Bool(name == "true"))
            }
            ExprKind::Ref(name) => {
                let id = match self.resolve(&symbol.module, &symbol.name, name, &expr.span) {
                    Ok(id) => id,
                    Err(_) if !pattern => return Ok(MeaningValue::Capture(name.clone())),
                    Err(error) => return Err(error),
                };
                if !self.registry.symbols.values().any(|symbol| symbol == &id)
                    && !self
                        .symbols
                        .get(&id)
                        .is_some_and(|s| matches!(s.declaration.kind, DeclKind::Symbol { .. }))
                {
                    return Err(Diagnostic::error(
                        "yl.invalid_meaning_match",
                        "Match constants must be semantic identity symbols",
                        expr.span.clone(),
                    ));
                }
                Ok(MeaningValue::Symbol(id))
            }
            _ => Err(Diagnostic::error(
                "yl.invalid_meaning_match",
                "Expected a native value call or constant",
                expr.span.clone(),
            )),
        }
    }
    fn lower_meaning_match(
        &self,
        symbol: &Symbol,
        matching: &MeaningSyntaxMatch,
    ) -> Result<MeaningMatch> {
        let inputs = matching
            .inputs
            .iter()
            .map(|input| self.lower_meaning_value(symbol, input, false))
            .collect::<Result<Vec<_>>>()?;
        let mut cases = vec![];
        for (index, case) in matching.cases.iter().enumerate() {
            let mut alternatives = vec![];
            for alternative in &case.alternatives {
                if alternative.len() != inputs.len() {
                    return Err(Diagnostic::error(
                        "yl.invalid_meaning_match",
                        "Match tuple arity differs from its inputs",
                        case.span.clone(),
                    ));
                }
                if alternative.iter().all(Option::is_none) && index + 1 != matching.cases.len() {
                    return Err(Diagnostic::error(
                        "yl.invalid_meaning_match",
                        "Wildcard case must be last",
                        case.span.clone(),
                    ));
                }
                alternatives.push(
                    alternative
                        .iter()
                        .map(|value| {
                            value
                                .as_ref()
                                .map(|value| self.lower_meaning_value(symbol, value, true))
                                .transpose()
                        })
                        .collect::<Result<Vec<_>>>()?,
                );
            }
            let mut actions = vec![];
            for call in &case.actions {
                if matches!(call.name.as_str(), "error" | "warn" | "warning") {
                    let [argument] = call.arguments.as_slice() else {
                        return Err(Diagnostic::error(
                            "yl.invalid_meaning_match",
                            "Diagnostic needs one message",
                            call.span.clone(),
                        ));
                    };
                    let ExprKind::Literal(text) = &argument.value.kind else {
                        return Err(Diagnostic::error(
                            "yl.invalid_meaning_match",
                            "Diagnostic needs a string message",
                            argument.span.clone(),
                        ));
                    };
                    let mut message = vec![];
                    let mut remaining = text.as_str();
                    while let Some((before, after)) = remaining.split_once("${") {
                        message.push(MeaningMessagePart::Text(before.into()));
                        let Some((capture, rest)) = after.split_once('}') else {
                            return Err(Diagnostic::error(
                                "yl.invalid_meaning_match",
                                "Unterminated diagnostic interpolation",
                                argument.span.clone(),
                            ));
                        };
                        message.push(MeaningMessagePart::Capture(capture.into()));
                        remaining = rest;
                    }
                    message.push(MeaningMessagePart::Text(remaining.into()));
                    actions.push(MeaningAction::Emit {
                        severity: if call.name == "error" {
                            Severity::Error
                        } else {
                            Severity::Warning
                        },
                        message,
                        span: call.span.clone(),
                    });
                } else {
                    for call in self.lower_meaning_actions(symbol, call)? {
                        if call.contract.provides_context {
                            return Err(Diagnostic::error(
                                "yl.deferred_context",
                                "Deferred actions cannot provide lifecycle contexts",
                                call.span,
                            ));
                        }
                        actions.push(MeaningAction::Call(call));
                    }
                }
            }
            cases.push(MeaningCase {
                alternatives,
                actions,
                span: case.span.clone(),
            });
        }
        if !cases.last().is_some_and(|case| {
            case.alternatives
                .iter()
                .any(|alternative| alternative.iter().all(Option::is_none))
        }) {
            return Err(Diagnostic::error(
                "yl.non_exhaustive_meaning_match",
                "Match requires a final wildcard case",
                matching.span.clone(),
            ));
        }
        Ok(MeaningMatch {
            inputs,
            cases,
            span: matching.span.clone(),
        })
    }
    fn is_query_group(&self, symbol: &Symbol, group: &MeaningSyntaxGroup) -> Result<bool> {
        if !group.actions.is_empty() {
            return Ok(true);
        }
        fn query(expr: &Expr) -> bool {
            match &expr.kind {
                ExprKind::Lambda { .. } | ExprKind::Local(_) => true,
                ExprKind::Call(name, args) => {
                    name.starts_with("std/query#") || args.iter().any(|arg| query(&arg.value))
                }
                ExprKind::Tuple(values) | ExprKind::List(values) => values.iter().any(query),
                ExprKind::Group(value) | ExprKind::Not(value) => query(value),
                _ => false,
            }
        }
        fn action(
            compiler: &Compiler,
            symbol: &Symbol,
            call: &MeaningSyntaxCall,
            visiting: &mut BTreeSet<String>,
        ) -> Result<bool> {
            if matches!(call.name.as_str(), "error" | "warn" | "warning") {
                return Ok(false);
            }
            let id = compiler.resolve(&symbol.module, &symbol.name, &call.name, &call.span)?;
            if id.starts_with("std/query#") {
                return Ok(true);
            }
            if let Some(wrapper) = compiler.symbols.get(&id) {
                if visiting.len() >= 128 || !visiting.insert(id.clone()) {
                    return Err(Diagnostic::error(
                        "yl.operation_cycle",
                        "Recursive semantic operation expansion",
                        call.span.clone(),
                    ));
                }
                if let DeclKind::Operation {
                    body: OperationBody::Conditional(_),
                    ..
                } = &wrapper.declaration.kind
                {
                    return Ok(true);
                }
                if let DeclKind::Operation {
                    body: OperationBody::Actions(calls),
                    ..
                } = &wrapper.declaration.kind
                {
                    for nested in calls {
                        if action(compiler, wrapper, nested, visiting)? {
                            return Ok(true);
                        }
                    }
                }
                visiting.remove(&id);
            }
            let bindings = match &symbol.declaration.kind {
                DeclKind::Operation { parameters, .. } => parameters
                    .iter()
                    .map(|parameter| {
                        (
                            parameter.name.clone(),
                            Expr {
                                kind: ExprKind::Absent,
                                span: parameter.span.clone(),
                            },
                        )
                    })
                    .collect(),
                _ => BTreeMap::new(),
            };
            let expanded =
                compiler.expand_semantic_actions(symbol, call, &bindings, &mut vec![])?;
            Ok(expanded.iter().any(|call| {
                call.name.starts_with("std/query#")
                    || call.arguments.iter().any(|arg| query(&arg.value))
            }))
        }
        for call in &group.calls {
            if matches!(call.name.as_str(), "error" | "warn" | "warning") {
                return Ok(true);
            }
            if action(self, symbol, call, &mut BTreeSet::new())? {
                return Ok(true);
            }
        }
        if let Some(matching) = &group.matching {
            if matching
                .cases
                .iter()
                .any(|case| !case.nested_actions.is_empty())
            {
                return Ok(true);
            }
            for input in &matching.inputs {
                let expanded =
                    self.expand_semantic_expr(symbol, input, &BTreeMap::new(), &mut vec![])?;
                if query(&expanded) {
                    return Ok(true);
                }
            }
            for case in &matching.cases {
                for call in &case.actions {
                    if action(self, symbol, call, &mut BTreeSet::new())? {
                        return Ok(true);
                    }
                }
            }
        }
        Ok(false)
    }
    fn lower_query_meanings(
        &self,
        symbol: &Symbol,
        meanings: &Meanings,
    ) -> Result<Vec<crate::semantics::queries::QueryGroup>> {
        let mut groups = vec![];
        let mut names = BTreeSet::new();
        for group in &meanings.groups {
            if group
                .name
                .as_ref()
                .is_some_and(|name| !names.insert(name.clone()))
            {
                return Err(Diagnostic::error(
                    "yl.duplicate_meaning_group",
                    "Duplicate meaning group",
                    group.span.clone(),
                ));
            }
            if !self.is_query_group(symbol, group)? {
                continue;
            }
            let mut syntax = group
                .calls
                .iter()
                .cloned()
                .map(MeaningSyntaxAction::Call)
                .collect::<Vec<_>>();
            syntax.extend(group.actions.clone());
            if let Some(matching) = &group.matching {
                syntax.push(MeaningSyntaxAction::Match(matching.clone()));
            }
            let actions =
                self.lower_query_actions(symbol, &syntax, &BTreeMap::new(), &mut vec![])?;
            groups.push(crate::semantics::queries::QueryGroup {
                name: group.name.clone(),
                actions,
                span: group.span.clone(),
            });
        }
        Ok(groups)
    }
    fn lower_query_expr(
        &self,
        symbol: &Symbol,
        expr: &Expr,
        bindings: &BTreeMap<String, Expr>,
        stack: &mut Vec<String>,
    ) -> Result<crate::semantics::queries::QueryExpr> {
        let expanded = self.expand_semantic_expr(symbol, expr, bindings, stack)?;
        self.query_expr_normalized(symbol, &expanded)
    }
    fn query_expr_normalized(
        &self,
        symbol: &Symbol,
        expr: &Expr,
    ) -> Result<crate::semantics::queries::QueryExpr> {
        use crate::semantics::queries::QueryExpr as Q;
        Ok(match &expr.kind {
            ExprKind::Local(name) => Q::Local(name.clone()),
            ExprKind::SemanticCapture(name) => Q::Capture(name.clone()),
            ExprKind::Lambda { parameter, body } => Q::Lambda {
                parameter: parameter.clone(),
                body: Box::new(self.query_expr_normalized(symbol, body)?),
            },
            ExprKind::Ref(name) if name == "self" => Q::SelfValue,
            ExprKind::Ref(name) if name == "true" || name == "false" => Q::Bool(name == "true"),
            ExprKind::Ref(name) if name == "absent" => Q::Absent,
            ExprKind::Ref(name) => {
                match self.resolve(&symbol.module, &symbol.name, name, &expr.span) {
                    Ok(id)
                        if self.symbols.get(&id).is_some_and(|s| {
                            matches!(s.declaration.kind, DeclKind::Symbol { .. })
                        }) || self.registry.symbols.values().any(|value| value == &id) =>
                    {
                        Q::Symbol(id)
                    }
                    Ok(_) => {
                        return Err(Diagnostic::error(
                            "yl.invalid_query_value",
                            "Expected a capture or identity symbol",
                            expr.span.clone(),
                        ))
                    }
                    Err(_) => Q::Capture(name.clone()),
                }
            }
            ExprKind::Absent => Q::Absent,
            ExprKind::Bool(value) => Q::Bool(*value),
            ExprKind::Literal(value) => Q::Text(value.clone()),
            ExprKind::Group(value) => self.query_expr_normalized(symbol, value)?,
            ExprKind::Not(value) => Q::Not(Box::new(self.query_expr_normalized(symbol, value)?)),
            ExprKind::Tuple(values) => Q::Tuple(
                values
                    .iter()
                    .map(|value| self.query_expr_normalized(symbol, value))
                    .collect::<Result<Vec<_>>>()?,
            ),
            ExprKind::List(values) => Q::List(
                values
                    .iter()
                    .map(|value| self.query_expr_normalized(symbol, value))
                    .collect::<Result<Vec<_>>>()?,
            ),
            ExprKind::Call(name, args) => Q::Call {
                name: name.clone(),
                args: self.query_arguments(symbol, name, args, &expr.span)?,
            },
            _ => {
                return Err(Diagnostic::error(
                    "yl.invalid_query_value",
                    "Expected a declarative query value",
                    expr.span.clone(),
                ))
            }
        })
    }
    fn query_arguments(
        &self,
        symbol: &Symbol,
        id: &str,
        args: &[Argument],
        span: &Span,
    ) -> Result<Vec<crate::semantics::queries::QueryExpr>> {
        let contract = self.registry.signature(id).ok_or_else(|| {
            Diagnostic::error(
                "yl.unknown_operation",
                "Unknown query primitive",
                span.clone(),
            )
        })?;
        if !id.starts_with("std/query#") {
            return Err(Diagnostic::error(
                "yl.query_native_effect",
                "Declarative queries cannot call native lifecycle operations",
                span.clone(),
            ));
        }
        let mut values = BTreeMap::new();
        let mut position = 0;
        let mut named = false;
        for arg in args {
            let parameter = if let Some(name) = &arg.name {
                named = true;
                contract
                    .parameters
                    .iter()
                    .find(|parameter| &parameter.name == name)
            } else {
                if named {
                    return Err(Diagnostic::error(
                        "yl.invalid_argument",
                        "Positional argument follows named argument",
                        arg.span.clone(),
                    ));
                }
                let p = contract.parameters.get(position);
                position += 1;
                p
            }
            .ok_or_else(|| {
                Diagnostic::error(
                    "yl.invalid_argument",
                    "Unknown or excess query argument",
                    arg.span.clone(),
                )
            })?;
            if values
                .insert(
                    parameter.name.clone(),
                    self.query_expr_normalized(symbol, &arg.value)?,
                )
                .is_some()
            {
                return Err(Diagnostic::error(
                    "yl.invalid_argument",
                    "Duplicate query argument",
                    arg.span.clone(),
                ));
            }
        }
        contract
            .parameters
            .iter()
            .map(|parameter| {
                values.remove(&parameter.name).ok_or_else(|| {
                    Diagnostic::error(
                        "yl.invalid_argument",
                        format!("Missing query argument {}", parameter.name),
                        span.clone(),
                    )
                })
            })
            .collect()
    }
    fn lower_query_actions(
        &self,
        symbol: &Symbol,
        actions: &[MeaningSyntaxAction],
        bindings: &BTreeMap<String, Expr>,
        stack: &mut Vec<String>,
    ) -> Result<Vec<crate::semantics::queries::QueryAction>> {
        use crate::semantics::queries::{QueryAction as A, QueryCase, QueryMessagePart};
        let mut result = vec![];
        for action in actions {
            match action {
                MeaningSyntaxAction::If {
                    condition,
                    actions,
                    otherwise,
                    span,
                } => result.push(A::If {
                    condition: self.lower_query_expr(symbol, condition, bindings, stack)?,
                    then_actions: self.lower_query_actions(symbol, actions, bindings, stack)?,
                    else_actions: self.lower_query_actions(symbol, otherwise, bindings, stack)?,
                    span: span.clone(),
                }),
                MeaningSyntaxAction::Match(matching) => {
                    let inputs = matching
                        .inputs
                        .iter()
                        .map(|input| self.lower_query_expr(symbol, input, bindings, stack))
                        .collect::<Result<Vec<_>>>()?;
                    let mut cases = vec![];
                    for (index, case) in matching.cases.iter().enumerate() {
                        let alternatives = case
                            .alternatives
                            .iter()
                            .map(|alternative| {
                                if alternative.len() != inputs.len()
                                    || (index + 1 != matching.cases.len()
                                        && alternative.iter().all(Option::is_none))
                                {
                                    return Err(Diagnostic::error(
                                        "yl.invalid_meaning_match",
                                        "Match arity differs or wildcard is not last",
                                        case.span.clone(),
                                    ));
                                }
                                alternative
                                    .iter()
                                    .map(|value| {
                                        value
                                            .as_ref()
                                            .map(|value| {
                                                self.lower_query_expr(
                                                    symbol, value, bindings, stack,
                                                )
                                            })
                                            .transpose()
                                    })
                                    .collect::<Result<Vec<_>>>()
                            })
                            .collect::<Result<Vec<_>>>()?;
                        let mut actions = case
                            .actions
                            .iter()
                            .cloned()
                            .map(MeaningSyntaxAction::Call)
                            .collect::<Vec<_>>();
                        actions.extend(case.nested_actions.clone());
                        cases.push(QueryCase {
                            alternatives,
                            actions: self.lower_query_actions(symbol, &actions, bindings, stack)?,
                            span: case.span.clone(),
                        });
                    }
                    if !matching.cases.last().is_some_and(|case| {
                        case.alternatives
                            .iter()
                            .any(|alt| alt.iter().all(Option::is_none))
                    }) {
                        return Err(Diagnostic::error(
                            "yl.non_exhaustive_meaning_match",
                            "Match requires a final wildcard case",
                            matching.span.clone(),
                        ));
                    }
                    result.push(A::Match {
                        inputs,
                        cases,
                        span: matching.span.clone(),
                    });
                }
                MeaningSyntaxAction::Call(call) => {
                    if matches!(call.name.as_str(), "error" | "warn" | "warning") {
                        let (arg, target, related) = match call.arguments.as_slice() {
                            [arg] => (arg, None, vec![]),
                            [arg, target] => (
                                arg,
                                Some(self.lower_query_expr(
                                    symbol,
                                    &target.value,
                                    bindings,
                                    stack,
                                )?),
                                vec![],
                            ),
                            [arg, target, related] => (
                                arg,
                                Some(self.lower_query_expr(
                                    symbol,
                                    &target.value,
                                    bindings,
                                    stack,
                                )?),
                                vec![self.lower_query_expr(
                                    symbol,
                                    &related.value,
                                    bindings,
                                    stack,
                                )?],
                            ),
                            _ => {
                                return Err(Diagnostic::error(
                                    "yl.invalid_diagnostic",
                                    "Diagnostic needs a message and optional source target",
                                    call.span.clone(),
                                ))
                            }
                        };
                        let expanded =
                            self.expand_semantic_expr(symbol, &arg.value, bindings, stack)?;
                        let ExprKind::Literal(text) = &expanded.kind else {
                            return Err(Diagnostic::error(
                                "yl.invalid_diagnostic",
                                "Diagnostic needs a string",
                                call.span.clone(),
                            ));
                        };
                        let mut message = vec![];
                        let mut rest = text.as_str();
                        while let Some((before, after)) = rest.split_once("${") {
                            message.push(QueryMessagePart::Text(before.into()));
                            let mut quoted = false;
                            let mut escaped = false;
                            let close = after
                                .char_indices()
                                .find_map(|(offset, ch)| {
                                    if escaped {
                                        escaped = false;
                                        return None;
                                    }
                                    if quoted && ch == '\\' {
                                        escaped = true;
                                        return None;
                                    }
                                    if ch == '"' {
                                        quoted = !quoted;
                                        return None;
                                    }
                                    (ch == '}' && !quoted).then_some(offset)
                                })
                                .ok_or_else(|| {
                                    Diagnostic::error(
                                        "yl.invalid_diagnostic",
                                        "Unterminated diagnostic interpolation",
                                        arg.span.clone(),
                                    )
                                })?;
                            let capture = &after[..close];
                            let tail = &after[close + 1..];
                            let expression =
                                crate::frontend::parse_semantic_expr(&arg.span.file, capture)
                                    .map_err(|errors| {
                                        Diagnostic::error(
                                            "yl.invalid_diagnostic",
                                            format!(
                                                "Invalid diagnostic interpolation: {}",
                                                errors[0].message
                                            ),
                                            arg.span.clone(),
                                        )
                                    })?;
                            let value =
                                self.lower_query_expr(symbol, &expression, bindings, stack)?;
                            message.push(QueryMessagePart::Value(value));
                            rest = tail;
                        }
                        message.push(QueryMessagePart::Text(rest.into()));
                        result.push(A::Emit {
                            severity: if call.name == "error" {
                                Severity::Error
                            } else {
                                Severity::Warning
                            },
                            message,
                            span: call.span.clone(),
                            target,
                            related,
                        });
                        continue;
                    }
                    let id = self.resolve(&symbol.module, &symbol.name, &call.name, &call.span)?;
                    let args = call
                        .arguments
                        .iter()
                        .map(|arg| {
                            Ok(Argument {
                                value: self
                                    .expand_semantic_expr(symbol, &arg.value, bindings, stack)?,
                                ..arg.clone()
                            })
                        })
                        .collect::<Result<Vec<_>>>()?;
                    if let Some(wrapper) = self.symbols.get(&id) {
                        if let DeclKind::Operation {
                            parameters, body, ..
                        } = &wrapper.declaration.kind
                        {
                            let env = self.bind_semantic_wrapper(
                                wrapper, parameters, &args, &call.span, stack,
                            )?;
                            let nested = match body {
                                OperationBody::Actions(calls) => calls
                                    .iter()
                                    .cloned()
                                    .map(MeaningSyntaxAction::Call)
                                    .collect(),
                                OperationBody::Conditional(actions) => actions.clone(),
                                OperationBody::Query(_) => {
                                    return Err(Diagnostic::error(
                                        "yl.invalid_operation_action",
                                        "Query operation cannot be used as an action",
                                        call.span.clone(),
                                    ))
                                }
                            };
                            result.extend(self.lower_query_actions(wrapper, &nested, &env, stack)?);
                            stack.pop();
                            continue;
                        }
                    }
                    result.push(A::Call {
                        name: id.clone(),
                        args: self.query_arguments(symbol, &id, &args, &call.span)?,
                        span: call.span.clone(),
                    });
                }
            }
        }
        Ok(result)
    }
    fn lower_meanings(&self, symbol: &Symbol, meanings: &Meanings) -> Result<MeaningDefinition> {
        let mut groups = vec![];
        let mut names = BTreeSet::new();
        for group in &meanings.groups {
            if self.is_query_group(symbol, group)? {
                groups.push(MeaningGroup {
                    name: group.name.clone(),
                    calls: vec![],
                    matching: None,
                    span: group.span.clone(),
                });
                continue;
            }
            if group
                .name
                .as_ref()
                .is_some_and(|name| !names.insert(name.clone()))
            {
                return Err(Diagnostic::error(
                    "yl.duplicate_meaning_group",
                    "Duplicate meaning group",
                    group.span.clone(),
                ));
            }
            let calls = group
                .calls
                .iter()
                .map(|call| self.lower_meaning_actions(symbol, call))
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .flatten()
                .collect();
            let matching = group
                .matching
                .as_ref()
                .map(|matching| self.lower_meaning_match(symbol, matching))
                .transpose()?;
            groups.push(MeaningGroup {
                name: group.name.clone(),
                calls,
                matching,
                span: group.span.clone(),
            });
        }
        let mut precedence = vec![];
        for chain in &meanings.precedence {
            let mut selectors = vec![];
            for selector in &chain.selectors {
                let ExprKind::Ref(name) = &selector.kind else {
                    return Err(Diagnostic::error(
                        "yl.invalid_meaning_selector",
                        "Expected a definition selector",
                        selector.span.clone(),
                    ));
                };
                if name == "_" {
                    return Err(Diagnostic::error(
                        "yl.invalid_meaning_selector",
                        "Wildcard meaning selectors are not supported",
                        selector.span.clone(),
                    ));
                }
                let (definition, group) = name
                    .split_once("::meanings::")
                    .map_or((name.as_str(), None), |(definition, group)| {
                        (definition, Some(group.to_owned()))
                    });
                let definition =
                    self.resolve(&symbol.module, &symbol.name, definition, &selector.span)?;
                let target = self.symbols.get(&definition).ok_or_else(|| {
                    Diagnostic::error(
                        "yl.invalid_meaning_selector",
                        "Meaning selectors require a node or pattern",
                        selector.span.clone(),
                    )
                })?;
                let target_meanings = match &target.declaration.kind {
                    DeclKind::Node { meanings, .. } | DeclKind::Pattern { meanings, .. } => {
                        meanings
                    }
                    _ => {
                        return Err(Diagnostic::error(
                            "yl.invalid_meaning_selector",
                            "Meaning selectors require a node or pattern",
                            selector.span.clone(),
                        ))
                    }
                };
                if let Some(group) = &group {
                    if !target_meanings.as_ref().is_some_and(|meanings| {
                        meanings
                            .groups
                            .iter()
                            .any(|candidate| candidate.name.as_ref() == Some(group))
                    }) {
                        return Err(Diagnostic::error(
                            "yl.unknown_meaning_group",
                            format!("Unknown meaning group {group}"),
                            selector.span.clone(),
                        ));
                    }
                }
                selectors.push(MeaningSelector { definition, group });
            }
            precedence.push(MeaningChain {
                selectors,
                span: chain.span.clone(),
            });
        }
        Ok(MeaningDefinition {
            boundary: meanings.boundary,
            groups,
            precedence,
        })
    }
    #[allow(clippy::too_many_arguments)]
    fn bind(
        &mut self,
        symbol: &Symbol,
        parameters: &[Parameter],
        args: &[Argument],
        module: &str,
        scope: &str,
        caller: &Environment,
        depth: usize,
    ) -> Result<Environment> {
        let mut supplied = BTreeMap::new();
        let mut position = 0;
        let mut seen_named = false;
        for arg in args {
            let name = if let Some(name) = &arg.name {
                seen_named = true;
                name.clone()
            } else {
                if seen_named {
                    return Err(Diagnostic::error(
                        "yl.invalid_argument",
                        "Positional argument after named argument",
                        arg.span.clone(),
                    ));
                }
                let parameter = parameters.get(position).ok_or_else(|| {
                    Diagnostic::error(
                        "yl.invalid_argument",
                        "Too many arguments",
                        arg.span.clone(),
                    )
                })?;
                position += 1;
                parameter.name.clone()
            };
            if !parameters.iter().any(|p| p.name == name)
                || supplied.insert(name, &arg.value).is_some()
            {
                return Err(Diagnostic::error(
                    "yl.invalid_argument",
                    "Unknown or duplicate argument",
                    arg.span.clone(),
                ));
            }
        }
        let mut env = Environment::new();
        for p in parameters {
            if env.contains_key(&p.name) {
                return Err(Diagnostic::error(
                    "yl.duplicate_member",
                    "Duplicate parameter",
                    p.span.clone(),
                ));
            }
            let (value, context_module, context_scope, context_env) =
                if let Some(value) = supplied.get(&p.name) {
                    (*value, module, scope, caller)
                } else {
                    (
                        p.default.as_ref().ok_or_else(|| {
                            Diagnostic::error(
                                "yl.invalid_argument",
                                format!("Missing argument {}", p.name),
                                p.span.clone(),
                            )
                        })?,
                        symbol.module.as_str(),
                        symbol.name.as_str(),
                        &env,
                    )
                };
            let ty =
                p.ty.as_ref()
                    .map(|t| self.resolve(&symbol.module, &symbol.scope, t, &p.span))
                    .transpose()?;
            let enum_type = ty.as_ref().is_some_and(|ty| {
                self.symbols
                    .get(ty)
                    .is_some_and(|s| matches!(s.declaration.kind, DeclKind::Enum { .. }))
            });
            let bound = if enum_type {
                let (ty, v) = self.enum_value(
                    context_module,
                    context_scope,
                    value,
                    ty.as_deref(),
                    context_env,
                )?;
                let origin = if let ExprKind::Ref(name) = &ungroup(value).kind {
                    if let Some(Value::Enum(_, _, origin)) = context_env.get(name) {
                        origin.clone()
                    } else if supplied.contains_key(&p.name) {
                        Some(value.span.clone())
                    } else {
                        None
                    }
                } else if supplied.contains_key(&p.name) {
                    Some(value.span.clone())
                } else {
                    None
                };
                Value::Enum(ty, v, origin)
            } else {
                let term =
                    self.lower(context_module, context_scope, value, context_env, depth + 1)?;
                if let Some(ty) = ty {
                    if !self.accepts(&term, &ty, &mut BTreeSet::new())? {
                        return Err(Diagnostic::error(
                            "yl.invalid_typed_argument",
                            format!(
                                "Grammar does not produce {}",
                                p.ty.as_deref().unwrap_or("expected type")
                            ),
                            value.span.clone(),
                        ));
                    }
                }
                Value::Grammar(term)
            };
            env.insert(p.name.clone(), bound);
        }
        Ok(env)
    }
    fn enum_value(
        &self,
        module: &str,
        scope: &str,
        expr: &Expr,
        expected: Option<&str>,
        env: &Environment,
    ) -> Result<(String, String)> {
        let (ty, variant) = match &ungroup(expr).kind {
            ExprKind::EnumConstant(ty, variant) => (ty.clone(), variant.clone()),
            ExprKind::Variant(v) => (
                expected
                    .ok_or_else(|| {
                        Diagnostic::error(
                            "yl.ambiguous_enum_variant",
                            "Expected enum type is ambiguous",
                            expr.span.clone(),
                        )
                    })?
                    .into(),
                v.clone(),
            ),
            ExprKind::Ref(name) => {
                if let Some(Value::Enum(ty, v, _)) = env.get(name) {
                    return Ok((ty.clone(), v.clone()));
                }
                let (ty, v) = name.rsplit_once("::").ok_or_else(|| {
                    Diagnostic::error(
                        "yl.invalid_argument",
                        "Expected enum variant",
                        expr.span.clone(),
                    )
                })?;
                (self.resolve(module, scope, ty, &expr.span)?, v.into())
            }
            _ => {
                return Err(Diagnostic::error(
                    "yl.invalid_argument",
                    "Expected enum variant",
                    expr.span.clone(),
                ))
            }
        };
        if expected.is_some_and(|e| e != ty) {
            return Err(Diagnostic::error(
                "yl.invalid_typed_argument",
                "Wrong enum type",
                expr.span.clone(),
            ));
        }
        if !self.symbols.get(&ty).is_some_and(|s|matches!(&s.declaration.kind,DeclKind::Enum { variants,.. } if variants.contains(&variant))) { return Err(Diagnostic::error("yl.invalid_argument","Unknown enum variant",expr.span.clone())); }
        Ok((ty, variant))
    }
    fn accepts(&self, term: &Term, ty: &str, visiting: &mut BTreeSet<String>) -> Result<bool> {
        match term {
            Term::Ref(id) => {
                if id == ty {
                    return Ok(true);
                }
                let mut leaves = vec![];
                self.leaves(ty, visiting, &mut leaves)?;
                let mut actual = vec![];
                self.leaves(id, &mut BTreeSet::new(), &mut actual)?;
                Ok(actual.iter().all(|id| leaves.contains(id)))
            }
            Term::Capture(_, t)
            | Term::Meaning { term: t, .. }
            | Term::Example { term: t, .. }
            | Term::Mark { term: t, .. } => self.accepts(t, ty, visiting),
            Term::Project {
                id,
                term,
                quantifier: None,
                ..
            } => {
                if let Some(original) = find_marker(term, *id) {
                    self.accepts(original, ty, visiting)
                } else {
                    Ok(false)
                }
            }
            Term::Choice(items) => {
                for t in items {
                    if !self.accepts(t, ty, visiting)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            Term::Sequence(items) => {
                let values: Vec<_> = items
                    .iter()
                    .filter(|t| !matches!(t, Term::NotAhead(_) | Term::NotBehind(_)))
                    .collect();
                if values.len() == 1 {
                    self.accepts(values[0], ty, visiting)
                } else {
                    Ok(false)
                }
            }
            _ => Ok(false),
        }
    }
    fn lower_checks(
        &self,
        module: &str,
        scope: &str,
        constraints: &[Constraint],
        args: &BTreeMap<String, Expr>,
        depth: usize,
    ) -> Result<Vec<Check>> {
        if depth > 64 {
            return Err(Diagnostic::error(
                "yl.constraint_cycle",
                "Recursive constraint expansion",
                Span::default(),
            ));
        }
        let mut result = vec![];
        for constraint in constraints {
            match &constraint.kind {
                ConstraintKind::Call(name, values) => {
                    let id = self.resolve(module, scope, name, &constraint.span)?;
                    let symbol = self.symbols.get(&id).ok_or_else(|| {
                        Diagnostic::error(
                            "yl.unknown_reference",
                            "Unknown constraint",
                            constraint.span.clone(),
                        )
                    })?;
                    let DeclKind::Constraint {
                        parameters, body, ..
                    } = &symbol.declaration.kind
                    else {
                        return Err(Diagnostic::error(
                            "yl.invalid_constraint",
                            "Expected reusable constraint",
                            constraint.span.clone(),
                        ));
                    };
                    let mut bound = bind_constraint(parameters, values, args, &constraint.span)?;
                    for value in bound.values_mut() {
                        if matches!(&value.kind,ExprKind::Ref(name) if name.contains("::")) {
                            let source_module = if self.modules.contains_key(&value.span.file) {
                                value.span.file.as_str()
                            } else {
                                module
                            };
                            let (ty, variant) = self.enum_value(
                                source_module,
                                scope,
                                value,
                                None,
                                &Environment::new(),
                            )?;
                            value.kind = ExprKind::EnumConstant(ty, variant);
                        }
                    }
                    for parameter in parameters {
                        if let Some(ty) = &parameter.ty {
                            let ty =
                                self.resolve(&symbol.module, &symbol.scope, ty, &parameter.span)?;
                            let value = &bound[&parameter.name];
                            if self.symbols.get(&ty).is_some_and(|s| {
                                matches!(s.declaration.kind, DeclKind::Enum { .. })
                            }) {
                                let (ty, variant) = self.enum_value(
                                    module,
                                    scope,
                                    value,
                                    Some(&ty),
                                    &Environment::new(),
                                )?;
                                let span = value.span.clone();
                                bound.insert(
                                    parameter.name.clone(),
                                    Expr {
                                        kind: ExprKind::EnumConstant(ty, variant),
                                        span,
                                    },
                                );
                            } else {
                                if !self.symbols.get(&ty).is_some_and(|s| {
                                    matches!(s.declaration.kind, DeclKind::Node { .. })
                                }) {
                                    return Err(Diagnostic::error(
                                        "yl.invalid_typed_argument",
                                        "Constraint type must be a node or enum",
                                        parameter.span.clone(),
                                    ));
                                }
                                let ExprKind::Ref(capture) = &value.kind else {
                                    return Err(Diagnostic::error(
                                        "yl.invalid_typed_argument",
                                        "Typed constraint argument must be a node capture",
                                        value.span.clone(),
                                    ));
                                };
                                result.push(Check {
                                    condition: Condition::CaptureType {
                                        capture: capture.clone(),
                                        rule: ty,
                                    },
                                    emissions: vec![],
                                });
                            }
                        }
                    }
                    result.extend(self.lower_checks(
                        &symbol.module,
                        &symbol.scope,
                        body,
                        &bound,
                        depth + 1,
                    )?);
                }
                ConstraintKind::When(condition, body) => {
                    let condition = substitute(condition, args);
                    let condition = self.condition(module, scope, &condition)?;
                    let mut emissions = vec![];
                    for c in body {
                        let ConstraintKind::Call(name, values) = &c.kind else {
                            if !emissions.is_empty() {
                                result.push(Check {
                                    condition: condition.clone(),
                                    emissions: std::mem::take(&mut emissions),
                                });
                            }
                            for mut nested in self.lower_checks(
                                module,
                                scope,
                                std::slice::from_ref(c),
                                args,
                                depth + 1,
                            )? {
                                nested.condition = Condition::And(
                                    Box::new(condition.clone()),
                                    Box::new(nested.condition),
                                );
                                result.push(nested);
                            }
                            continue;
                        };
                        let severity = match name.as_str() {
                            "error" => Severity::Error,
                            "warning" => Severity::Warning,
                            "help" => Severity::Help,
                            _ => {
                                return Err(Diagnostic::error(
                                    "yl.invalid_constraint",
                                    "when body requires diagnostic emission",
                                    c.span.clone(),
                                ))
                            }
                        };
                        let [argument] = values.as_slice() else {
                            return Err(Diagnostic::error(
                                "yl.invalid_argument",
                                "Diagnostic emission takes one positional string",
                                c.span.clone(),
                            ));
                        };
                        if argument.name.is_some() {
                            return Err(Diagnostic::error(
                                "yl.invalid_argument",
                                "Diagnostic emission takes one positional string",
                                argument.span.clone(),
                            ));
                        }
                        let message = substitute(&argument.value, args);
                        let Expr {
                            kind: ExprKind::Literal(message),
                            ..
                        } = &message
                        else {
                            return Err(Diagnostic::error(
                                "yl.invalid_argument",
                                "Diagnostic emission takes one string",
                                c.span.clone(),
                            ));
                        };
                        emissions.push(Emission {
                            severity,
                            message: message.clone(),
                            span: c.span.clone(),
                        });
                    }
                    result.push(Check {
                        condition,
                        emissions,
                    });
                }
            }
        }
        Ok(result)
    }
    fn condition(&self, module: &str, scope: &str, expr: &Expr) -> Result<Condition> {
        self.condition_depth(module, scope, expr, 0)
    }
    fn condition_depth(
        &self,
        module: &str,
        scope: &str,
        expr: &Expr,
        depth: usize,
    ) -> Result<Condition> {
        if depth > 128 {
            return Err(Diagnostic::error(
                "yl.depth_limit",
                "Condition nesting exceeds resource limit",
                expr.span.clone(),
            ));
        }
        let bad =
            |message: &str| Diagnostic::error("yl.invalid_constraint", message, expr.span.clone());
        match &expr.kind {
            ExprKind::Group(inner) => self.condition_depth(module, scope, inner, depth + 1),
            ExprKind::EnumConstant(ty, variant) => Ok(Condition::EnumValue {
                ty: ty.clone(),
                variant: variant.clone(),
            }),
            ExprKind::Bool(value) => Ok(Condition::Bool(*value)),
            ExprKind::Absent => Ok(Condition::Absent),
            ExprKind::Literal(value) => Ok(Condition::Text(value.clone())),
            ExprKind::Ref(name) if name == "true" || name == "false" => {
                Ok(Condition::Bool(name == "true"))
            }
            ExprKind::Ref(name) if name == "absent" => Ok(Condition::Absent),
            ExprKind::Ref(name) if name.contains("::") => {
                let (ty, variant) =
                    self.enum_value(module, scope, expr, None, &Environment::new())?;
                Ok(Condition::EnumValue { ty, variant })
            }
            ExprKind::Ref(name) => Ok(Condition::CaptureValue(name.clone())),
            ExprKind::Not(inner) => Ok(Condition::Not(Box::new(self.condition_depth(
                module,
                scope,
                inner,
                depth + 1,
            )?))),
            ExprKind::Binary(op, left, right) => {
                let enum_operand = |variant: &Expr, other: &Expr| -> Result<Condition> {
                    let (ty, _) =
                        self.enum_value(module, scope, ungroup(other), None, &Environment::new())?;
                    let (ty, variant) = self.enum_value(
                        module,
                        scope,
                        ungroup(variant),
                        Some(&ty),
                        &Environment::new(),
                    )?;
                    Ok(Condition::EnumValue { ty, variant })
                };
                let a = if matches!(ungroup(left).kind, ExprKind::Variant(_)) {
                    enum_operand(left, right)?
                } else {
                    self.condition_depth(module, scope, left, depth + 1)?
                };
                let b = if matches!(ungroup(right).kind, ExprKind::Variant(_)) {
                    enum_operand(right, left)?
                } else {
                    self.condition_depth(module, scope, right, depth + 1)?
                };
                let (a, b) = (Box::new(a), Box::new(b));
                Ok(match op.as_str() {
                    "&&" => Condition::And(a, b),
                    "||" => Condition::Or(a, b),
                    "==" => Condition::Equal(a, b),
                    "!=" => Condition::Not(Box::new(Condition::Equal(a, b))),
                    _ => return Err(bad("Unknown condition operator")),
                })
            }
            ExprKind::Call(name, arguments) if name.ends_with(".isPresent") => {
                if !arguments.is_empty() {
                    return Err(bad("isPresent takes no arguments"));
                }
                Ok(Condition::Present(
                    name.trim_end_matches(".isPresent").into(),
                ))
            }
            ExprKind::Call(name, arguments) if name.ends_with(".between") => {
                let [a, b] = arguments.as_slice() else {
                    return Err(bad("between requires two captures"));
                };
                let (ExprKind::Ref(left), ExprKind::Ref(right)) =
                    (&ungroup(&a.value).kind, &ungroup(&b.value).kind)
                else {
                    return Err(bad("between requires capture references"));
                };
                let owner = name.trim_end_matches(".between");
                let trivia = if owner == "trivia" {
                    None
                } else {
                    Some(self.resolve(module, scope, owner, &expr.span)?)
                };
                Ok(Condition::Between {
                    trivia,
                    left: left.clone(),
                    right: right.clone(),
                })
            }
            ExprKind::Call(name, arguments) if name.ends_with(".matches") => {
                let [arg] = arguments.as_slice() else {
                    return Err(bad("matches requires one regex"));
                };
                let ExprKind::Regex(regex) = &ungroup(&arg.value).kind else {
                    return Err(bad("matches requires a regex"));
                };
                regex::Regex::new(regex).map_err(|e| bad(&e.to_string()))?;
                let capture = name.trim_end_matches(".matches");
                if let Some(capture) = capture.strip_suffix('?') {
                    Ok(Condition::OptionalMatches {
                        capture: capture.into(),
                        regex: regex.clone(),
                    })
                } else {
                    Ok(Condition::Matches {
                        capture: capture.into(),
                        regex: regex.clone(),
                    })
                }
            }
            _ => Err(bad("Unsupported condition expression")),
        }
    }
}
fn bind_constraint(
    parameters: &[Parameter],
    arguments: &[Argument],
    caller: &BTreeMap<String, Expr>,
    span: &Span,
) -> Result<BTreeMap<String, Expr>> {
    let mut supplied = BTreeMap::new();
    let mut position = 0;
    let mut named = false;
    for argument in arguments {
        let name = if let Some(name) = &argument.name {
            named = true;
            name.clone()
        } else {
            if named {
                return Err(Diagnostic::error(
                    "yl.invalid_argument",
                    "Positional argument after named argument",
                    argument.span.clone(),
                ));
            }
            let parameter = parameters.get(position).ok_or_else(|| {
                Diagnostic::error(
                    "yl.invalid_argument",
                    "Too many constraint arguments",
                    argument.span.clone(),
                )
            })?;
            position += 1;
            parameter.name.clone()
        };
        if !parameters.iter().any(|p| p.name == name)
            || supplied
                .insert(name, substitute(&argument.value, caller))
                .is_some()
        {
            return Err(Diagnostic::error(
                "yl.invalid_argument",
                "Unknown or duplicate constraint argument",
                argument.span.clone(),
            ));
        }
    }
    let mut bound = BTreeMap::new();
    for parameter in parameters {
        let value = if let Some(value) = supplied.remove(&parameter.name) {
            value
        } else if let Some(default) = &parameter.default {
            substitute(default, &bound)
        } else {
            return Err(Diagnostic::error(
                "yl.invalid_argument",
                format!("Missing constraint argument {}", parameter.name),
                span.clone(),
            ));
        };
        bound.insert(parameter.name.clone(), ungroup(&value).clone());
    }
    Ok(bound)
}
fn ungroup(mut expr: &Expr) -> &Expr {
    while let ExprKind::Group(inner) = &expr.kind {
        expr = inner;
    }
    expr
}
fn substitute(expr: &Expr, args: &BTreeMap<String, Expr>) -> Expr {
    if let ExprKind::Ref(name) = &expr.kind {
        if let Some(value) = args.get(name) {
            return value.clone();
        }
    }
    let mut result = expr.clone();
    if let ExprKind::Group(inner) = &mut result.kind {
        **inner = substitute(inner, args);
    }
    if let ExprKind::Not(inner) = &mut result.kind {
        **inner = substitute(inner, args);
    }
    if let ExprKind::Binary(_, left, right) = &mut result.kind {
        **left = substitute(left, args);
        **right = substitute(right, args);
    }
    if let ExprKind::Call(name, values) = &mut result.kind {
        if let Some((receiver, method)) = name.rsplit_once('.') {
            let optional = receiver.ends_with('?');
            let receiver = receiver.trim_end_matches('?');
            if let Some(Expr {
                kind: ExprKind::Ref(actual),
                ..
            }) = args.get(receiver)
            {
                *name = format!("{actual}{}.{method}", if optional { "?" } else { "" });
            }
        }
        for value in values {
            value.value = substitute(&value.value, args);
        }
    }
    result
}
pub(crate) fn edge_term(term: &Term, right: bool) -> Option<&Term> {
    match term {
        Term::Ref(_) => Some(term),
        Term::Capture(_, term)
        | Term::Meaning { term, .. }
        | Term::Example { term, .. }
        | Term::Mark { term, .. }
        | Term::Project { term, .. } => edge_term(term, right),
        Term::Sequence(items) => {
            let mut candidates = items.iter().filter(|t| !zero_width(t));
            let edge = if right {
                candidates.next_back()
            } else {
                candidates.next()
            }?;
            edge_term(edge, right)
        }
        _ => None,
    }
}
fn zero_width(term: &Term) -> bool {
    match term {
        Term::NotAhead(_) | Term::NotBehind(_) => true,
        Term::Literal(text) => text.is_empty(),
        Term::Capture(_, term)
        | Term::Meaning { term, .. }
        | Term::Example { term, .. }
        | Term::Mark { term, .. }
        | Term::Project { term, .. } => zero_width(term),
        Term::Sequence(items) => items.iter().all(zero_width),
        _ => false,
    }
}
fn edge_ref(term: &Term, right: bool) -> Option<&str> {
    match edge_term(term, right)? {
        Term::Ref(id) => Some(id),
        _ => None,
    }
}

/// Native standard library artifacts must be recompiled against authored modules.
pub(crate) fn reject_retired_standard_operation(id: &str, span: &Span) -> Result<()> {
    if id.starts_with("std/variables#")
        || id.starts_with("std/references#")
        || id.starts_with("std/types#")
    {
        return Err(Diagnostic::error("yl.artifact_retired_std",
            format!("Native standard operation {id} has been retired; recompile the language against the authored standard modules"),
            span.clone()));
    }
    Ok(())
}
pub(crate) fn validate_language(language: &CompiledLanguage) -> Result<()> {
    if !matches!(language.version, 1..=4)
        || (language.version < 3 && !language.query_meanings.is_empty())
        || (language.version == 1
            && (!language.meanings.is_empty() || !language.query_meanings.is_empty()))
        || !language.rules.contains_key(&language.entry)
    {
        return Err(Diagnostic::error(
            "yl.artifact",
            "Invalid artifact version or entry rule",
            Span::default(),
        ));
    }
    fn term(t: &Term, language: &CompiledLanguage, span: &Span, depth: usize) -> Result<()> {
        if depth > 128 {
            return Err(Diagnostic::error(
                "yl.artifact",
                "Artifact term nesting exceeds resource limit",
                span.clone(),
            ));
        }
        match t {
            Term::Example {
                definition,
                term: inner,
            } => {
                if !language.examples.contains_key(definition) {
                    return Err(Diagnostic::error(
                        "yl.artifact",
                        "Unknown example metadata reference",
                        span.clone(),
                    ));
                }
                term(inner, language, span, depth + 1)
            }
            Term::Meaning {
                definition,
                term: inner,
            } => {
                let meanings = language.meanings.get(definition).ok_or_else(|| {
                    Diagnostic::error(
                        "yl.artifact",
                        format!("Unknown meaning boundary {definition}"),
                        span.clone(),
                    )
                })?;
                validate_meaning_captures(meanings, &capture_sets(inner).0)?;
                if let Some(groups) = language.query_meanings.get(definition) {
                    crate::semantics::queries::validate_groups(
                        groups,
                        &capture_sets(inner).0,
                        language.version,
                    )?;
                }
                term(inner, language, span, depth + 1)
            }
            Term::Ref(id) if !language.rules.contains_key(id) => Err(Diagnostic::error(
                "yl.artifact",
                format!("Unknown rule {id}"),
                span.clone(),
            )),
            Term::Regex(s) => regex::Regex::new(s)
                .map(|_| ())
                .map_err(|e| Diagnostic::error("yl.invalid_regex", e.to_string(), span.clone())),
            Term::Sequence(items) | Term::Choice(items) => {
                for t in items {
                    term(t, language, span, depth + 1)?;
                }
                Ok(())
            }
            Term::Repeat(t, _)
            | Term::Capture(_, t)
            | Term::NotAhead(t)
            | Term::NotBehind(t)
            | Term::Mark { term: t, .. }
            | Term::Project { term: t, .. } => term(t, language, span, depth + 1),
            _ => Ok(()),
        }
    }
    for (family, rule) in &language.rules {
        if let RuleBody::Concrete(t) = &rule.body {
            validate_projections(t, &mut BTreeSet::new(), &rule.span)?;
        }
        match &rule.body {
            RuleBody::Concrete(t) => term(t, language, &rule.span, 0)?,
            RuleBody::Abstract { bases, operators } => {
                for id in bases.iter().chain(operators.iter().map(|o| &o.rule)) {
                    if !language.rules.contains_key(id) {
                        return Err(Diagnostic::error(
                            "yl.artifact",
                            "Unknown abstract member",
                            rule.span.clone(),
                        ));
                    }
                }
                let mut operator_rules = BTreeSet::new();
                for operator in operators {
                    let member = language.rules.get(&operator.rule).ok_or_else(|| {
                        Diagnostic::error(
                            "yl.artifact",
                            "Unknown precedence operator",
                            rule.span.clone(),
                        )
                    })?;
                    let RuleBody::Concrete(grammar) = &member.body else {
                        return Err(Diagnostic::error(
                            "yl.artifact",
                            "Precedence operator is not a concrete rule",
                            rule.span.clone(),
                        ));
                    };
                    let left = edge_ref(grammar, false) == Some(family.as_str());
                    let right = edge_ref(grammar, true) == Some(family.as_str());
                    if operator.binding_power == 0
                        || operator.binding_power > language.rules.len()
                        || (!left && !right)
                        || operator.left != left
                        || operator.right != right
                        || !operator_rules.insert(&operator.rule)
                    {
                        return Err(Diagnostic::error(
                            "yl.artifact",
                            "Invalid precedence binding power, role or duplicate operator",
                            rule.span.clone(),
                        ));
                    }
                }
            }
        }
        if let RuleBody::Concrete(t) = &rule.body {
            captures(t, &rule.span)?;
        }
        let (captures, guaranteed) = rule_capture_sets(rule, language, &mut BTreeSet::new());
        if let Some(meanings) = language.meanings.get(family) {
            validate_meaning_captures(meanings, &captures)?;
        }
        if let Some(groups) = language.query_meanings.get(family) {
            crate::semantics::queries::validate_groups(groups, &captures, language.version)?;
        }
        for check in &rule.constraints {
            let ty = validate_condition(
                &check.condition,
                &captures,
                &guaranteed,
                language,
                rule,
                &rule.span,
            )?;
            if !matches!(ty, ConditionType::Bool(_) | ConditionType::Absent) {
                return Err(Diagnostic::error(
                    "yl.condition_type",
                    "when requires a boolean or optional boolean",
                    rule.span.clone(),
                ));
            }
        }
    }
    for (owner, groups) in &language.query_meanings {
        let placeholders = language.meanings.get(owner).ok_or_else(|| {
            Diagnostic::error(
                "yl.artifact",
                "Query meanings have an unknown occurrence owner",
                Span::default(),
            )
        })?;
        let mut used = BTreeSet::new();
        for group in groups {
            let mut matching = placeholders
                .groups
                .iter()
                .enumerate()
                .filter(|(_, placeholder)| {
                    placeholder.name == group.name && placeholder.span == group.span
                });
            let Some((index, placeholder)) = matching.next() else {
                return Err(Diagnostic::error(
                    "yl.artifact",
                    "Query meaning group has no scheduling placeholder",
                    group.span.clone(),
                ));
            };
            if matching.next().is_some()
                || !used.insert(index)
                || !placeholder.calls.is_empty()
                || placeholder.matching.is_some()
            {
                return Err(Diagnostic::error(
                    "yl.artifact",
                    "Query meaning group has an invalid or ambiguous scheduling placeholder",
                    group.span.clone(),
                ));
            }
        }
    }
    for id in &language.trivia {
        if !language.rules.get(id).is_some_and(|r| r.trivia) {
            return Err(Diagnostic::error(
                "yl.artifact",
                "Invalid trivia rule",
                Span::default(),
            ));
        }
    }
    for (id, definition) in &language.examples {
        if language.rules.contains_key(id) && definition.term != Term::Ref(id.clone()) {
            return Err(Diagnostic::error(
                "yl.artifact",
                "Node example must reference its actual node rule",
                definition.span.clone(),
            ));
        }
        term(&definition.term, language, &definition.span, 0)?;
        validate_projections(&definition.term, &mut BTreeSet::new(), &definition.span)?;
        captures(&definition.term, &definition.span)?;
    }
    crate::examples::validate_explicit(language)?;
    for (owner, meanings) in &language.meanings {
        if owner.is_empty() {
            return Err(Diagnostic::error(
                "yl.artifact",
                "Empty meaning definition identity",
                Span::default(),
            ));
        }
        let mut groups = BTreeSet::new();
        for group in &meanings.groups {
            if group
                .name
                .as_ref()
                .is_some_and(|name| !groups.insert(name.clone()))
                || group.name.as_ref().is_some_and(String::is_empty)
            {
                return Err(Diagnostic::error(
                    "yl.artifact",
                    "Duplicate or empty meaning group",
                    group.span.clone(),
                ));
            }
            if let Some(matching) = &group.matching {
                if matching.inputs.is_empty()
                    || matching.cases.is_empty()
                    || !matching.cases.last().is_some_and(|case| {
                        case.alternatives.iter().any(|alternative| {
                            alternative.len() == matching.inputs.len()
                                && alternative.iter().all(Option::is_none)
                        })
                    })
                {
                    return Err(Diagnostic::error(
                        "yl.artifact",
                        "Meaning match needs inputs and a final wildcard",
                        matching.span.clone(),
                    ));
                }
                if matching.inputs.iter().any(|value| matches!(value, MeaningValue::Capture(_)) || matches!(value, MeaningValue::Call(call) if call.contract.provides_context)) {
                    return Err(Diagnostic::error("yl.artifact", "Invalid meaning match input", matching.span.clone()));
                }
                for (index, case) in matching.cases.iter().enumerate() {
                    if case.alternatives.is_empty()
                        || case.alternatives.iter().any(|alternative| {
                            alternative.len() != matching.inputs.len()
                                || alternative.iter().flatten().any(|value| {
                                    !matches!(
                                        value,
                                        MeaningValue::Text(_)
                                            | MeaningValue::Bool(_)
                                            | MeaningValue::Symbol(_)
                                    )
                                })
                                || (index + 1 != matching.cases.len()
                                    && alternative.iter().all(Option::is_none))
                        })
                    {
                        return Err(Diagnostic::error(
                            "yl.artifact",
                            "Invalid meaning match patterns",
                            case.span.clone(),
                        ));
                    }
                    for action in &case.actions {
                        if let MeaningAction::Call(call) = action {
                            if call.contract.provides_context {
                                return Err(Diagnostic::error(
                                    "yl.artifact",
                                    "Deferred action provides a context",
                                    call.span.clone(),
                                ));
                            }
                        }
                    }
                }
            }
            for call in group.all_calls() {
                reject_retired_standard_operation(&call.operation, &call.span)?;
                if (call.contract.finalization_only || call.contract.contribution)
                    && call.contract.provides_context
                {
                    return Err(Diagnostic::error(
                        "yl.artifact",
                        "Finalization-only operations and contributions cannot provide lifecycle contexts",
                        call.span.clone(),
                    ));
                }
                if call.operation.is_empty() || call.operation != call.contract.id {
                    return Err(Diagnostic::error(
                        "yl.artifact",
                        "Meaning operation identity does not match its contract",
                        call.span.clone(),
                    ));
                }
                if call.arguments.values().any(semantic_has_context_query)
                    || (call.contract.provides_context
                        && call.arguments.values().any(semantic_has_query))
                {
                    return Err(Diagnostic::error(
                        "yl.artifact",
                        "Deferred calls cannot provide lifecycle contexts",
                        call.span.clone(),
                    ));
                }
                let mut parameters = BTreeSet::new();
                for parameter in &call.contract.parameters {
                    if parameter.name.is_empty() || !parameters.insert(parameter.name.clone()) {
                        return Err(Diagnostic::error(
                            "yl.artifact",
                            "Invalid operation parameter names",
                            call.span.clone(),
                        ));
                    }
                    if parameter.default.as_ref().is_some_and(|value| {
                        semantic_has_query(value) || !meaning_value_matches(value, &parameter.kind)
                    }) {
                        return Err(Diagnostic::error(
                            "yl.artifact",
                            "Operation parameter default has an incompatible type",
                            call.span.clone(),
                        ));
                    }
                    if !call
                        .arguments
                        .get(&parameter.name)
                        .is_some_and(|value| meaning_value_matches(value, &parameter.kind))
                    {
                        return Err(Diagnostic::error(
                            "yl.artifact",
                            "Missing or incompatible meaning argument",
                            call.span.clone(),
                        ));
                    }
                }
                if call.arguments.len() != parameters.len() {
                    return Err(Diagnostic::error(
                        "yl.artifact",
                        "Unknown meaning argument",
                        call.span.clone(),
                    ));
                }
            }
        }
        for chain in &meanings.precedence {
            if chain.selectors.len() < 2 {
                return Err(Diagnostic::error(
                    "yl.artifact",
                    "Meaning precedence requires at least two selectors",
                    chain.span.clone(),
                ));
            }
            for selector in &chain.selectors {
                let target = language.meanings.get(&selector.definition);
                if selector.definition != "*"
                    && !language.rules.contains_key(&selector.definition)
                    && target.is_none()
                {
                    return Err(Diagnostic::error(
                        "yl.artifact",
                        "Unknown meaning precedence definition",
                        chain.span.clone(),
                    ));
                }
                if selector.group.as_ref().is_some_and(|name| {
                    !target.is_some_and(|definition| {
                        definition
                            .groups
                            .iter()
                            .any(|group| group.name.as_ref() == Some(name))
                    })
                }) {
                    return Err(Diagnostic::error(
                        "yl.artifact",
                        "Unknown selected meaning group",
                        chain.span.clone(),
                    ));
                }
            }
        }
    }
    Ok(())
}

fn semantic_has_query(value: &MeaningValue) -> bool {
    match value {
        MeaningValue::Call(_) => true,
        MeaningValue::Tuple(values) | MeaningValue::List(values) => {
            values.iter().any(semantic_has_query)
        }
        _ => false,
    }
}
fn semantic_has_context_query(value: &MeaningValue) -> bool {
    match value {
        MeaningValue::Call(call) => {
            call.contract.provides_context
                || call.arguments.values().any(semantic_has_context_query)
        }
        MeaningValue::Tuple(values) | MeaningValue::List(values) => {
            values.iter().any(semantic_has_context_query)
        }
        _ => false,
    }
}
fn semantic_value_is_well_formed(value: &MeaningValue) -> bool {
    match value {
        MeaningValue::Capture(name) | MeaningValue::Symbol(name) => !name.is_empty(),
        MeaningValue::Tuple(values) | MeaningValue::List(values) => {
            values.iter().all(semantic_value_is_well_formed)
        }
        _ => true,
    }
}
fn meaning_value_matches(value: &MeaningValue, kind: &ParameterKind) -> bool {
    match (value, kind) {
        (_, ParameterKind::Value) => semantic_value_is_well_formed(value),
        (MeaningValue::Call(_), _) => true,
        (MeaningValue::SelfValue, ParameterKind::Capture) => true,
        (MeaningValue::Capture(name), ParameterKind::Capture)
        | (MeaningValue::Symbol(name), ParameterKind::Symbol) => !name.is_empty(),
        (MeaningValue::Text(_), ParameterKind::Text)
        | (MeaningValue::Bool(_), ParameterKind::Bool) => true,
        _ => false,
    }
}

fn symbol_has_capture(symbol: &Symbol, name: &str) -> bool {
    let grammar = match &symbol.declaration.kind {
        DeclKind::Node { grammar, .. } => grammar.as_ref(),
        DeclKind::Pattern { grammar, .. } => Some(grammar),
        _ => None,
    };
    grammar.is_some_and(|grammar| syntax_has_capture(grammar, name))
}
fn syntax_has_capture(expr: &Expr, name: &str) -> bool {
    match &expr.kind {
        ExprKind::Capture(capture, inner) => capture == name || syntax_has_capture(inner, name),
        ExprKind::Group(inner) | ExprKind::Repeat(inner, _) | ExprKind::Pipe(inner, _, _) => {
            syntax_has_capture(inner, name)
        }
        ExprKind::Sequence(values) | ExprKind::Choice(values) => {
            values.iter().any(|value| syntax_has_capture(value, name))
        }
        _ => false,
    }
}
fn validate_semantic_capture_value(
    value: &MeaningValue,
    captures: &BTreeSet<String>,
    span: &Span,
) -> Result<()> {
    match value {
        MeaningValue::Capture(name)
            if !captures.contains(name.split('.').next().unwrap_or(name)) =>
        {
            return Err(Diagnostic::error(
                "yl.artifact",
                format!("Meaning argument references unavailable capture {name}"),
                span.clone(),
            ))
        }
        MeaningValue::Call(call) => {
            for value in call.arguments.values() {
                validate_semantic_capture_value(value, captures, &call.span)?;
            }
        }
        MeaningValue::Tuple(values) | MeaningValue::List(values) => {
            for value in values {
                validate_semantic_capture_value(value, captures, span)?;
            }
        }
        _ => {}
    }
    Ok(())
}
fn validate_meaning_captures(
    meanings: &MeaningDefinition,
    captures: &BTreeSet<String>,
) -> Result<()> {
    for call in meanings.groups.iter().flat_map(MeaningGroup::all_calls) {
        for value in call.arguments.values() {
            validate_semantic_capture_value(value, captures, &call.span)?;
        }
    }

    for group in &meanings.groups {
        if let Some(matching) = &group.matching {
            for case in &matching.cases {
                for action in &case.actions {
                    if let MeaningAction::Emit { message, span, .. } = action {
                        for part in message {
                            if let MeaningMessagePart::Capture(name) = part {
                                if !captures.contains(name) {
                                    return Err(Diagnostic::error(
                                        "yl.artifact",
                                        format!("Diagnostic references unavailable capture {name}"),
                                        span.clone(),
                                    ));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq)]
enum ConditionType {
    Bool(bool),
    Text,
    Enum(String),
    Absent,
}
fn condition_presence(condition: &Condition, truth: bool) -> BTreeSet<String> {
    match condition {
        Condition::Present(name) if truth => BTreeSet::from([name.clone()]),
        Condition::Not(inner) => condition_presence(inner, !truth),
        Condition::And(a, b) if truth => {
            let mut set = condition_presence(a, true);
            set.extend(condition_presence(b, true));
            set
        }
        Condition::Or(a, b) if !truth => {
            let mut set = condition_presence(a, false);
            set.extend(condition_presence(b, false));
            set
        }
        Condition::Equal(a, b) if !truth => match (a.as_ref(), b.as_ref()) {
            (Condition::CaptureValue(name), Condition::Absent)
            | (Condition::Absent, Condition::CaptureValue(name)) => BTreeSet::from([name.clone()]),
            _ => BTreeSet::new(),
        },
        Condition::OptionalMatches { capture, .. } if truth => BTreeSet::from([capture.clone()]),
        _ => BTreeSet::new(),
    }
}
fn validate_condition(
    condition: &Condition,
    captures: &BTreeSet<String>,
    guaranteed: &BTreeSet<String>,
    language: &CompiledLanguage,
    rule: &Rule,
    span: &Span,
) -> Result<ConditionType> {
    let bad = |message: &str| Diagnostic::error("yl.condition_type", message, span.clone());
    let capture = |name: &String, required: bool| -> Result<()> {
        if !captures.contains(name) {
            return Err(Diagnostic::error(
                "yl.unknown_capture",
                format!("Constraint references unknown capture {name}"),
                span.clone(),
            ));
        }
        if required && !guaranteed.contains(name) {
            return Err(Diagnostic::error(
                "yl.optional_capture",
                format!("Capture {name} may be absent; use ?. or an isPresent guard"),
                span.clone(),
            ));
        }
        Ok(())
    };
    let validate =
        |c: &Condition| validate_condition(c, captures, guaranteed, language, rule, span);
    Ok(match condition {
        Condition::Always | Condition::Bool(_) => ConditionType::Bool(false),
        Condition::Text(_) => ConditionType::Text,
        Condition::Absent => ConditionType::Absent,
        Condition::EnumValue { ty, variant } => {
            if ty.is_empty() || variant.is_empty() {
                return Err(bad("Invalid normalized enum value"));
            }
            ConditionType::Enum(ty.clone())
        }
        Condition::CaptureValue(name) => {
            capture(name, false)?;
            let terms = capture_terms(rule, language, name, &mut BTreeSet::new());
            if terms.iter().any(|t| !string_term(t)) {
                return Err(bad("Equality supports scalar strings, booleans and enum variants; node/list values are not comparable"));
            }
            ConditionType::Text
        }
        Condition::CaptureType {
            capture: name,
            rule: expected,
        } => {
            capture(name, true)?;
            let terms = capture_terms(rule, language, name, &mut BTreeSet::new());
            if terms.is_empty()
                || terms
                    .iter()
                    .any(|t| !node_term(t, expected, language, &mut BTreeSet::new()))
            {
                return Err(Diagnostic::error(
                    "yl.invalid_typed_argument",
                    "Constraint capture does not produce the required node type",
                    span.clone(),
                ));
            }
            ConditionType::Bool(false)
        }
        Condition::Present(name) => {
            capture(name, false)?;
            ConditionType::Bool(false)
        }
        Condition::Matches {
            capture: name,
            regex,
        }
        | Condition::OptionalMatches {
            capture: name,
            regex,
        } => {
            let optional = matches!(condition, Condition::OptionalMatches { .. });
            capture(name, !optional)?;
            regex::Regex::new(regex)
                .map_err(|e| Diagnostic::error("yl.invalid_regex", e.to_string(), span.clone()))?;
            ConditionType::Bool(optional && !guaranteed.contains(name))
        }
        Condition::Between {
            trivia,
            left,
            right,
        } => {
            capture(left, true)?;
            capture(right, true)?;
            if let Some(id) = trivia {
                if !language.rules.get(id).is_some_and(|r| r.trivia) {
                    return Err(bad("between owner is not declared trivia"));
                }
            }
            ConditionType::Bool(false)
        }
        Condition::Not(inner) => {
            let t = validate(inner)?;
            if !matches!(t, ConditionType::Bool(_) | ConditionType::Absent) {
                return Err(bad("! requires a boolean or optional boolean"));
            }
            t
        }
        Condition::And(a, b) | Condition::Or(a, b) => {
            let left = validate(a)?;
            let mut known = guaranteed.clone();
            known.extend(condition_presence(
                a,
                matches!(condition, Condition::And(..)),
            ));
            let right = validate_condition(b, captures, &known, language, rule, span)?;
            let left = if left == ConditionType::Absent {
                ConditionType::Bool(true)
            } else {
                left
            };
            let right = if right == ConditionType::Absent {
                ConditionType::Bool(true)
            } else {
                right
            };
            let (ConditionType::Bool(a), ConditionType::Bool(b)) = (left, right) else {
                return Err(bad("Logical operators require boolean operands"));
            };
            ConditionType::Bool(a || b)
        }
        Condition::Equal(a, b) => {
            if let (Condition::CaptureValue(name), Condition::Absent)
            | (Condition::Absent, Condition::CaptureValue(name)) = (a.as_ref(), b.as_ref())
            {
                capture(name, false)?;
                return Ok(ConditionType::Bool(false));
            }
            let (a, b) = (validate(a)?, validate(b)?);
            let compatible = matches!(
                (&a, &b),
                (ConditionType::Absent, _)
                    | (_, ConditionType::Absent)
                    | (ConditionType::Bool(_), ConditionType::Bool(_))
            ) || a == b;
            if !compatible {
                return Err(bad("Equality operands must have matching types"));
            }
            ConditionType::Bool(false)
        }
    })
}

fn rule_capture_sets(
    rule: &Rule,
    language: &CompiledLanguage,
    visiting: &mut BTreeSet<String>,
) -> (BTreeSet<String>, BTreeSet<String>) {
    match &rule.body {
        RuleBody::Concrete(t) => capture_sets(t),
        RuleBody::Abstract { bases, operators } => {
            let mut possible = BTreeSet::new();
            let mut guaranteed: Option<BTreeSet<String>> = None;
            for id in bases.iter().chain(operators.iter().map(|o| &o.rule)) {
                if !visiting.insert(id.clone()) {
                    continue;
                }
                if let Some(rule) = language.rules.get(id) {
                    let (p, g) = rule_capture_sets(rule, language, visiting);
                    possible.extend(p);
                    guaranteed = Some(guaranteed.map_or_else(
                        || g.clone(),
                        |prev| prev.intersection(&g).cloned().collect(),
                    ));
                }
                visiting.remove(id);
            }
            (possible, guaranteed.unwrap_or_default())
        }
    }
}
fn capture_sets(term: &Term) -> (BTreeSet<String>, BTreeSet<String>) {
    match term {
        Term::Capture(name, term) => {
            let (mut possible, mut guaranteed) = capture_sets(term);
            possible.insert(name.clone());
            if !may_be_none(term) {
                guaranteed.insert(name.clone());
            }
            (possible, guaranteed)
        }
        Term::Sequence(items) => items.iter().map(capture_sets).fold(
            (BTreeSet::new(), BTreeSet::new()),
            |(mut p, mut g), (a, b)| {
                p.extend(a);
                g.extend(b);
                (p, g)
            },
        ),
        Term::Choice(items) => {
            let mut p = BTreeSet::new();
            let mut g: Option<BTreeSet<String>> = None;
            for (a, b) in items.iter().map(capture_sets) {
                p.extend(a);
                g = Some(g.map_or_else(
                    || b.clone(),
                    |prev| prev.intersection(&b).cloned().collect(),
                ));
            }
            (p, g.unwrap_or_default())
        }
        Term::Repeat(term, q) => {
            let (p, g) = capture_sets(term);
            (
                p,
                if *q == Quantifier::Plus {
                    g
                } else {
                    BTreeSet::new()
                },
            )
        }
        Term::Mark { term, .. } | Term::Meaning { term, .. } | Term::Example { term, .. } => {
            capture_sets(term)
        }
        Term::Project {
            id,
            term,
            quantifier,
            ..
        } => {
            let (p, g) = find_marker(term, *id).map(capture_sets).unwrap_or_default();
            (
                p,
                if *quantifier == Some(Quantifier::Star) {
                    BTreeSet::new()
                } else {
                    g
                },
            )
        }
        _ => Default::default(),
    }
}
fn may_be_none(term: &Term) -> bool {
    match term {
        Term::Repeat(_, Quantifier::Optional) | Term::NotAhead(_) | Term::NotBehind(_) => true,
        Term::Capture(_, term)
        | Term::Meaning { term, .. }
        | Term::Example { term, .. }
        | Term::Mark { term, .. } => may_be_none(term),
        Term::Project {
            id,
            term,
            quantifier: None,
            ..
        } => find_marker(term, *id).is_none_or(may_be_none),
        Term::Choice(items) => items.iter().any(may_be_none),
        _ => false,
    }
}

fn validate_projections(term: &Term, active: &mut BTreeSet<u32>, span: &Span) -> Result<()> {
    match term {
        Term::Project {
            id,
            term,
            quantifier,
            collect_lists,
        } => {
            if !active.insert(*id) {
                return Err(Diagnostic::error(
                    "yl.artifact",
                    "Nested projection reuses its enclosing marker",
                    span.clone(),
                ));
            }
            if !valid_cardinality(
                *quantifier,
                marker_value_cardinality(term, *id, *collect_lists),
            ) {
                return Err(Diagnostic::error(
                    "yl.artifact",
                    "Projection output violates its cardinality",
                    span.clone(),
                ));
            }
            validate_projections(term, active, span)?;
            active.remove(id);
        }
        Term::Mark { id, term } => {
            if !active.contains(id) {
                return Err(Diagnostic::error(
                    "yl.artifact",
                    "Value marker is outside its projection",
                    span.clone(),
                ));
            }
            validate_projections(term, active, span)?;
        }
        Term::Sequence(items) | Term::Choice(items) => {
            for t in items {
                validate_projections(t, active, span)?;
            }
        }
        Term::Repeat(t, _)
        | Term::Meaning { term: t, .. }
        | Term::Example { term: t, .. }
        | Term::Capture(_, t)
        | Term::NotAhead(t)
        | Term::NotBehind(t) => validate_projections(t, active, span)?,
        _ => {}
    }
    Ok(())
}

fn captures(term: &Term, span: &Span) -> Result<BTreeSet<String>> {
    let mut names = BTreeSet::new();
    match term {
        Term::Capture(name, inner) => {
            names = captures(inner, span)?;
            if !names.insert(name.clone()) {
                return Err(Diagnostic::error(
                    "yl.duplicate_member",
                    format!("Duplicate capture {name}"),
                    span.clone(),
                ));
            }
        }
        Term::Sequence(items) => {
            for item in items {
                for name in captures(item, span)? {
                    if !names.insert(name.clone()) {
                        return Err(Diagnostic::error(
                            "yl.duplicate_member",
                            format!("Duplicate capture {name}"),
                            span.clone(),
                        ));
                    }
                }
            }
        }
        Term::Choice(items) => {
            for item in items {
                names.extend(captures(item, span)?);
            }
        }
        Term::Repeat(inner, _) => names = captures(inner, span)?,
        Term::Mark { term, .. } | Term::Meaning { term, .. } | Term::Example { term, .. } => {
            names = captures(term, span)?
        }
        Term::Project { id, term, .. } => {
            if let Some(original) = find_marker(term, *id) {
                names = captures(original, span)?;
            }
        }
        _ => {}
    }
    Ok(names)
}

fn find_marker(term: &Term, marker: u32) -> Option<&Term> {
    match term {
        Term::Mark { id, term } if *id == marker => Some(term),
        Term::Sequence(items) | Term::Choice(items) => {
            items.iter().find_map(|t| find_marker(t, marker))
        }
        Term::Repeat(t, _)
        | Term::Capture(_, t)
        | Term::Meaning { term: t, .. }
        | Term::Example { term: t, .. }
        | Term::Mark { term: t, .. }
        | Term::Project { term: t, .. } => find_marker(t, marker),
        _ => None,
    }
}
/// Counts binding uses on all successful paths, without choosing a scalar value.
fn marker_value_cardinality(term: &Term, marker: u32, flatten: bool) -> (usize, Option<usize>) {
    match term {
        Term::Mark { id, term } if *id == marker => match value_shape(term) {
            Some(Quantifier::Optional) => (0, Some(1)),
            Some(Quantifier::Star) if flatten => (0, None),
            Some(Quantifier::Plus) if flatten => (1, None),
            _ => (1, Some(1)),
        },
        Term::Sequence(items) => items
            .iter()
            .map(|t| marker_value_cardinality(t, marker, flatten))
            .fold((0, Some(0)), |(min, max), (a, b)| {
                (
                    min.saturating_add(a),
                    max.zip(b).and_then(|(x, y)| x.checked_add(y)),
                )
            }),
        Term::Choice(items) => items
            .iter()
            .map(|t| marker_value_cardinality(t, marker, flatten))
            .reduce(|(a, b), (c, d)| (a.min(c), b.zip(d).map(|(b, d)| b.max(d))))
            .unwrap_or((0, Some(0))),
        Term::Repeat(t, q) => {
            let (min, max) = marker_value_cardinality(t, marker, flatten);
            match q {
                Quantifier::Optional => (0, max),
                Quantifier::Star => (0, if max == Some(0) { Some(0) } else { None }),
                Quantifier::Plus => (min, if max == Some(0) { Some(0) } else { None }),
            }
        }
        Term::Capture(_, t)
        | Term::Meaning { term: t, .. }
        | Term::Example { term: t, .. }
        | Term::Mark { term: t, .. }
        | Term::Project { term: t, .. } => marker_value_cardinality(t, marker, flatten),
        _ => (0, Some(0)),
    }
}

fn value_shape(term: &Term) -> Option<Quantifier> {
    match term {
        Term::Repeat(_, q) => Some(*q),
        Term::Capture(_, t)
        | Term::Meaning { term: t, .. }
        | Term::Example { term: t, .. }
        | Term::Mark { term: t, .. } => value_shape(t),
        Term::Project { quantifier, .. } => *quantifier,
        _ => None,
    }
}
fn valid_cardinality(quantifier: Option<Quantifier>, (min, max): (usize, Option<usize>)) -> bool {
    match quantifier {
        None => min == 1 && max == Some(1),
        Some(Quantifier::Optional) => max.is_some_and(|n| n <= 1),
        Some(Quantifier::Star) => true,
        Some(Quantifier::Plus) => min >= 1,
    }
}

fn capture_terms<'a>(
    rule: &'a Rule,
    language: &'a CompiledLanguage,
    name: &str,
    visiting: &mut BTreeSet<String>,
) -> Vec<&'a Term> {
    fn walk<'a>(term: &'a Term, name: &str) -> Vec<&'a Term> {
        match term {
            Term::Capture(n, inner) if n == name => vec![inner],
            Term::Sequence(items) | Term::Choice(items) => {
                items.iter().flat_map(|t| walk(t, name)).collect()
            }
            Term::Capture(_, inner)
            | Term::Repeat(inner, _)
            | Term::Meaning { term: inner, .. }
            | Term::Example { term: inner, .. }
            | Term::Mark { term: inner, .. } => walk(inner, name),
            Term::Project { id, term, .. } => find_marker(term, *id)
                .map(|t| walk(t, name))
                .unwrap_or_default(),
            _ => vec![],
        }
    }
    match &rule.body {
        RuleBody::Concrete(t) => walk(t, name),
        RuleBody::Abstract { bases, operators } => bases
            .iter()
            .chain(operators.iter().map(|o| &o.rule))
            .flat_map(|id| {
                if !visiting.insert(id.clone()) {
                    return vec![];
                }
                let result = language
                    .rules
                    .get(id)
                    .map(|r| capture_terms(r, language, name, visiting))
                    .unwrap_or_default();
                visiting.remove(id);
                result
            })
            .collect(),
    }
}
fn string_term(term: &Term) -> bool {
    match term {
        Term::Example { term, .. } => string_term(term),
        Term::Literal(_) | Term::Regex(_) => true,
        Term::Repeat(t, Quantifier::Optional)
        | Term::Capture(_, t)
        | Term::Mark { term: t, .. } => string_term(t),
        Term::Choice(items) => items.iter().all(string_term),
        Term::Project {
            id,
            term,
            quantifier: None | Some(Quantifier::Optional),
            ..
        } => find_marker(term, *id).is_some_and(string_term),
        _ => false,
    }
}
fn node_term(
    term: &Term,
    expected: &str,
    language: &CompiledLanguage,
    visiting: &mut BTreeSet<String>,
) -> bool {
    match term {
        Term::Ref(id) if id == expected => true,
        Term::Ref(id) => {
            if !visiting.insert(expected.into()) {
                return false;
            }
            let result = language.rules.get(expected).is_some_and(|r| match &r.body {
                RuleBody::Abstract { bases, operators } => bases
                    .iter()
                    .chain(operators.iter().map(|o| &o.rule))
                    .any(|member| node_term(&Term::Ref(id.clone()), member, language, visiting)),
                _ => false,
            });
            visiting.remove(expected);
            result
        }
        Term::Capture(_, t)
        | Term::Meaning { term: t, .. }
        | Term::Example { term: t, .. }
        | Term::Mark { term: t, .. } => node_term(t, expected, language, visiting),
        Term::Choice(items) => items
            .iter()
            .all(|t| node_term(t, expected, language, visiting)),
        Term::Project {
            id,
            term,
            quantifier: None,
            ..
        } => find_marker(term, *id).is_some_and(|t| node_term(t, expected, language, visiting)),
        _ => false,
    }
}

fn validate_case_emission(emission: &Constraint) -> Result<()> {
    let ConstraintKind::Call(name, args) = &emission.kind else {
        return Err(Diagnostic::error(
            "yl.invalid_constraint",
            "Expected diagnostic emission",
            emission.span.clone(),
        ));
    };
    let [argument] = args.as_slice() else {
        return Err(Diagnostic::error(
            "yl.invalid_argument",
            "Diagnostic takes one positional string",
            emission.span.clone(),
        ));
    };
    if argument.name.is_some() || !matches!(argument.value.kind, ExprKind::Literal(_)) {
        return Err(Diagnostic::error(
            "yl.invalid_argument",
            "Diagnostic takes one positional string",
            argument.span.clone(),
        ));
    }
    if !matches!(name.as_str(), "error" | "warning" | "help") {
        return Err(Diagnostic::error(
            "yl.invalid_constraint",
            "Unknown diagnostic emission",
            emission.span.clone(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod semantic_artifact_tests {
    use super::*;
    use crate::semantics::{MeaningCall, MeaningGroup, OperationParameter, OperationSignature};

    fn artifact() -> CompiledLanguage {
        let sources = BTreeMap::from([(
            "test.yl".into(),
            "node Program = name: /[a-z]+/ entry Program".into(),
        )]);
        let mut language = compile_sources("test.yl", &sources).unwrap();
        language.version = 2;
        let span = Span::new("test.yl", 0, 1);
        language.meanings.insert(
            "test.yl#Program".into(),
            MeaningDefinition {
                boundary: false,
                groups: vec![MeaningGroup {
                    matching: None,
                    name: Some("Register".into()),
                    span: span.clone(),
                    calls: vec![MeaningCall {
                        operation: "native#register".into(),
                        contract: OperationSignature {
                            id: "native#register".into(),
                            finalization_only: false,
                            contribution: false,
                            provides_context: false,
                            parameters: vec![OperationParameter {
                                name: "name".into(),
                                kind: ParameterKind::Capture,
                                default: None,
                            }],
                        },
                        arguments: BTreeMap::from([(
                            "name".into(),
                            MeaningValue::Capture("name".into()),
                        )]),
                        span,
                    }],
                }],
                precedence: vec![],
            },
        );
        language
    }
    #[test]
    fn semantic_contracts_roundtrip_and_reject_mismatched_artifacts() {
        let mut language = artifact();
        crate::load_compiled_language(&language.to_bytes().unwrap()).unwrap();
        language.meanings.get_mut("test.yl#Program").unwrap().groups[0].calls[0]
            .contract
            .id = "other".into();
        assert_eq!(
            crate::load_compiled_language(&language.to_bytes().unwrap()).unwrap_err()[0].code,
            "yl.artifact"
        );
    }
    #[test]
    fn artifacts_reject_retired_native_standard_operations_explicitly() {
        for id in [
            "std/variables#declare",
            "std/references#use",
            "std/types#annotate",
        ] {
            let mut language = artifact();
            let call =
                &mut language.meanings.get_mut("test.yl#Program").unwrap().groups[0].calls[0];
            call.operation = id.into();
            call.contract.id = id.into();
            let bytes = serde_json::to_vec(&language).unwrap();
            let error = crate::load_compiled_language(&bytes).unwrap_err();
            assert_eq!(error[0].code, "yl.artifact_retired_std");
            assert!(error[0].message.contains("recompile"));
            let analyzed =
                crate::semantics::SemanticEngine::default().analyze(&language, "input", "name");
            assert_eq!(analyzed.diagnostics[0].code, "yl.artifact_retired_std");
            assert!(analyzed.ast.is_none());
        }
    }

    #[test]
    fn semantic_artifacts_reject_invalid_arguments_and_missing_captures() {
        let mut language = artifact();
        language.meanings.get_mut("test.yl#Program").unwrap().groups[0].calls[0]
            .arguments
            .insert("name".into(), MeaningValue::Capture("missing".into()));
        assert!(
            crate::load_compiled_language(&language.to_bytes().unwrap()).unwrap_err()[0]
                .message
                .contains("unavailable capture")
        );
        language.meanings.get_mut("test.yl#Program").unwrap().groups[0].calls[0]
            .arguments
            .insert("name".into(), MeaningValue::Bool(true));
        assert!(crate::load_compiled_language(&language.to_bytes().unwrap()).is_err());
    }
    #[test]
    fn semantic_artifacts_reject_nested_missing_captures_and_illegal_lifecycles() {
        let mut language = artifact();
        let call = &mut language.meanings.get_mut("test.yl#Program").unwrap().groups[0].calls[0];
        call.contract.parameters[0].kind = ParameterKind::Value;
        call.arguments.insert(
            "name".into(),
            MeaningValue::Tuple(vec![MeaningValue::List(vec![MeaningValue::Capture(
                "missing".into(),
            )])]),
        );
        assert!(
            crate::load_compiled_language(&language.to_bytes().unwrap()).unwrap_err()[0]
                .message
                .contains("unavailable capture")
        );
        let call = &mut language.meanings.get_mut("test.yl#Program").unwrap().groups[0].calls[0];
        call.arguments
            .insert("name".into(), MeaningValue::Symbol(String::new()));
        assert!(crate::load_compiled_language(&language.to_bytes().unwrap()).is_err());
        let call = &mut language.meanings.get_mut("test.yl#Program").unwrap().groups[0].calls[0];
        call.arguments
            .insert("name".into(), MeaningValue::Capture("name".into()));
        call.contract.finalization_only = true;
        call.contract.provides_context = true;
        assert!(
            crate::load_compiled_language(&language.to_bytes().unwrap()).unwrap_err()[0]
                .message
                .contains("Finalization-only")
        );
    }
}

#[cfg(test)]
mod declarative_query_frontend_tests {
    use super::*;
    use crate::semantics::queries::{QueryAction, QueryExpr};
    fn compile(source: &str) -> CompileResult<CompiledLanguage> {
        let mut registry = OperationRegistry::new();
        crate::semantics::queries::register(&mut registry).unwrap();
        compile_sources_with_registry(
            "query.yl",
            &BTreeMap::from([("query.yl".into(), source.into())]),
            &registry,
        )
    }
    #[test]
    fn query_wrappers_expand_to_normalized_callbacks_and_conditionals() {
        let source = r#"import { get, contribute, ancestors, find, isPresent, text } from "std/query"
symbol scope
symbol declarations
operation resolve(name) = find(ancestors(self), ancestor => get((get((ancestor, scope)), declarations, text(name))))
operation declare(name) {
    if isPresent(name) { contribute((self, declarations, text(name)), self) }
}
node Program = name: /[a-z]+/ {
    meanings {
        group Register { declare(name) }
        if !isPresent(resolve(name)) { error("Missing ${name}") }
    }
}
entry Program
"#;
        let language = compile(source).unwrap();
        let groups = &language.query_meanings["query.yl#Program"];
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].name.as_deref(), Some("Register"));
        let QueryAction::If {
            condition: QueryExpr::Not(value),
            ..
        } = &groups[1].actions[0]
        else {
            panic!("expected if");
        };
        let QueryExpr::Call { name, args } = &**value else {
            panic!("expected isPresent");
        };
        assert_eq!(name, "std/query#isPresent");
        let QueryExpr::Call { args, .. } = &args[0] else {
            panic!("expected find");
        };
        let QueryExpr::Lambda { parameter, body } = &args[1] else {
            panic!("expected callback");
        };
        assert!(parameter.starts_with("ancestor@"));
        assert!(serde_json::to_string(body).unwrap().contains("Local"));
        let bytes = language.to_bytes().unwrap();
        let loaded = crate::load_compiled_language(&bytes).unwrap();
        assert_eq!(language.query_meanings, loaded.query_meanings);
    }
    #[test]
    fn pure_query_helpers_and_nested_callbacks_keep_lexical_bindings() {
        let language = compile(r#"import { contribute, find, ancestors, get } from "std/query"
operation key(value) = (value, "tag")
operation select(name) = find(ancestors(self), outer => find(ancestors(outer), inner => get((outer, inner, name))))
node Program = name: /[a-z]+/ { meanings { contribute(key(self), select(name)) } }
entry Program"#).unwrap();
        let QueryAction::Call { args, .. } =
            &language.query_meanings["query.yl#Program"][0].actions[0]
        else {
            panic!("expected contribution");
        };
        assert_eq!(
            args[0],
            QueryExpr::Tuple(vec![QueryExpr::SelfValue, QueryExpr::Text("tag".into())])
        );
        let QueryExpr::Call { args, .. } = &args[1] else {
            panic!("expected find");
        };
        let QueryExpr::Lambda {
            parameter: outer,
            body,
        } = &args[1]
        else {
            panic!("expected outer callback");
        };
        let QueryExpr::Call { args, .. } = &**body else {
            panic!("expected nested find");
        };
        let QueryExpr::Lambda {
            parameter: inner,
            body,
        } = &args[1]
        else {
            panic!("expected inner callback");
        };
        let QueryExpr::Call { args, .. } = &**body else {
            panic!("expected get");
        };
        assert_eq!(
            args[0],
            QueryExpr::Tuple(vec![
                QueryExpr::Local(outer.clone()),
                QueryExpr::Local(inner.clone()),
                QueryExpr::Capture("name".into())
            ])
        );
    }
    #[test]
    fn diagnostics_interpolate_bound_values_and_nested_queries() {
        use crate::semantics::queries::QueryMessagePart as P;
        let language = compile(r#"import { get } from "std/query"
operation require(actual) { if !actual { error("Bad ${actual}; attachment ${get((self, \"}tag\"))}") } }
node Program = "x" { meanings { require(false) } }
entry Program"#).unwrap();
        let QueryAction::If { then_actions, .. } =
            &language.query_meanings["query.yl#Program"][0].actions[0]
        else {
            panic!("expected if");
        };
        let QueryAction::Emit { message, .. } = &then_actions[0] else {
            panic!("expected diagnostic");
        };
        assert_eq!(message[1], P::Value(QueryExpr::Bool(false)));
        assert_eq!(
            message[3],
            P::Value(QueryExpr::Call {
                name: "std/query#get".into(),
                args: vec![QueryExpr::Tuple(vec![
                    QueryExpr::SelfValue,
                    QueryExpr::Text("}tag".into())
                ])]
            })
        );
    }
    #[test]
    fn malformed_diagnostic_expressions_are_structured_errors() {
        for message in ["${get(}", "${}", "${self self}"] {
            let source = format!(
                r#"node Program = "x" {{ meanings {{ if true {{ error("{message}") }} }} }} entry Program"#
            );
            let errors = compile(&source).unwrap_err();
            assert!(
                errors
                    .iter()
                    .any(|error| error.code == "yl.invalid_diagnostic"),
                "{errors:?}"
            );
        }
    }
    #[test]
    fn named_query_arguments_preserve_contract_order() {
        let language = compile(
            r#"import { contribute } from "std/query"
node Program = "x" { meanings { contribute(value = "result", key = self) } }
entry Program"#,
        )
        .unwrap();
        let QueryAction::Call { args, .. } =
            &language.query_meanings["query.yl#Program"][0].actions[0]
        else {
            panic!("expected call");
        };
        assert_eq!(
            args,
            &[QueryExpr::SelfValue, QueryExpr::Text("result".into())]
        );
    }
    #[test]
    fn excess_and_duplicate_query_arguments_are_diagnostics() {
        for call in [
            "contribute(self)",
            "contribute(self, self, self)",
            "contribute(key = self, key = self, value = self)",
        ] {
            let source = format!("import {{ contribute }} from \"std/query\" node Program = \"x\" {{ meanings {{ {call} }} }} entry Program");
            let errors = compile(&source).unwrap_err();
            assert!(
                errors
                    .iter()
                    .any(|error| error.code == "yl.invalid_argument"),
                "{errors:?}"
            );
        }
    }
}

#[cfg(test)]
mod authored_standard_module_tests {
    use super::*;
    use crate::semantics::SemanticEngine;

    const LANGUAGE: &str = r#"
        import * as references from "std/references"
        import { declare, use } from "std/variables"
        import { unknown } from "std/types"
        import { contribute } from "std/query"
        trivia Space = /\s+/
        node Declaration = "let" name: /[a-z]+/ ";" {
            meanings { declare(name) }
        }
        node Reference = name: /[a-z]+/ ";" {
            meanings { use(name) }
        }
        node Program = (Declaration | Reference)* {
            meanings {
                references::lexicalScope()
                contribute((self, unknown), unknown)
            }
        }
        entry Program { trivia Space }
    "#;

    fn assert_authored(language: &CompiledLanguage) {
        assert!(!language.query_meanings.is_empty());
        assert!(language.meanings.values().all(|definition| definition
            .groups
            .iter()
            .all(|group| group.calls.is_empty() && group.matching.is_none())));
        let json = serde_json::to_string(language).unwrap();
        assert!(json.contains("std/types#unknown"));
        assert!(json.contains("std/references#scope"));
        assert!(!json.contains("std/variables#"));
        assert!(!json.contains("std/references#declare"));
        assert!(!json.contains("std/references#lexicalScope"));
        let engine = SemanticEngine::default();
        let reloaded = engine.load(&language.to_bytes().unwrap()).unwrap();
        assert!(engine
            .analyze(&reloaded, "program", "let x; x;")
            .diagnostics
            .is_empty());
        let errors = engine.analyze(&reloaded, "program", "missing;");
        assert!(errors
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("Unknown name 'missing'")));
    }

    #[test]
    fn canonical_authored_imports_expand_and_reload_without_native_adapters() {
        let sources = BTreeMap::from([
            ("nested/language.yl".into(), LANGUAGE.into()),
            // Caller-supplied source maps cannot replace reserved definitions.
            ("std/types".into(), "export symbol wrong".into()),
            ("std/references".into(), "export symbol wrong".into()),
            ("std/variables".into(), "export symbol wrong".into()),
        ]);
        assert_authored(&compile_sources("nested/language.yl", &sources).unwrap());
    }

    #[test]
    fn authored_operation_arguments_keep_caller_capture_identity() {
        let source = r#"import * as types from "std/types"
            symbol Red
            node Root = first: /x/ { meanings {
                types::annotate(first, Red)
                types::restrictTo(first, Red)
            } }
            entry Root"#;
        let engine = SemanticEngine::default();
        let language = compile_sources(
            "language.yl",
            &BTreeMap::from([("language.yl".into(), source.into())]),
        )
        .unwrap();
        let loaded = engine.load(&language.to_bytes().unwrap()).unwrap();
        assert!(engine.analyze(&loaded, "input", "x").diagnostics.is_empty());
    }

    #[test]
    fn nested_wrappers_preserve_caller_captures_and_member_projections() {
        let source = r#"import * as types from "std/types"
            import { apply, read } from "./helpers"
            import { equal } from "std/query"
            symbol Red
            node Container = first: /x/
            node Root = box: Container { meanings {
                apply(box, Red)
                if !equal(read(box), Red) { error("Wrong caller capture") }
            } }
            entry Root"#;
        let helper = r#"import * as types from "std/types"
            operation nested(value, annotation) { types::annotate(value.first, annotation) }
            export operation apply(value, annotation) { nested(value, annotation) }
            export operation read(value) = types::typeof(value.first)"#;
        let engine = SemanticEngine::default();
        let language = compile_sources(
            "language.yl",
            &BTreeMap::from([
                ("language.yl".into(), source.into()),
                ("helpers.yl".into(), helper.into()),
            ]),
        )
        .unwrap();
        assert!(engine
            .analyze(&language, "input", "x")
            .diagnostics
            .is_empty());
    }

    #[test]
    fn query_artifacts_reject_missing_and_ambiguous_scheduling_groups() {
        let source = r#"import { contribute } from "std/query"
            node Root = "x" { meanings { contribute("value", true) } } entry Root"#;
        let language = compile_sources(
            "language.yl",
            &BTreeMap::from([("language.yl".into(), source.into())]),
        )
        .unwrap();
        for mutation in 0..3 {
            let mut broken = language.clone();
            let groups = &mut broken.meanings.get_mut("language.yl#Root").unwrap().groups;
            match mutation {
                0 => groups.clear(),
                1 => groups[0].span.start += 1,
                _ => groups.push(groups[0].clone()),
            }
            let errors =
                crate::load_compiled_language(&serde_json::to_vec(&broken).unwrap()).unwrap_err();
            assert_eq!(errors[0].code, "yl.artifact");
            assert!(errors[0].message.contains("scheduling placeholder"));
        }
    }

    #[test]
    fn filesystem_imports_use_the_same_canonical_modules() {
        let directory = std::env::temp_dir().join(format!(
            "yl-authored-std-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&directory).unwrap();
        let entry = directory.join("language.yl");
        std::fs::write(&entry, LANGUAGE).unwrap();
        let compiled = compile_language(&entry);
        std::fs::remove_file(&entry).unwrap();
        std::fs::remove_dir(&directory).unwrap();
        assert_authored(&compiled.unwrap());
    }

    #[test]
    fn supplied_native_registry_cannot_replace_authored_standard_modules() {
        for module in ["std/references", "std/variables", "std/types"] {
            let mut registry = OperationRegistry::new();
            registry.export_symbol(module, "replacement").unwrap();
            let errors = compile_sources_with_registry(
                "language.yl",
                &BTreeMap::from([("language.yl".into(), LANGUAGE.into())]),
                &registry,
            )
            .unwrap_err();
            assert_eq!(errors[0].code, "yl.reserved_module");
            assert!(errors[0].message.contains(module));
        }
    }

    #[test]
    fn default_engine_registers_only_generic_query_primitives() {
        let engine = SemanticEngine::default();
        assert!(engine.registry().contains_module("std/query"));
        assert!(!engine.registry().contains_module("std/references"));
        assert!(!engine.registry().contains_module("std/variables"));
        assert!(!engine.registry().contains_module("std/types"));
        assert!(!engine.registry().contains_module("std/syntax"));
    }
}
