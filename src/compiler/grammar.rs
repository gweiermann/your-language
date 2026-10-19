//! Concrete grammar lowering and abstract membership.

use super::*;

impl Compiler {
    pub(super) fn leaves(
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
    /// Expand a source grammar with bound parameters into a runtime term.
    /// `env` contains grammar/enum arguments; `depth` bounds recursive surface expansion.
    /// Captures retain their source spans and projection markers across nested pipes.
    pub(super) fn lower(
        &mut self,
        module: &str,
        scope: &str,
        expr: &Expr,
        env: &GrammarBindings,
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
                    if let GrammarArgument::Grammar(term) = value {
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
                    GrammarArgument::Grammar(Term::Mark {
                        id: marker,
                        term: Box::new(value),
                    }),
                );
                let body = match &arm.body {
                    RewriteBody::Direct(body) => body,
                    RewriteBody::Cases(original) => {
                        let enums: Vec<_> = parameters
                            .iter()
                            .filter(|p| {
                                matches!(bound.get(&p.name), Some(GrammarArgument::Enum(..)))
                            })
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
            ExprKind::Match { .. }
            | ExprKind::Tuple(_)
            | ExprKind::Index(_, _)
            | ExprKind::List(_)
            | ExprKind::EnumConstant(_, _)
            | ExprKind::Bool(_)
            | ExprKind::Absent
            | ExprKind::Not(_)
            | ExprKind::Binary(_, _, _)
            | ExprKind::SemanticBlock { .. }
            | ExprKind::SemanticBinding { .. }
            | ExprKind::Variant(_) => {
                return Err(Diagnostic::error(
                    "yl.ambiguous_enum_variant",
                    "Contextual enum needs an expected enum type",
                    expr.span.clone(),
                ))
            }
        })
    }
}
