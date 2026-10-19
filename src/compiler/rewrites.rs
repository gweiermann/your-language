//! Structural rewrite selection and pattern expansion.

use super::*;

impl Compiler {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn select_case(
        &mut self,
        symbol: &DeclarationSymbol,
        selectors: &[Expr],
        cases: &[RewriteCase],
        bound: &GrammarBindings,
        parameters: &[Parameter],
        call: &Span,
        emit: bool,
    ) -> Result<usize> {
        let inferred;
        let selectors = if selectors.is_empty() {
            let enums: Vec<_> = parameters
                .iter()
                .filter(|p| matches!(bound.get(&p.name), Some(GrammarArgument::Enum(..))))
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
            let Some(GrammarArgument::Enum(ty, variant, origin)) = bound.get(name) else {
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
                if let ConstraintKind::Call(name, _) = &emission.kind {
                    if self
                        .diagnostic_severity(&symbol.module, &symbol.name, name, &emission.span)?
                        .is_none()
                    {
                        return Err(Diagnostic::error(
                            "yl.invalid_constraint",
                            "Expected imported diagnostic operation",
                            emission.span.clone(),
                        ));
                    }
                }
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
            let severity = self
                .diagnostic_severity(&symbol.module, &symbol.name, name, &emission.span)?
                .ok_or_else(|| {
                    Diagnostic::error(
                        "yl.invalid_constraint",
                        "Expected imported diagnostic operation",
                        emission.span.clone(),
                    )
                })?;
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
    pub(super) fn juxtaposition(
        &self,
        module: &str,
        scope: &str,
        expr: &Expr,
        env: &GrammarBindings,
    ) -> Result<Option<(Expr, Expr)>> {
        let make = |kind| Expr {
            kind,
            span: expr.span.clone(),
        };
        Ok(match &expr.kind {
            ExprKind::Call(name, args) if args.len() == 1 && args[0].name.is_none() => {
                let grammar = if let Some(value) = env.get(name) {
                    matches!(value, GrammarArgument::Grammar(_))
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
    pub(super) fn expand(
        &mut self,
        id: &str,
        symbol: &DeclarationSymbol,
        expr: &Expr,
        env: &GrammarBindings,
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
                } | DeclKind::Pattern {
                    ide: Some(crate::syntax::Ide {
                        highlight_as: Some(_),
                        ..
                    }),
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
}
