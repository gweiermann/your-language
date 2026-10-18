//! Declaration and rewrite coverage validation.

use super::*;

impl Compiler {
    pub(super) fn validate_declarations(&self) -> Result<()> {
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
    pub(super) fn validate_pipe_matches(&mut self) -> Result<()> {
        for symbol in self.symbols.clone().values() {
            let DeclKind::Pipe {
                parameters, arms, ..
            } = &symbol.declaration.kind
            else {
                continue;
            };
            let mut bound = GrammarBindings::new();
            for p in parameters {
                if let Some(ty) = &p.ty {
                    let id = self.resolve(&symbol.module, &symbol.scope, ty, &p.span)?;
                    if let Some(DeclarationSymbol {
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
                            GrammarArgument::Enum(id.clone(), variant.clone(), None),
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
    pub(super) fn validate_expr(
        &self,
        symbol: &DeclarationSymbol,
        expr: &Expr,
        locals: &BTreeSet<String>,
    ) -> Result<()> {
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
            ExprKind::Capture(_, inner)
            | ExprKind::Repeat(inner, _)
            | ExprKind::Index(inner, _)
            | ExprKind::Group(inner) => self.validate_expr(symbol, inner, locals)?,
            ExprKind::Regex(regex) => {
                regex::Regex::new(regex).map_err(|e| {
                    Diagnostic::error("yl.invalid_regex", e.to_string(), expr.span.clone())
                })?;
            }
            _ => {}
        }
        Ok(())
    }
}
