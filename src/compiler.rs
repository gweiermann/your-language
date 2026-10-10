//! Module graph collection precedes resolution. Expansion never reaches the runtime.
use crate::{
    diagnostic::{CompileResult, Diagnostic, Severity, Span},
    frontend::parse_yl,
    ir::*,
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
    modules: BTreeMap<String, Module>,
    dependencies: BTreeMap<(String, String), String>,
    imports: BTreeMap<(String, String), String>,
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
    let mut compiler = Compiler::default();
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
fn import_path(module: &str, path: &str) -> String {
    if path == "std/parser" || path == "core/parser" {
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
        let source = if id == "std/parser" {
            include_str!("../stdlib/parser.yl")
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
                if let DeclKind::Import { path, .. } = &d.kind {
                    Some((path.clone(), import_path(id, path), d.span.clone()))
                } else {
                    None
                }
            })
            .collect();
        self.modules.insert(id.into(), module);
        for (specifier, dependency, span) in dependencies {
            self.dependencies
                .insert((id.into(), specifier), dependency.clone());
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
                if let DeclKind::Import { path, .. } = &d.kind {
                    Some((path.clone(), d.span.clone()))
                } else {
                    None
                }
            })
            .collect();
        self.modules.insert(id.clone(), module);
        for (dependency, span) in dependencies {
            if dependency == "core/parser" {
                self.dependencies
                    .insert((id.clone(), dependency.clone()), dependency);
                continue;
            }
            if dependency == "std/parser" {
                self.dependencies
                    .insert((id.clone(), dependency.clone()), dependency);
                self.read_sources("std/parser", &BTreeMap::new())?;
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
            DeclKind::Node { name, .. }
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
                        ));
                    }
                }
            }
        }
        // Root imports first; qualified imports can depend on those memberships.
        pending.sort_by_key(|(_, _, name, _, _)| name.matches("::").count());
        for (module, from, name, alias, span) in pending {
            let id = if from == "core/parser" {
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
            if self.symbols.contains_key(&format!("{module}#{alias}"))
                || self.imports.insert((module, alias), id).is_some()
            {
                return Err(Diagnostic::error(
                    "yl.duplicate_declaration",
                    "Import conflicts with a local/imported declaration",
                    span,
                ));
            }
        }
        self.validate_declarations()?;
        self.validate_pipe_matches()?;
        let mut entries = vec![];
        for (module, ast) in &self.modules {
            for d in &ast.declarations {
                if let DeclKind::Entry(name) = &d.kind {
                    entries.push(self.resolve(module, "", name, &d.span)?);
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
        let mut language = CompiledLanguage {
            version: 1,
            entry: entries.remove(0),
            rules: BTreeMap::new(),
            trivia: vec![],
            diagnostics: vec![],
        };
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
                if *trivia {
                    language.trivia.push(id);
                }
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
        language.diagnostics = std::mem::take(&mut self.diagnostics);
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
            ExprKind::EnumConstant(_, _)
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
        result
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
            Term::Capture(_, t) | Term::Mark { term: t, .. } => self.accepts(t, ty, visiting),
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
        Term::Capture(_, term) | Term::Mark { term, .. } | Term::Project { term, .. } => {
            edge_term(term, right)
        }
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
        Term::Capture(_, term) | Term::Mark { term, .. } | Term::Project { term, .. } => {
            zero_width(term)
        }
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

pub(crate) fn validate_language(language: &CompiledLanguage) -> Result<()> {
    if language.version != 1 || !language.rules.contains_key(&language.entry) {
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
    for id in &language.trivia {
        if !language.rules.get(id).is_some_and(|r| r.trivia) {
            return Err(Diagnostic::error(
                "yl.artifact",
                "Invalid trivia rule",
                Span::default(),
            ));
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
        Term::Mark { term, .. } => capture_sets(term),
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
        Term::Capture(_, term) | Term::Mark { term, .. } => may_be_none(term),
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
        Term::Repeat(t, _) | Term::Capture(_, t) | Term::NotAhead(t) | Term::NotBehind(t) => {
            validate_projections(t, active, span)?
        }
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
        Term::Mark { term, .. } => names = captures(term, span)?,
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
        Term::Capture(_, t) | Term::Mark { term: t, .. } | Term::Project { term: t, .. } => {
            marker_value_cardinality(t, marker, flatten)
        }
        _ => (0, Some(0)),
    }
}

fn value_shape(term: &Term) -> Option<Quantifier> {
    match term {
        Term::Repeat(_, q) => Some(*q),
        Term::Capture(_, t) | Term::Mark { term: t, .. } => value_shape(t),
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
            Term::Capture(_, inner) | Term::Repeat(inner, _) | Term::Mark { term: inner, .. } => {
                walk(inner, name)
            }
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
        Term::Capture(_, t) | Term::Mark { term: t, .. } => {
            node_term(t, expected, language, visiting)
        }
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
