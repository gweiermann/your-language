//! Grammar arguments, enum values, and type acceptance.

use super::*;

impl Compiler {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn bind(
        &mut self,
        symbol: &DeclarationSymbol,
        parameters: &[Parameter],
        args: &[Argument],
        module: &str,
        scope: &str,
        caller: &GrammarBindings,
        depth: usize,
    ) -> Result<GrammarBindings> {
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
        let mut env = GrammarBindings::new();
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
                    if let Some(GrammarArgument::Enum(_, _, origin)) = context_env.get(name) {
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
                GrammarArgument::Enum(ty, v, origin)
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
                GrammarArgument::Grammar(term)
            };
            env.insert(p.name.clone(), bound);
        }
        Ok(env)
    }
    pub(super) fn enum_value(
        &self,
        module: &str,
        scope: &str,
        expr: &Expr,
        expected: Option<&str>,
        env: &GrammarBindings,
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
                if let Some(GrammarArgument::Enum(ty, v, _)) = env.get(name) {
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
    pub(super) fn accepts(
        &self,
        term: &Term,
        ty: &str,
        visiting: &mut BTreeSet<String>,
    ) -> Result<bool> {
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
}
