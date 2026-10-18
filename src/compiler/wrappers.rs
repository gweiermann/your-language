//! Hygienic semantic wrapper expansion and argument effects.

use super::*;

impl Compiler {
    pub(super) fn validate_semantic_wrappers(&self) -> Result<()> {
        for wrapper in self.symbols.values() {
            let DeclKind::Operation {
                parameters, body, ..
            } = &wrapper.declaration.kind
            else {
                continue;
            };
            fn results(actions: &[MeaningSyntaxAction], nested: bool) -> Result<usize> {
                let mut count = 0;
                for action in actions {
                    match action {
                        MeaningSyntaxAction::Return { span, .. } => {
                            if nested {
                                return Err(Diagnostic::error(
                                    "yl.operation_result",
                                    "return must define one unconditional operation result",
                                    span.clone(),
                                ));
                            }
                            count += 1;
                            if count > 1 {
                                return Err(Diagnostic::error(
                                    "yl.operation_result",
                                    "An operation has only one return definition",
                                    span.clone(),
                                ));
                            }
                        }
                        MeaningSyntaxAction::If {
                            actions, otherwise, ..
                        } => {
                            results(actions, true)?;
                            results(otherwise, true)?;
                        }
                        MeaningSyntaxAction::Match(matching) => {
                            for case in &matching.cases {
                                results(&case.nested_actions, true)?;
                            }
                        }
                        _ => {}
                    }
                }
                Ok(count)
            }
            results(&self.operation_actions(body), false)?;
            let mut bindings = BTreeMap::new();
            for parameter in parameters {
                if let Some(default) = &parameter.default {
                    self.expand_semantic_expr(wrapper, default, &bindings, &mut vec![])?;
                }
                bindings.insert(
                    parameter.name.clone(),
                    Expr {
                        kind: ExprKind::Absent,
                        span: parameter.span.clone(),
                    },
                );
            }
            let mut stack = vec![format!("{}#{}", wrapper.module, wrapper.name)];
            match body {
                OperationBody::Conditional(actions) => {
                    self.lower_query_actions(wrapper, actions, &bindings, &mut stack)?;
                }
                OperationBody::Actions(calls) => {
                    for call in calls {
                        let probe = MeaningSyntaxGroup {
                            name: None,
                            calls: vec![call.clone()],
                            matching: None,
                            actions: vec![],
                            span: call.span.clone(),
                        };
                        if self.is_query_group(wrapper, &probe)? {
                            self.lower_query_actions(
                                wrapper,
                                &[MeaningSyntaxAction::Call(call.clone())],
                                &bindings,
                                &mut stack,
                            )?;
                        } else {
                            self.expand_semantic_actions(wrapper, call, &bindings, &mut stack)?;
                        }
                    }
                }
                OperationBody::Query(expr) => {
                    let value = self.expand_semantic_expr(wrapper, expr, &bindings, &mut stack)?;
                    if !matches!(value.kind, ExprKind::Call(_, _)) {
                        self.query_expr_normalized(wrapper, &value)?;
                    }
                }
            }
        }
        Ok(())
    }
    /// Charge every copied expression/action against the per-compilation expansion budget.
    /// Count nested action payloads as well as expression children to bound pathological wrappers.
    pub(super) fn charge_semantic_copy(&self, expr: &Expr) -> Result<()> {
        const LIMIT: usize = 1_000_000;
        let mut nodes = vec![expr];
        let mut count = self.expansion_nodes.get();
        while let Some(value) = nodes.pop() {
            count = count.saturating_add(1);
            if count > LIMIT {
                return Err(Diagnostic::error(
                    "yl.expansion_limit",
                    "Semantic definition expansion exceeds the resource limit",
                    expr.span.clone(),
                ));
            }
            match &value.kind {
                ExprKind::SemanticBinding { result, .. } => nodes.push(result),
                ExprKind::SemanticBlock {
                    actions, result, ..
                } => {
                    count = count.saturating_add(
                        crate::semantics::queries::action_node_count(
                            actions,
                            LIMIT.saturating_sub(count),
                        )
                        .ok_or_else(|| {
                            Diagnostic::error(
                                "yl.expansion_limit",
                                "Semantic definition expansion exceeds the resource limit",
                                expr.span.clone(),
                            )
                        })?,
                    );
                    nodes.push(result);
                }
                ExprKind::Call(_, args) => nodes.extend(args.iter().map(|arg| &arg.value)),
                ExprKind::Tuple(values) | ExprKind::List(values) => nodes.extend(values),
                ExprKind::Group(value) | ExprKind::Not(value) | ExprKind::Index(value, _) => {
                    nodes.push(value)
                }
                ExprKind::Lambda { body, .. } => nodes.push(body),
                ExprKind::Binary(_, left, right) => {
                    nodes.push(left);
                    nodes.push(right);
                }
                ExprKind::Match { inputs, cases } => {
                    nodes.extend(inputs);
                    for case in cases {
                        nodes.push(&case.value);
                        nodes.extend(case.alternatives.iter().flatten().flatten());
                    }
                }
                _ => {}
            }
        }
        self.expansion_nodes.set(count);
        Ok(())
    }
    /// Strip action-bearing wrapper envelopes when reading an already-defined binding.
    /// The original definition owns its effects; consumers only observe the resulting value.
    pub(super) fn result_only(value: &Expr) -> Expr {
        let kind = match &value.kind {
            ExprKind::Binary(operator, left, right) => ExprKind::Binary(
                operator.clone(),
                Box::new(Self::result_only(left)),
                Box::new(Self::result_only(right)),
            ),
            ExprKind::Match { inputs, cases } => ExprKind::Match {
                inputs: inputs.iter().map(Self::result_only).collect(),
                cases: cases
                    .iter()
                    .map(|case| ValueMatchCase {
                        alternatives: case
                            .alternatives
                            .iter()
                            .map(|patterns| {
                                patterns
                                    .iter()
                                    .map(|pattern| pattern.as_ref().map(Self::result_only))
                                    .collect()
                            })
                            .collect(),
                        value: Self::result_only(&case.value),
                        span: case.span.clone(),
                    })
                    .collect(),
            },
            ExprKind::SemanticBlock {
                invocation, result, ..
            } => ExprKind::SemanticBinding {
                invocation: invocation.clone(),
                result: Box::new(Self::result_only(result)),
            },
            ExprKind::SemanticBinding { invocation, result } => ExprKind::SemanticBinding {
                invocation: invocation.clone(),
                result: Box::new(Self::result_only(result)),
            },
            ExprKind::Call(name, args) => ExprKind::Call(
                name.clone(),
                args.iter()
                    .map(|arg| Argument {
                        value: Self::result_only(&arg.value),
                        ..arg.clone()
                    })
                    .collect(),
            ),
            ExprKind::Tuple(values) => {
                ExprKind::Tuple(values.iter().map(Self::result_only).collect())
            }
            ExprKind::List(values) => {
                ExprKind::List(values.iter().map(Self::result_only).collect())
            }
            ExprKind::Group(value) => ExprKind::Group(Box::new(Self::result_only(value))),
            ExprKind::Not(value) => ExprKind::Not(Box::new(Self::result_only(value))),
            ExprKind::Index(value, index) => {
                ExprKind::Index(Box::new(Self::result_only(value)), *index)
            }
            // A callback owns its body; it is not evaluated by reading its value.
            _ => value.kind.clone(),
        };
        Expr {
            kind,
            span: value.span.clone(),
        }
    }
    pub(super) fn argument_actions(
        &self,
        symbol: &DeclarationSymbol,
        bindings: &BTreeMap<String, Expr>,
    ) -> Result<Vec<crate::semantics::queries::QueryAction>> {
        fn has_block(value: &Expr) -> bool {
            match &value.kind {
                ExprKind::Binary(_, left, right) => has_block(left) || has_block(right),
                ExprKind::Match { inputs, cases } => {
                    inputs.iter().any(has_block)
                        || cases.iter().any(|case| {
                            has_block(&case.value)
                                || case.alternatives.iter().flatten().flatten().any(has_block)
                        })
                }
                ExprKind::SemanticBlock { .. } => true,
                ExprKind::Call(_, args) => args.iter().any(|arg| has_block(&arg.value)),
                ExprKind::Tuple(values) | ExprKind::List(values) => values.iter().any(has_block),
                ExprKind::Group(value) | ExprKind::Not(value) | ExprKind::Index(value, _) => {
                    has_block(value)
                }
                _ => false,
            }
        }
        bindings
            .values()
            .filter(|value| has_block(value))
            .map(|value| {
                Ok(crate::semantics::queries::QueryAction::Call {
                    name: "core/query#first".into(),
                    args: vec![crate::semantics::queries::QueryExpr::List(vec![
                        self.query_expr_normalized(symbol, value)?
                    ])],
                    span: value.span.clone(),
                })
            })
            .collect()
    }
    /// Expand authored operation expressions while preserving lexical bindings and call-site identity.
    /// `stack` detects expansion cycles; copied nodes share the compilation resource budget.
    pub(super) fn expand_semantic_expr(
        &self,
        context: &DeclarationSymbol,
        expr: &Expr,
        bindings: &BTreeMap<String, Expr>,
        stack: &mut Vec<String>,
    ) -> Result<Expr> {
        let mut result = expr.clone();
        result.kind = match &expr.kind {
            ExprKind::Lambda { parameter, body } => {
                let local = format!("{parameter}@{}:{}", expr.span.file, expr.span.start);
                let mut env = bindings.clone();
                env.insert(
                    parameter.clone(),
                    Expr {
                        kind: ExprKind::Local(local.clone()),
                        span: expr.span.clone(),
                    },
                );
                ExprKind::Lambda {
                    parameter: local,
                    body: Box::new(self.expand_semantic_expr(context, body, &env, stack)?),
                }
            }
            ExprKind::Not(inner) => ExprKind::Not(Box::new(
                self.expand_semantic_expr(context, inner, bindings, stack)?,
            )),
            ExprKind::Binary(operator, left, right) if matches!(operator.as_str(), "&&" | "||") => {
                let left = self.expand_semantic_expr(context, left, bindings, stack)?;
                let right = self.expand_semantic_expr(context, right, bindings, stack)?;
                let constant = Expr {
                    kind: ExprKind::Bool(operator == "||"),
                    span: expr.span.clone(),
                };
                let arguments = if operator == "&&" {
                    vec![left, right, constant]
                } else {
                    vec![left, constant, right]
                };
                ExprKind::Call(
                    "core/query#choose".into(),
                    arguments
                        .into_iter()
                        .map(|value| Argument {
                            name: None,
                            span: expr.span.clone(),
                            value,
                        })
                        .collect(),
                )
            }
            ExprKind::Ref(name) => {
                if let Some(value) = bindings.get(name) {
                    self.charge_semantic_copy(value)?;
                    return Ok(Self::result_only(value));
                }
                if let Some((root, suffix)) = name.split_once('.') {
                    if let Some(value) = bindings.get(root) {
                        fn project(value: &Expr, suffix: &str) -> Option<Expr> {
                            let kind = match &value.kind {
                                ExprKind::SemanticBinding { invocation, result } => {
                                    ExprKind::SemanticBinding {
                                        invocation: invocation.clone(),
                                        result: Box::new(project(result, suffix)?),
                                    }
                                }
                                ExprKind::SemanticBlock {
                                    invocation, result, ..
                                } => ExprKind::SemanticBinding {
                                    invocation: invocation.clone(),
                                    result: Box::new(project(result, suffix)?),
                                },
                                ExprKind::Ref(actual) => {
                                    ExprKind::Ref(format!("{actual}.{suffix}"))
                                }
                                ExprKind::SemanticCapture(actual) => {
                                    ExprKind::SemanticCapture(format!("{actual}.{suffix}"))
                                }
                                ExprKind::Absent => ExprKind::Absent,
                                _ => return None,
                            };
                            Some(Expr {
                                kind,
                                span: value.span.clone(),
                            })
                        }
                        if let Some(value) = project(value, suffix) {
                            return Ok(value);
                        }
                    }
                }
                if name == "self" {
                    if matches!(context.declaration.kind, DeclKind::Operation { .. }) {
                        return Err(Diagnostic::error(
                            "yl.operation_self",
                            "Operations have no implicit self; pass the occurrence as an argument",
                            expr.span.clone(),
                        ));
                    }
                    ExprKind::Ref(name.clone())
                } else if symbol_has_capture(context, name.split('.').next().unwrap_or(name)) {
                    ExprKind::SemanticCapture(name.clone())
                } else if name == "absent" {
                    ExprKind::Absent
                } else if matches!(name.as_str(), "true" | "false") {
                    ExprKind::Bool(name == "true")
                } else {
                    match self.resolve(&context.module, &context.name, name, &expr.span) {
                        Ok(id) => ExprKind::Ref(id),
                        Err(error)
                            if matches!(context.declaration.kind, DeclKind::Operation { .. }) =>
                        {
                            return Err(error)
                        }
                        Err(_) => ExprKind::Ref(name.clone()),
                    }
                }
            }

            ExprKind::Call(name, arguments) => {
                let id = self.resolve(&context.module, &context.name, name, &expr.span)?;
                let arguments = arguments
                    .iter()
                    .map(|argument| {
                        Ok(Argument {
                            value: self.expand_semantic_expr(
                                context,
                                &argument.value,
                                bindings,
                                stack,
                            )?,
                            ..argument.clone()
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                if let Some(wrapper) = self.symbols.get(&id) {
                    if let DeclKind::Operation {
                        parameters, body, ..
                    } = &wrapper.declaration.kind
                    {
                        if self.query_operations.contains(&id) {
                            self.compile_query_function(&id)?;
                            let mut call_stack = vec![];
                            self.invocation_path.borrow_mut().push(expr.span.clone());
                            let env = self.bind_semantic_wrapper(
                                wrapper,
                                parameters,
                                &arguments,
                                &expr.span,
                                &mut call_stack,
                            );
                            self.invocation_path.borrow_mut().pop();
                            let env = env?;
                            return Ok(Expr {
                                kind: ExprKind::Call(
                                    id,
                                    parameters
                                        .iter()
                                        .map(|parameter| Argument {
                                            name: None,
                                            value: env[&parameter.name].clone(),
                                            span: expr.span.clone(),
                                        })
                                        .collect(),
                                ),
                                span: expr.span.clone(),
                            });
                        }
                        self.invocation_path.borrow_mut().push(expr.span.clone());
                        let env = self.bind_semantic_wrapper(
                            wrapper, parameters, &arguments, &expr.span, stack,
                        )?;
                        let mut argument_actions = self.argument_actions(wrapper, &env)?;
                        let expanded = match body {
                            OperationBody::Query(body) => {
                                let value =
                                    self.expand_semantic_expr(wrapper, body, &env, stack)?;
                                if argument_actions.is_empty() {
                                    Ok(value)
                                } else {
                                    let invocation = self
                                        .invocation_path
                                        .borrow()
                                        .iter()
                                        .map(|span| {
                                            format!("{}:{}:{}", span.file, span.start, span.end)
                                        })
                                        .collect::<Vec<_>>()
                                        .join("/");
                                    Ok(Expr {
                                        kind: ExprKind::SemanticBlock {
                                            invocation,
                                            actions: argument_actions,
                                            result: Box::new(value),
                                        },
                                        span: expr.span.clone(),
                                    })
                                }
                            }
                            _ => {
                                let actions = self.operation_actions(body);
                                let returns: Vec<_> = actions
                                    .iter()
                                    .filter_map(|action| match action {
                                        MeaningSyntaxAction::Return { value, .. } => Some(value),
                                        _ => None,
                                    })
                                    .collect();
                                if returns.len() != 1 {
                                    Err(Diagnostic::error("yl.operation_result", "An operation used as a value requires exactly one root return definition", expr.span.clone()))
                                } else {
                                    let env =
                                        self.local_bindings(wrapper, &actions, &env, stack)?;
                                    let result = self
                                        .expand_semantic_expr(wrapper, returns[0], &env, stack)?;
                                    argument_actions.extend(
                                        self.lower_query_actions(wrapper, &actions, &env, stack)?,
                                    );
                                    let actions = argument_actions;
                                    let invocation = self
                                        .invocation_path
                                        .borrow()
                                        .iter()
                                        .map(|span| {
                                            format!("{}:{}:{}", span.file, span.start, span.end)
                                        })
                                        .collect::<Vec<_>>()
                                        .join("/");
                                    Ok(Expr {
                                        kind: ExprKind::SemanticBlock {
                                            invocation,
                                            actions,
                                            result: Box::new(result),
                                        },
                                        span: expr.span.clone(),
                                    })
                                }
                            }
                        };
                        self.invocation_path.borrow_mut().pop();
                        stack.pop();
                        return expanded.map(|mut value| {
                            value.span = expr.span.clone();
                            value
                        });
                    }
                }
                ExprKind::Call(id, arguments)
            }
            ExprKind::Tuple(values) => ExprKind::Tuple(
                values
                    .iter()
                    .map(|value| self.expand_semantic_expr(context, value, bindings, stack))
                    .collect::<Result<Vec<_>>>()?,
            ),
            ExprKind::List(values) => ExprKind::List(
                values
                    .iter()
                    .map(|value| self.expand_semantic_expr(context, value, bindings, stack))
                    .collect::<Result<Vec<_>>>()?,
            ),
            ExprKind::Index(value, index) => ExprKind::Index(
                Box::new(self.expand_semantic_expr(context, value, bindings, stack)?),
                *index,
            ),
            ExprKind::Match { inputs, cases } => ExprKind::Match {
                inputs: inputs
                    .iter()
                    .map(|value| self.expand_semantic_expr(context, value, bindings, stack))
                    .collect::<Result<_>>()?,
                cases: cases
                    .iter()
                    .map(|case| {
                        Ok(ValueMatchCase {
                            alternatives: case
                                .alternatives
                                .iter()
                                .map(|row| {
                                    row.iter()
                                        .map(|value| {
                                            value
                                                .as_ref()
                                                .map(|value| {
                                                    self.expand_semantic_expr(
                                                        context, value, bindings, stack,
                                                    )
                                                })
                                                .transpose()
                                        })
                                        .collect::<Result<_>>()
                                })
                                .collect::<Result<_>>()?,
                            value: self.expand_semantic_expr(
                                context,
                                &case.value,
                                bindings,
                                stack,
                            )?,
                            span: case.span.clone(),
                        })
                    })
                    .collect::<Result<_>>()?,
            },
            ExprKind::Group(value) => {
                return self.expand_semantic_expr(context, value, bindings, stack)
            }
            _ => expr.kind.clone(),
        };
        Ok(result)
    }
    pub(super) fn bind_semantic_wrapper(
        &self,
        wrapper: &DeclarationSymbol,
        parameters: &[Parameter],
        arguments: &[Argument],
        span: &Span,
        stack: &mut Vec<String>,
    ) -> Result<BTreeMap<String, Expr>> {
        let id = format!("{}#{}", wrapper.module, wrapper.name);
        if stack.contains(&id) || stack.len() >= 128 {
            return Err(Diagnostic::error(
                "yl.operation_cycle",
                "Recursive semantic operation expansion",
                span.clone(),
            ));
        }
        stack.push(id);
        let mut env = BTreeMap::new();
        let mut position = 0;
        let mut named = false;
        for argument in arguments {
            let parameter = if let Some(name) = &argument.name {
                named = true;
                parameters.iter().find(|parameter| &parameter.name == name)
            } else {
                if named {
                    return Err(Diagnostic::error(
                        "yl.invalid_argument",
                        "Positional argument follows named argument",
                        argument.span.clone(),
                    ));
                }
                let parameter = parameters.get(position);
                position += 1;
                parameter
            }
            .ok_or_else(|| {
                Diagnostic::error(
                    "yl.invalid_argument",
                    "Unknown or excess wrapper argument",
                    argument.span.clone(),
                )
            })?;
            if env
                .insert(parameter.name.clone(), argument.value.clone())
                .is_some()
            {
                return Err(Diagnostic::error(
                    "yl.invalid_argument",
                    "Duplicate wrapper argument",
                    argument.span.clone(),
                ));
            }
        }
        for parameter in parameters {
            if !env.contains_key(&parameter.name) {
                let default = parameter.default.as_ref().ok_or_else(|| {
                    Diagnostic::error(
                        "yl.invalid_argument",
                        format!("Missing operation argument {}", parameter.name),
                        span.clone(),
                    )
                })?;
                let value = self.expand_semantic_expr(wrapper, default, &env, stack)?;
                env.insert(parameter.name.clone(), value);
            }
        }
        Ok(env)
    }
    pub(super) fn expand_semantic_actions(
        &self,
        context: &DeclarationSymbol,
        call: &MeaningSyntaxCall,
        bindings: &BTreeMap<String, Expr>,
        stack: &mut Vec<String>,
    ) -> Result<Vec<MeaningSyntaxCall>> {
        let id = self.resolve(&context.module, &context.name, &call.name, &call.span)?;
        let arguments = call
            .arguments
            .iter()
            .map(|argument| {
                Ok(Argument {
                    value: self.expand_semantic_expr(context, &argument.value, bindings, stack)?,
                    ..argument.clone()
                })
            })
            .collect::<Result<Vec<_>>>()?;
        if let Some(wrapper) = self.symbols.get(&id) {
            if let DeclKind::Operation {
                parameters, body, ..
            } = &wrapper.declaration.kind
            {
                let OperationBody::Actions(calls) = body else {
                    return Err(Diagnostic::error(
                        "yl.invalid_operation_action",
                        "Query operation cannot be used as an action",
                        call.span.clone(),
                    ));
                };
                self.invocation_path.borrow_mut().push(call.span.clone());
                let env =
                    self.bind_semantic_wrapper(wrapper, parameters, &arguments, &call.span, stack)?;
                let mut expanded = vec![];
                for nested in calls {
                    expanded.extend(self.expand_semantic_actions(wrapper, nested, &env, stack)?);
                }
                self.invocation_path.borrow_mut().pop();
                stack.pop();
                for nested in &mut expanded {
                    nested.span = call.span.clone();
                }
                return Ok(expanded);
            }
        }
        Ok(vec![MeaningSyntaxCall {
            name: id,
            arguments,
            span: call.span.clone(),
        }])
    }
}
