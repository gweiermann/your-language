//! Module graph collection precedes resolution. Expansion never reaches the runtime.
use crate::{
    diagnostic::{CompileResult, Diagnostic, Severity, Span},
    frontend::parse_yl,
    ir::*,
    syntax::*,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

type Result<T> = std::result::Result<T, Diagnostic>;
type Environment = BTreeMap<String, Value>;
#[derive(Clone)]
enum Value {
    Grammar(Term),
    Enum(String, String),
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
    imports: BTreeMap<(String, String), String>,
    symbols: BTreeMap<String, Symbol>,
    exports: BTreeMap<(String, String), String>,
    members: BTreeMap<(String, String), String>,
    extensions: Vec<(String, Declaration)>,
    expanding: Vec<String>,
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
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
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
                    self.validate_expr(symbol, default, &locals)?;
                }
            }
            for grammar in grammars {
                self.validate_expr(symbol, grammar, &locals)?;
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
            ExprKind::Capture(_, inner) | ExprKind::Repeat(inner, _) => {
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
                    Some((import_path(id, path), d.span.clone()))
                } else {
                    None
                }
            })
            .collect();
        self.modules.insert(id.into(), module);
        for (dependency, span) in dependencies {
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
                continue;
            }
            if dependency == "std/parser" {
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
                            import_path(module, path),
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
                            let member = self.member_path(&id, name, &level.span, 0)?;
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
                            let items = sequence(&term);
                            let left = items
                                .first()
                                .is_some_and(|t| edge_ref(t) == Some(id.as_str()));
                            let right = items
                                .last()
                                .is_some_and(|t| edge_ref(t) == Some(id.as_str()));
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
        Ok(match &expr.kind {
            ExprKind::Literal(s) => Term::Literal(s.clone()),
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
                    return Err(Diagnostic::error("yl.parked_application", "PARKED: a grammar parameter followed by parentheses is ambiguous between application and sequence; see documentation/implementation/PARKED.md",expr.span.clone()));
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
                bound.insert(arm.binding.clone(), Value::Grammar(value));
                let body = match &arm.body {
                    RewriteBody::Direct(body) => body,
                    RewriteBody::Cases(cases) => {
                        let enums: Vec<_> = bound
                            .values()
                            .filter_map(|v| {
                                if let Value::Enum(ty, v) = v {
                                    Some((ty, v))
                                } else {
                                    None
                                }
                            })
                            .collect();
                        if enums.len() != 1 {
                            return Err(Diagnostic::error("yl.parked_enum_dispatch","PARKED: enum case dispatch requires one unambiguous enum parameter; multiple-parameter selector syntax is unspecified",arm.span.clone()));
                        }
                        let (ty, variant) = enums[0];
                        let mut chosen = None;
                        for (case, body) in cases {
                            let (case_ty, case_variant) = self.enum_value(
                                &symbol.module,
                                &symbol.scope,
                                case,
                                Some(ty),
                                &bound,
                            )?;
                            if case_ty == *ty && case_variant == *variant {
                                if chosen.is_some() {
                                    return Err(Diagnostic::error(
                                        "yl.duplicate_member",
                                        "Duplicate rewrite case",
                                        case.span.clone(),
                                    ));
                                }
                                chosen = Some(body);
                            }
                        }
                        chosen.ok_or_else(|| {
                            Diagnostic::error(
                                "yl.invalid_rewrite",
                                "No enum case matches argument",
                                arm.span.clone(),
                            )
                        })?
                    }
                };
                self.expand(&id, &symbol, body, &bound, depth + 1)?
            }
            ExprKind::Variant(_) => {
                return Err(Diagnostic::error(
                    "yl.ambiguous_enum_variant",
                    "Contextual enum needs an expected enum type",
                    expr.span.clone(),
                ))
            }
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
                Value::Enum(ty, v)
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
        let (ty, variant) = match &expr.kind {
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
                if let Some(Value::Enum(ty, v)) = env.get(name) {
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
            Term::Capture(_, t) => self.accepts(t, ty, visiting),
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
                    if parameters.len() != values.len() {
                        return Err(Diagnostic::error(
                            "yl.invalid_argument",
                            "Wrong constraint argument count",
                            constraint.span.clone(),
                        ));
                    }
                    let bound = parameters
                        .iter()
                        .zip(values)
                        .map(|(p, e)| (p.name.clone(), substitute(e, args)))
                        .collect();
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
                            return Err(Diagnostic::error("yl.parked_constraint_logic","PARKED: nested boolean constraint evaluation contract is unspecified",c.span.clone()));
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
                        let [Expr {
                            kind: ExprKind::Literal(message),
                            ..
                        }] = values.as_slice()
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
        match &expr.kind {
            ExprKind::Ref(name) if name=="true"=>Ok(Condition::Always),
            ExprKind::Call(name,arguments) if name.ends_with(".between")=>{
                let [a,b]=arguments.as_slice() else { return Err(Diagnostic::error("yl.invalid_argument","between requires two captures",expr.span.clone())); };
                let (ExprKind::Ref(left),ExprKind::Ref(right))=(&a.value.kind,&b.value.kind) else { return Err(Diagnostic::error("yl.invalid_constraint","between requires capture references",expr.span.clone())); };
                let owner=name.trim_end_matches(".between"); let trivia=if owner=="trivia" { None } else { Some(self.resolve(module,scope,owner,&expr.span)?) };
                Ok(Condition::Between { trivia,left:left.clone(),right:right.clone() })
            },
            ExprKind::Call(name,arguments) if name.ends_with(".matches")=>{
                let [arg]=arguments.as_slice() else { return Err(Diagnostic::error("yl.invalid_argument","matches requires one regex",expr.span.clone())); };
                let ExprKind::Regex(regex)=&arg.value.kind else { return Err(Diagnostic::error("yl.invalid_argument","matches requires a regex",expr.span.clone())); };
                regex::Regex::new(regex).map_err(|e|Diagnostic::error("yl.invalid_regex",e.to_string(),expr.span.clone()))?;
                Ok(Condition::Matches { capture:name.trim_end_matches(".matches").into(),regex:regex.clone() })
            },
            _=>Err(Diagnostic::error("yl.parked_constraint_logic","PARKED: boolean/string constraint operators beyond documented between/matches need specified syntax and behavior",expr.span.clone())),
        }
    }
}
fn substitute(expr: &Expr, args: &BTreeMap<String, Expr>) -> Expr {
    if let ExprKind::Ref(name) = &expr.kind {
        if let Some(value) = args.get(name) {
            return value.clone();
        }
    }
    let mut result = expr.clone();
    if let ExprKind::Call(_, values) = &mut result.kind {
        for value in values {
            value.value = substitute(&value.value, args);
        }
    }
    result
}
pub(crate) fn sequence(term: &Term) -> Vec<&Term> {
    if let Term::Sequence(items) = term {
        items.iter().collect()
    } else {
        vec![term]
    }
}
pub(crate) fn edge_ref(term: &Term) -> Option<&str> {
    match term {
        Term::Ref(id) => Some(id),
        Term::Capture(_, t) => edge_ref(t),
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
            Term::Repeat(t, _) | Term::Capture(_, t) | Term::NotAhead(t) | Term::NotBehind(t) => {
                term(t, language, span, depth + 1)
            }
            _ => Ok(()),
        }
    }
    for rule in language.rules.values() {
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
            }
        }
        let captures = match &rule.body {
            RuleBody::Concrete(t) => captures(t, &rule.span)?,
            RuleBody::Abstract { bases, operators } => {
                let mut names = BTreeSet::new();
                for id in bases.iter().chain(operators.iter().map(|o| &o.rule)) {
                    if let Some(Rule {
                        body: RuleBody::Concrete(t),
                        ..
                    }) = language.rules.get(id)
                    {
                        names.extend(captures(t, &rule.span)?);
                    }
                }
                names
            }
        };
        for check in &rule.constraints {
            let required = match &check.condition {
                Condition::Matches { capture, regex } => {
                    regex::Regex::new(regex).map_err(|e| {
                        Diagnostic::error("yl.invalid_regex", e.to_string(), rule.span.clone())
                    })?;
                    vec![capture]
                }
                Condition::Between {
                    trivia,
                    left,
                    right,
                } => {
                    if let Some(id) = trivia {
                        if !language.rules.get(id).is_some_and(|r| r.trivia) {
                            return Err(Diagnostic::error(
                                "yl.invalid_constraint",
                                "between owner is not declared trivia",
                                rule.span.clone(),
                            ));
                        }
                    }
                    vec![left, right]
                }
                Condition::Always => vec![],
            };
            for name in required {
                if !captures.contains(name) {
                    return Err(Diagnostic::error(
                        "yl.unknown_capture",
                        format!("Constraint references unknown capture {name}"),
                        rule.span.clone(),
                    ));
                }
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
        _ => {}
    }
    Ok(names)
}
