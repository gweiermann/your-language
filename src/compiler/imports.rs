//! Register all declarations before resolving imports, preserving safe module cycles.

use super::*;

impl Compiler {
    /// Register all declarations before resolving imports, preserving safe module cycles.
    pub(super) fn resolve_imports(&mut self) -> Result<()> {
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
        Ok(())
    }
}
