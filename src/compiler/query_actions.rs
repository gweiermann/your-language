//! Reactive lexical bindings and query actions.

use super::*;

impl Compiler {
    /// Resolve immutable definitions by dependency rather than textual order.
    /// `outer` is the enclosing lexical environment; each returned binding retains its defining
    /// query context so reads cannot accidentally re-anchor order-sensitive operations.
    pub(super) fn local_bindings(
        &self,
        symbol: &DeclarationSymbol,
        actions: &[MeaningSyntaxAction],
        outer: &BTreeMap<String, Expr>,
        stack: &mut Vec<String>,
    ) -> Result<BTreeMap<String, Expr>> {
        let mut definitions = BTreeMap::new();
        for action in actions {
            if let MeaningSyntaxAction::Binding { name, value, span } = action {
                if definitions.insert(name.clone(), value.clone()).is_some() {
                    return Err(Diagnostic::error(
                        "yl.duplicate_binding",
                        format!("Duplicate immutable definition {name}"),
                        span.clone(),
                    ));
                }
            }
        }
        struct Definitions<'a> {
            compiler: &'a Compiler,
            symbol: &'a DeclarationSymbol,
            values: &'a BTreeMap<String, Expr>,
            env: BTreeMap<String, Expr>,
            visiting: BTreeSet<String>,
            completed: BTreeSet<String>,
        }
        impl Definitions<'_> {
            fn expand(&mut self, name: &str, stack: &mut Vec<String>) -> Result<()> {
                if self.completed.contains(name) {
                    return Ok(());
                }
                if self.visiting.len() >= 128 {
                    return Err(Diagnostic::error(
                        "yl.binding_limit",
                        "Immutable definition dependencies exceed the nesting limit",
                        self.values[name].span.clone(),
                    ));
                }
                if !self.visiting.insert(name.into()) {
                    return Err(Diagnostic::error(
                        "yl.binding_cycle",
                        "Cyclic immutable definitions",
                        self.values[name].span.clone(),
                    ));
                }
                fn refs(expr: &Expr, names: &mut BTreeSet<String>, bound: &BTreeSet<String>) {
                    match &expr.kind {
                        ExprKind::Binary(_, left, right) => {
                            refs(left, names, bound);
                            refs(right, names, bound);
                        }
                        ExprKind::Match { inputs, cases } => {
                            for input in inputs {
                                refs(input, names, bound);
                            }
                            for case in cases {
                                refs(&case.value, names, bound);
                                for pattern in case.alternatives.iter().flatten().flatten() {
                                    refs(pattern, names, bound);
                                }
                            }
                        }
                        ExprKind::Ref(name) => {
                            let root = name.split('.').next().unwrap_or(name);
                            if !bound.contains(root) {
                                names.insert(root.into());
                            }
                        }
                        ExprKind::Call(_, args) => {
                            for arg in args {
                                refs(&arg.value, names, bound);
                            }
                        }
                        ExprKind::List(values) | ExprKind::Tuple(values) => {
                            for value in values {
                                refs(value, names, bound);
                            }
                        }
                        ExprKind::Group(value)
                        | ExprKind::Not(value)
                        | ExprKind::Index(value, _) => refs(value, names, bound),
                        ExprKind::Lambda { parameter, body } => {
                            let mut nested = bound.clone();
                            nested.insert(parameter.clone());
                            refs(body, names, &nested);
                        }
                        _ => {}
                    }
                }
                let mut names = BTreeSet::new();
                refs(&self.values[name], &mut names, &BTreeSet::new());
                for dependency in names {
                    if self.values.contains_key(&dependency) {
                        self.expand(&dependency, stack)?;
                    }
                }
                let value = self.compiler.expand_semantic_expr(
                    self.symbol,
                    &self.values[name],
                    &self.env,
                    stack,
                )?;
                let span = self.values[name].span.clone();
                let outer = self
                    .compiler
                    .invocation_path
                    .borrow()
                    .iter()
                    .map(|span| format!("{}:{}:{}", span.file, span.start, span.end))
                    .collect::<Vec<_>>()
                    .join("/");
                let invocation =
                    format!("binding:{outer}/{}:{}:{}", span.file, span.start, span.end);
                self.env.insert(
                    name.into(),
                    Expr {
                        kind: ExprKind::SemanticBinding {
                            invocation,
                            result: Box::new(value),
                        },
                        span,
                    },
                );
                self.visiting.remove(name);
                self.completed.insert(name.into());
                Ok(())
            }
        }
        let mut definitions = Definitions {
            compiler: self,
            symbol,
            values: &definitions,
            env: outer.clone(),
            visiting: BTreeSet::new(),
            completed: BTreeSet::new(),
        };
        for name in definitions.values.keys() {
            definitions.expand(name, stack)?;
        }
        let env = definitions.env;
        Ok(env)
    }
    pub(super) fn lower_query_actions(
        &self,
        symbol: &DeclarationSymbol,
        actions: &[MeaningSyntaxAction],
        bindings: &BTreeMap<String, Expr>,
        stack: &mut Vec<String>,
    ) -> Result<Vec<crate::semantics::queries::QueryAction>> {
        use crate::semantics::queries::{
            QueryAction as A, QueryCase, QueryExpr as Q, QueryMessagePart,
        };
        let local_env = self.local_bindings(symbol, actions, bindings, stack)?;
        let bindings = &local_env;
        let mut result = vec![];
        for action in actions {
            if let MeaningSyntaxAction::Binding { name, span, .. } = action {
                let ExprKind::SemanticBinding {
                    invocation,
                    result: binding_result,
                } = &bindings[name].kind
                else {
                    unreachable!("local definitions carry their context")
                };
                let value = Q::Block {
                    invocation: invocation.clone(),
                    actions: vec![],
                    result: Box::new(self.query_expr_normalized(symbol, binding_result)?),
                };
                result.push(A::Call {
                    name: "core/query#first".into(),
                    args: vec![Q::List(vec![value])],
                    span: span.clone(),
                });
            }
        }
        for action in actions {
            match action {
                MeaningSyntaxAction::Binding { .. } => {}
                MeaningSyntaxAction::Return { value, span } => {
                    if !matches!(symbol.declaration.kind, DeclKind::Operation { .. }) {
                        return Err(Diagnostic::error(
                            "yl.operation_result",
                            "return is only valid in an operation",
                            span.clone(),
                        ));
                    }
                    // A result is declarative: later actions are still discovered.
                    let result_value = self.lower_query_expr(symbol, value, bindings, stack)?;
                    result.push(A::Call {
                        name: "core/query#first".into(),
                        args: vec![Q::List(vec![result_value])],
                        span: span.clone(),
                    });
                }
                MeaningSyntaxAction::If {
                    condition,
                    actions,
                    otherwise,
                    span,
                } => result.push(A::If {
                    condition: self.lower_query_expr(symbol, condition, bindings, stack)?,
                    then_actions: self.lower_query_actions(symbol, actions, bindings, stack)?,
                    else_actions: self.lower_query_actions(symbol, otherwise, bindings, stack)?,
                    span: span.clone(),
                }),
                MeaningSyntaxAction::Match(matching) => {
                    let inputs = matching
                        .inputs
                        .iter()
                        .map(|input| self.lower_query_expr(symbol, input, bindings, stack))
                        .collect::<Result<Vec<_>>>()?;
                    let mut cases = vec![];
                    for (index, case) in matching.cases.iter().enumerate() {
                        let alternatives = case
                            .alternatives
                            .iter()
                            .map(|alternative| {
                                if alternative.len() != inputs.len()
                                    || (index + 1 != matching.cases.len()
                                        && alternative.iter().all(Option::is_none))
                                {
                                    return Err(Diagnostic::error(
                                        "yl.invalid_meaning_match",
                                        "Match arity differs or wildcard is not last",
                                        case.span.clone(),
                                    ));
                                }
                                alternative
                                    .iter()
                                    .map(|value| {
                                        value
                                            .as_ref()
                                            .map(|value| {
                                                self.lower_query_expr(
                                                    symbol, value, bindings, stack,
                                                )
                                            })
                                            .transpose()
                                    })
                                    .collect::<Result<Vec<_>>>()
                            })
                            .collect::<Result<Vec<_>>>()?;
                        let mut actions = case
                            .actions
                            .iter()
                            .cloned()
                            .map(MeaningSyntaxAction::Call)
                            .collect::<Vec<_>>();
                        actions.extend(case.nested_actions.clone());
                        cases.push(QueryCase {
                            alternatives,
                            actions: self.lower_query_actions(symbol, &actions, bindings, stack)?,
                            span: case.span.clone(),
                        });
                    }
                    if !matching.cases.last().is_some_and(|case| {
                        case.alternatives
                            .iter()
                            .any(|alt| alt.iter().all(Option::is_none))
                    }) {
                        return Err(Diagnostic::error(
                            "yl.non_exhaustive_meaning_match",
                            "Match requires a final wildcard case",
                            matching.span.clone(),
                        ));
                    }
                    result.push(A::Match {
                        inputs,
                        cases,
                        span: matching.span.clone(),
                    });
                }
                MeaningSyntaxAction::Call(call) => {
                    if self
                        .diagnostic_severity(&symbol.module, &symbol.name, &call.name, &call.span)?
                        .is_some()
                    {
                        let (arg, target, related) = match call.arguments.as_slice() {
                            [arg] => (arg, None, vec![]),
                            [arg, target] => (
                                arg,
                                Some(self.lower_query_expr(
                                    symbol,
                                    &target.value,
                                    bindings,
                                    stack,
                                )?),
                                vec![],
                            ),
                            [arg, target, related] => (
                                arg,
                                Some(self.lower_query_expr(
                                    symbol,
                                    &target.value,
                                    bindings,
                                    stack,
                                )?),
                                vec![self.lower_query_expr(
                                    symbol,
                                    &related.value,
                                    bindings,
                                    stack,
                                )?],
                            ),
                            _ => {
                                return Err(Diagnostic::error(
                                    "yl.invalid_diagnostic",
                                    "Diagnostic needs a message and optional source target",
                                    call.span.clone(),
                                ))
                            }
                        };
                        let expanded =
                            self.expand_semantic_expr(symbol, &arg.value, bindings, stack)?;
                        let (text, mut message) = match &expanded.kind {
                            ExprKind::Literal(text) => (text.as_str(), vec![]),
                            _ => (
                                "",
                                vec![QueryMessagePart::Value(
                                    self.query_expr_normalized(symbol, &expanded)?,
                                )],
                            ),
                        };
                        let mut rest = text;
                        while let Some((before, after)) = rest.split_once("${") {
                            message.push(QueryMessagePart::Text(before.into()));
                            let mut quoted = false;
                            let mut escaped = false;
                            let close = after
                                .char_indices()
                                .find_map(|(offset, ch)| {
                                    if escaped {
                                        escaped = false;
                                        return None;
                                    }
                                    if quoted && ch == '\\' {
                                        escaped = true;
                                        return None;
                                    }
                                    if ch == '"' {
                                        quoted = !quoted;
                                        return None;
                                    }
                                    (ch == '}' && !quoted).then_some(offset)
                                })
                                .ok_or_else(|| {
                                    Diagnostic::error(
                                        "yl.invalid_diagnostic",
                                        "Unterminated diagnostic interpolation",
                                        arg.span.clone(),
                                    )
                                })?;
                            let capture = &after[..close];
                            let tail = &after[close + 1..];
                            let expression =
                                crate::frontend::parse_semantic_expr(&arg.span.file, capture)
                                    .map_err(|errors| {
                                        Diagnostic::error(
                                            "yl.invalid_diagnostic",
                                            format!(
                                                "Invalid diagnostic interpolation: {}",
                                                errors[0].message
                                            ),
                                            arg.span.clone(),
                                        )
                                    })?;
                            let value = self
                                .lower_query_expr(symbol, &expression, bindings, stack)
                                .map_err(|mut error| {
                                    // Interpolation parser spans are relative to its substring.
                                    // Report source errors on the original message argument.
                                    *error.primary = arg.span.clone();
                                    error
                                })?;
                            message.push(QueryMessagePart::Value(value));
                            rest = tail;
                        }
                        message.push(QueryMessagePart::Text(rest.into()));
                        result.push(A::Emit {
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
                            target,
                            related,
                        });
                        continue;
                    }
                    let id = self.resolve(&symbol.module, &symbol.name, &call.name, &call.span)?;
                    let args = call
                        .arguments
                        .iter()
                        .map(|arg| {
                            Ok(Argument {
                                value: self
                                    .expand_semantic_expr(symbol, &arg.value, bindings, stack)?,
                                ..arg.clone()
                            })
                        })
                        .collect::<Result<Vec<_>>>()?;
                    if let Some(wrapper) = self.symbols.get(&id) {
                        if let DeclKind::Operation {
                            parameters, body, ..
                        } = &wrapper.declaration.kind
                        {
                            self.invocation_path.borrow_mut().push(call.span.clone());
                            let env = self.bind_semantic_wrapper(
                                wrapper, parameters, &args, &call.span, stack,
                            )?;
                            let nested = match body {
                                OperationBody::Actions(calls) => calls
                                    .iter()
                                    .cloned()
                                    .map(MeaningSyntaxAction::Call)
                                    .collect(),
                                OperationBody::Conditional(actions) => actions.clone(),
                                OperationBody::Query(_) => {
                                    return Err(Diagnostic::error(
                                        "yl.invalid_operation_action",
                                        "Query operation cannot be used as an action",
                                        call.span.clone(),
                                    ))
                                }
                            };
                            result.extend(self.argument_actions(wrapper, &env)?);
                            result.extend(self.lower_query_actions(wrapper, &nested, &env, stack)?);
                            self.invocation_path.borrow_mut().pop();
                            stack.pop();
                            continue;
                        }
                    }
                    result.push(A::Call {
                        name: id.clone(),
                        args: self.query_arguments(symbol, &id, &args, &call.span)?,
                        span: call.span.clone(),
                    });
                }
            }
        }
        Ok(result)
    }
}
