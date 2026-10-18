//! Recursive authored query function compilation.

use super::*;

impl Compiler {
    pub(super) fn find_query_operations(&mut self) -> Result<()> {
        fn expressions<'a>(body: &'a OperationBody, out: &mut Vec<&'a Expr>) {
            fn actions<'a>(items: &'a [MeaningSyntaxAction], out: &mut Vec<&'a Expr>) {
                for item in items {
                    match item {
                        MeaningSyntaxAction::Binding { value, .. }
                        | MeaningSyntaxAction::Return { value, .. } => out.push(value),
                        MeaningSyntaxAction::Call(call) => {
                            out.extend(call.arguments.iter().map(|arg| &arg.value))
                        }
                        MeaningSyntaxAction::If {
                            condition,
                            actions: yes,
                            otherwise,
                            ..
                        } => {
                            out.push(condition);
                            actions(yes, out);
                            actions(otherwise, out);
                        }
                        MeaningSyntaxAction::Match(value) => {
                            out.extend(&value.inputs);
                            for case in &value.cases {
                                actions(&case.nested_actions, out);
                                for call in &case.actions {
                                    out.extend(call.arguments.iter().map(|arg| &arg.value));
                                }
                            }
                        }
                    }
                }
            }
            match body {
                OperationBody::Query(value) => out.push(value),
                OperationBody::Conditional(items) => actions(items, out),
                OperationBody::Actions(calls) => {
                    for call in calls {
                        out.extend(call.arguments.iter().map(|arg| &arg.value));
                    }
                }
            }
        }
        let mut graph = BTreeMap::<String, BTreeSet<String>>::new();
        let mut pure = BTreeSet::new();
        let mut projected_parameters = BTreeSet::new();
        for (id, symbol) in &self.symbols {
            let DeclKind::Operation {
                body, parameters, ..
            } = &symbol.declaration.kind
            else {
                continue;
            };
            if matches!(body, OperationBody::Query(_))
                || matches!(body, OperationBody::Conditional(actions) if actions.iter().all(|action| matches!(action, MeaningSyntaxAction::Binding { .. } | MeaningSyntaxAction::Return { .. })) && actions.iter().any(|action| matches!(action, MeaningSyntaxAction::Return { .. })))
            {
                pure.insert(id.clone());
            }
            let mut pending = vec![];
            expressions(body, &mut pending);
            let mut edges = BTreeSet::new();
            while let Some(value) = pending.pop() {
                match &value.kind {
                    ExprKind::Ref(name)
                        if name.split_once('.').is_some_and(|(root, _)| {
                            parameters.iter().any(|parameter| parameter.name == root)
                        }) =>
                    {
                        projected_parameters.insert(id.clone());
                    }
                    ExprKind::Call(name, args) => {
                        let called =
                            self.resolve(&symbol.module, &symbol.name, name, &value.span)?;
                        edges.insert(called);
                        pending.extend(args.iter().map(|arg| &arg.value));
                    }
                    ExprKind::Tuple(values) | ExprKind::List(values) => pending.extend(values),
                    ExprKind::Group(value) | ExprKind::Not(value) | ExprKind::Index(value, _) => {
                        pending.push(value)
                    }
                    ExprKind::Lambda { body, .. } => pending.push(body),
                    ExprKind::Binary(_, left, right) => {
                        pending.push(left);
                        pending.push(right);
                    }
                    ExprKind::Match { inputs, cases } => {
                        pending.extend(inputs);
                        for case in cases {
                            pending.push(&case.value);
                            pending.extend(case.alternatives.iter().flatten().flatten());
                        }
                    }
                    _ => {}
                }
            }
            graph.insert(id.clone(), edges);
        }
        loop {
            let rejected: Vec<_> = pure
                .iter()
                .filter(|id| {
                    graph[*id].iter().any(|called| {
                        if graph.contains_key(called) {
                            !pure.contains(called)
                        } else {
                            !called.starts_with("core/query#")
                                || matches!(
                                    called.as_str(),
                                    "core/query#contribute" | "core/query#contributeAll"
                                )
                        }
                    })
                })
                .cloned()
                .collect();
            if rejected.is_empty() {
                break;
            }
            for id in rejected {
                pure.remove(&id);
            }
        }
        loop {
            let inherited: Vec<_> = graph
                .iter()
                .filter(|(id, edges)| {
                    !projected_parameters.contains(*id)
                        && edges.iter().any(|edge| projected_parameters.contains(edge))
                })
                .map(|(id, _)| id.clone())
                .collect();
            if inherited.is_empty() {
                break;
            }
            projected_parameters.extend(inherited);
        }
        self.query_operations.extend(
            pure.into_iter()
                .filter(|id| !projected_parameters.contains(id))
                .filter(|id| {
                    matches!(
                        &self.symbols[id].declaration.kind,
                        DeclKind::Operation {
                            body: OperationBody::Conditional(_),
                            ..
                        }
                    )
                }),
        );
        for start in graph.keys() {
            let mut seen = BTreeSet::new();
            let mut pending: Vec<_> = graph[start].iter().collect();
            while let Some(next) = pending.pop() {
                if next == start {
                    self.query_operations.insert(start.clone());
                    break;
                }
                if seen.insert(next) {
                    if let Some(edges) = graph.get(next) {
                        pending.extend(edges);
                    }
                }
            }
        }
        Ok(())
    }

    /// Reserve a function identity before lowering its body, allowing references to recursive peers.
    /// The function table owns normalized bodies; callers refer to identities rather than re-expanding.
    pub(super) fn compile_query_function(&self, id: &str) -> Result<()> {
        if self.query_functions.borrow().contains_key(id) {
            return Ok(());
        }
        let symbol = &self.symbols[id];
        let DeclKind::Operation {
            parameters, body, ..
        } = &symbol.declaration.kind
        else {
            unreachable!()
        };
        let actions = self.operation_actions(body);
        if actions.iter().any(|action| {
            !matches!(
                action,
                MeaningSyntaxAction::Binding { .. } | MeaningSyntaxAction::Return { .. }
            )
        }) {
            return Err(Diagnostic::error(
                "yl.recursive_effect",
                "Recursive operations must be pure value computations",
                symbol.declaration.span.clone(),
            ));
        }
        self.query_functions.borrow_mut().insert(id.into(), None);
        let env = parameters
            .iter()
            .map(|parameter| {
                (
                    parameter.name.clone(),
                    Expr {
                        kind: ExprKind::Local(parameter.name.clone()),
                        span: parameter.span.clone(),
                    },
                )
            })
            .collect();
        let mut stack = vec![];
        let bindings = self.local_bindings(symbol, &actions, &env, &mut stack)?;
        let result = match body {
            OperationBody::Query(value) => value,
            _ => actions
                .iter()
                .find_map(|action| match action {
                    MeaningSyntaxAction::Return { value, .. } => Some(value),
                    _ => None,
                })
                .ok_or_else(|| {
                    Diagnostic::error(
                        "yl.operation_result",
                        "A recursive operation needs a value result",
                        symbol.declaration.span.clone(),
                    )
                })?,
        };
        let expanded = self.expand_semantic_expr(symbol, result, &bindings, &mut stack)?;
        fn pure(
            value: crate::semantics::queries::QueryExpr,
        ) -> Result<crate::semantics::queries::QueryExpr> {
            use crate::semantics::queries::QueryExpr as Q;
            fn actions_pure(actions: Vec<crate::semantics::queries::QueryAction>) -> Result<()> {
                use crate::semantics::queries::QueryAction as A;
                for action in actions {
                    match action {
                        A::Call { name, args, .. } => {
                            pure(Q::Call { name, args })?;
                        }
                        A::If {
                            condition,
                            then_actions,
                            else_actions,
                            ..
                        } => {
                            pure(condition)?;
                            actions_pure(then_actions)?;
                            actions_pure(else_actions)?;
                        }
                        A::Match { inputs, cases, .. } => {
                            for input in inputs {
                                pure(input)?;
                            }
                            for case in cases {
                                for value in case.alternatives.into_iter().flatten().flatten() {
                                    pure(value)?;
                                }
                                actions_pure(case.actions)?;
                            }
                        }
                        A::Emit { .. } => {
                            return Err(Diagnostic::error(
                                "yl.recursive_effect",
                                "Recursive operations cannot emit diagnostics",
                                Span::default(),
                            ))
                        }
                    }
                }
                Ok(())
            }
            Ok(match value {
                Q::BindingValue { result, .. } => pure(*result)?,
                Q::Block {
                    actions, result, ..
                } => {
                    actions_pure(actions)?;
                    pure(*result)?
                }
                Q::Call { name, args } => {
                    if matches!(
                        name.as_str(),
                        "core/query#contribute" | "core/query#contributeAll"
                    ) {
                        return Err(Diagnostic::error(
                            "yl.recursive_effect",
                            "Recursive operations cannot contribute facts",
                            Span::default(),
                        ));
                    }
                    Q::Call {
                        name,
                        args: args.into_iter().map(pure).collect::<Result<_>>()?,
                    }
                }
                Q::Invoke { name, args } => Q::Invoke {
                    name,
                    args: args.into_iter().map(pure).collect::<Result<_>>()?,
                },
                Q::Match { inputs, cases } => Q::Match {
                    inputs: inputs.into_iter().map(pure).collect::<Result<_>>()?,
                    cases: cases
                        .into_iter()
                        .map(|case| {
                            Ok(crate::semantics::queries::QueryValueCase {
                                alternatives: case.alternatives,
                                value: pure(case.value)?,
                            })
                        })
                        .collect::<Result<_>>()?,
                },
                Q::List(values) => Q::List(values.into_iter().map(pure).collect::<Result<_>>()?),
                Q::Tuple(values) => Q::Tuple(values.into_iter().map(pure).collect::<Result<_>>()?),
                Q::Index { value, index } => Q::Index {
                    value: Box::new(pure(*value)?),
                    index,
                },
                Q::Not(value) => Q::Not(Box::new(pure(*value)?)),
                Q::Lambda { parameter, body } => Q::Lambda {
                    parameter,
                    body: Box::new(pure(*body)?),
                },
                value => value,
            })
        }
        for action in &actions {
            if let MeaningSyntaxAction::Binding { name, .. } = action {
                pure(self.query_expr_normalized(symbol, &bindings[name])?).map_err(
                    |mut error| {
                        error.message = format!("{} ({id}, definition {name})", error.message);
                        *error.primary = symbol.declaration.span.clone();
                        error
                    },
                )?;
            }
        }
        let body = pure(self.query_expr_normalized(symbol, &expanded)?).map_err(|mut error| {
            error.message = format!("{} ({id})", error.message);
            *error.primary = symbol.declaration.span.clone();
            error
        })?;
        self.query_functions.borrow_mut().insert(
            id.into(),
            Some(crate::semantics::queries::QueryFunction {
                parameters: parameters.iter().map(|p| p.name.clone()).collect(),
                body,
            }),
        );
        Ok(())
    }
}
