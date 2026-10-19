//! Symbol registration and module/member name resolution.

use super::*;

impl Compiler {
    /// Index a declaration under its module and enclosing declaration path.
    /// Members and exports point at the same declaration identity rather than cloned semantic values.
    pub(super) fn register(
        &mut self,
        module: &str,
        scope: &str,
        declaration: &Declaration,
    ) -> Result<Option<String>> {
        let name = match &declaration.kind {
            DeclKind::Hook { name, .. }
            | DeclKind::Operation { name, .. }
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
            DeclarationSymbol {
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
    pub(super) fn is_enum_variant(&self, name: &str) -> bool {
        name.rsplit_once("::").is_some_and(|(parent, variant)| self.symbols.get(parent).is_some_and(|symbol| matches!(&symbol.declaration.kind, DeclKind::Enum { variants, .. } if variants.iter().any(|value| value == variant))))
    }
    /// Resolve a source spelling in a module/member scope, preserving the caller span on errors.
    pub(super) fn resolve(
        &self,
        module: &str,
        scope: &str,
        name: &str,
        span: &Span,
    ) -> Result<String> {
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
            || self.is_enum_variant(name)
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
    pub(super) fn member_path(
        &self,
        parent: &str,
        tail: &str,
        span: &Span,
        depth: usize,
    ) -> Result<String> {
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
        if self.symbols.get(parent).is_some_and(|symbol| matches!(&symbol.declaration.kind, DeclKind::Enum { variants, .. } if variants.iter().any(|variant| variant == head))) {
            if rest.is_some() { return Err(Diagnostic::error("yl.unknown_reference", "Enum variants have no members", span.clone())); }
            return Ok(format!("{parent}::{head}"));
        }
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
}
