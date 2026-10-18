//! Syntax-local checks and condition lowering.

use super::*;

impl Compiler {
    pub(super) fn lower_checks(
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
                                &GrammarBindings::new(),
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
                                    &GrammarBindings::new(),
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
                        let severity = self
                            .diagnostic_severity(module, scope, name, &c.span)?
                            .ok_or_else(|| {
                                Diagnostic::error(
                                    "yl.invalid_constraint",
                                    "when body requires imported diagnostic operation",
                                    c.span.clone(),
                                )
                            })?;
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
    pub(super) fn condition(&self, module: &str, scope: &str, expr: &Expr) -> Result<Condition> {
        self.condition_depth(module, scope, expr, 0)
    }
    pub(super) fn condition_depth(
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
                    self.enum_value(module, scope, expr, None, &GrammarBindings::new())?;
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
                    let (ty, _) = self.enum_value(
                        module,
                        scope,
                        ungroup(other),
                        None,
                        &GrammarBindings::new(),
                    )?;
                    let (ty, variant) = self.enum_value(
                        module,
                        scope,
                        ungroup(variant),
                        Some(&ty),
                        &GrammarBindings::new(),
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
