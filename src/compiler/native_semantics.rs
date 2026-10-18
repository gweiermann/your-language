//! Native semantic contracts and argument lowering.

use super::*;

impl Compiler {
    pub(super) fn lower_meaning_actions(
        &self,
        symbol: &DeclarationSymbol,
        call: &MeaningSyntaxCall,
    ) -> Result<Vec<MeaningCall>> {
        self.expand_semantic_actions(symbol, call, &BTreeMap::new(), &mut vec![])?
            .iter()
            .map(|call| self.lower_meaning_call(symbol, call))
            .collect()
    }
    fn lower_meaning_call(
        &self,
        symbol: &DeclarationSymbol,
        call: &MeaningSyntaxCall,
    ) -> Result<MeaningCall> {
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
        symbol: &DeclarationSymbol,
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
        if self.query_operations.contains(&name) {
            return Err(Diagnostic::error(
                "yl.query_native_effect",
                "Compiled query function values cannot be passed to native lifecycle operations",
                syntax.span.clone(),
            ));
        }
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
        symbol: &DeclarationSymbol,
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
            ExprKind::Call(name, _) if !pattern && self.query_operations.contains(name) => {
                Err(Diagnostic::error("yl.query_native_effect", "Compiled query function values cannot be passed to native lifecycle operations", expr.span.clone()))
            }
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
    pub(super) fn lower_meaning_match(
        &self,
        symbol: &DeclarationSymbol,
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
                if self
                    .diagnostic_severity(&symbol.module, &symbol.name, &call.name, &call.span)?
                    .is_some()
                {
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
                        severity: self
                            .diagnostic_severity(
                                &symbol.module,
                                &symbol.name,
                                &call.name,
                                &call.span,
                            )?
                            .expect("diagnostic resolved"),
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
}
