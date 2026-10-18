//! Reactive group recognition and expression compilation.

use super::*;

impl Compiler {
    pub(super) fn is_query_group(
        &self,
        symbol: &DeclarationSymbol,
        group: &MeaningSyntaxGroup,
    ) -> Result<bool> {
        if !group.actions.is_empty() {
            return Ok(true);
        }
        fn query(expr: &Expr) -> bool {
            match &expr.kind {
                ExprKind::Binary(_, _, _)
                | ExprKind::Match { .. }
                | ExprKind::Lambda { .. }
                | ExprKind::Local(_)
                | ExprKind::Index(_, _) => true,
                ExprKind::Call(name, args) => {
                    name.starts_with("core/query#") || args.iter().any(|arg| query(&arg.value))
                }
                ExprKind::Tuple(values) | ExprKind::List(values) => values.iter().any(query),
                ExprKind::Group(value) | ExprKind::Not(value) => query(value),
                _ => false,
            }
        }
        fn action(
            compiler: &Compiler,
            symbol: &DeclarationSymbol,
            call: &MeaningSyntaxCall,
            visiting: &mut BTreeSet<String>,
        ) -> Result<bool> {
            if compiler
                .diagnostic_severity(&symbol.module, &symbol.name, &call.name, &call.span)?
                .is_some()
            {
                return Ok(false);
            }
            let id = compiler.resolve(&symbol.module, &symbol.name, &call.name, &call.span)?;
            if id.starts_with("core/query#") {
                return Ok(true);
            }
            if let Some(wrapper) = compiler.symbols.get(&id) {
                if visiting.len() >= 128 || !visiting.insert(id.clone()) {
                    return Err(Diagnostic::error(
                        "yl.operation_cycle",
                        "Recursive semantic operation expansion",
                        call.span.clone(),
                    ));
                }
                if let DeclKind::Operation {
                    body: OperationBody::Conditional(_),
                    ..
                } = &wrapper.declaration.kind
                {
                    return Ok(true);
                }
                if let DeclKind::Operation {
                    body: OperationBody::Actions(calls),
                    ..
                } = &wrapper.declaration.kind
                {
                    for nested in calls {
                        if action(compiler, wrapper, nested, visiting)? {
                            return Ok(true);
                        }
                    }
                }
                visiting.remove(&id);
            }
            let bindings = match &symbol.declaration.kind {
                DeclKind::Operation { parameters, .. } => parameters
                    .iter()
                    .map(|parameter| {
                        (
                            parameter.name.clone(),
                            Expr {
                                kind: ExprKind::Absent,
                                span: parameter.span.clone(),
                            },
                        )
                    })
                    .collect(),
                _ => BTreeMap::new(),
            };
            let expanded =
                compiler.expand_semantic_actions(symbol, call, &bindings, &mut vec![])?;
            Ok(expanded.iter().any(|call| {
                call.name.starts_with("core/query#")
                    || call.arguments.iter().any(|arg| query(&arg.value))
            }))
        }
        for call in &group.calls {
            if self
                .diagnostic_severity(&symbol.module, &symbol.name, &call.name, &call.span)?
                .is_some()
            {
                return Ok(true);
            }
            if action(self, symbol, call, &mut BTreeSet::new())? {
                return Ok(true);
            }
        }
        if let Some(matching) = &group.matching {
            if matching
                .cases
                .iter()
                .any(|case| !case.nested_actions.is_empty())
            {
                return Ok(true);
            }
            for input in &matching.inputs {
                let expanded =
                    self.expand_semantic_expr(symbol, input, &BTreeMap::new(), &mut vec![])?;
                if query(&expanded) {
                    return Ok(true);
                }
            }
            for case in &matching.cases {
                for call in &case.actions {
                    if action(self, symbol, call, &mut BTreeSet::new())? {
                        return Ok(true);
                    }
                }
            }
        }
        Ok(false)
    }
    pub(super) fn lower_query_meanings(
        &self,
        symbol: &DeclarationSymbol,
        meanings: &Meanings,
    ) -> Result<Vec<crate::semantics::queries::QueryGroup>> {
        let mut groups = vec![];
        let mut names = BTreeSet::new();
        let ungrouped: Vec<_> = meanings
            .groups
            .iter()
            .filter(|group| group.name.is_none())
            .flat_map(|group| group.actions.clone())
            .collect();
        let common_bindings =
            self.local_bindings(symbol, &ungrouped, &BTreeMap::new(), &mut vec![])?;
        let mut common_discovered = false;
        for group in &meanings.groups {
            if group
                .name
                .as_ref()
                .is_some_and(|name| !names.insert(name.clone()))
            {
                return Err(Diagnostic::error(
                    "yl.duplicate_meaning_group",
                    "Duplicate meaning group",
                    group.span.clone(),
                ));
            }
            if !self.is_query_group(symbol, group)? {
                continue;
            }
            let mut syntax = group
                .calls
                .iter()
                .cloned()
                .map(MeaningSyntaxAction::Call)
                .collect::<Vec<_>>();
            syntax.extend(group.actions.clone());
            if let Some(matching) = &group.matching {
                syntax.push(MeaningSyntaxAction::Match(matching.clone()));
            }
            let mut actions =
                self.lower_query_actions(symbol, &syntax, &common_bindings, &mut vec![])?;
            if group.name.is_none() && !common_discovered {
                common_discovered = true;
                let definitions: Vec<_> = ungrouped
                    .iter()
                    .filter(|action| matches!(action, MeaningSyntaxAction::Binding { .. }))
                    .cloned()
                    .collect();
                let mut observers =
                    self.lower_query_actions(symbol, &definitions, &common_bindings, &mut vec![])?;
                observers.append(&mut actions);
                actions = observers;
            }
            groups.push(crate::semantics::queries::QueryGroup {
                name: group.name.clone(),
                actions,
                span: group.span.clone(),
            });
        }
        Ok(groups)
    }
    pub(super) fn lower_query_expr(
        &self,
        symbol: &DeclarationSymbol,
        expr: &Expr,
        bindings: &BTreeMap<String, Expr>,
        stack: &mut Vec<String>,
    ) -> Result<crate::semantics::queries::QueryExpr> {
        let expanded = self.expand_semantic_expr(symbol, expr, bindings, stack)?;
        self.query_expr_normalized(symbol, &expanded)
    }
    pub(super) fn query_expr_normalized(
        &self,
        symbol: &DeclarationSymbol,
        expr: &Expr,
    ) -> Result<crate::semantics::queries::QueryExpr> {
        use crate::semantics::queries::QueryExpr as Q;
        Ok(match &expr.kind {
            ExprKind::SemanticBinding { invocation, result } => Q::BindingValue {
                invocation: invocation.clone(),
                result: Box::new(self.query_expr_normalized(symbol, &Self::result_only(result))?),
            },
            ExprKind::SemanticBlock {
                invocation,
                actions,
                result,
            } => Q::Block {
                invocation: invocation.clone(),
                actions: actions.clone(),
                result: Box::new(self.query_expr_normalized(symbol, result)?),
            },
            ExprKind::Local(name) => Q::Local(name.clone()),
            ExprKind::SemanticCapture(name) => Q::Capture(name.clone()),
            ExprKind::Lambda { parameter, body } => {
                fn has_effect(value: &Expr) -> bool {
                    match &value.kind {
                        ExprKind::Binary(_, left, right) => has_effect(left) || has_effect(right),
                        ExprKind::Match { inputs, cases } => {
                            inputs.iter().any(has_effect)
                                || cases.iter().any(|case| {
                                    has_effect(&case.value)
                                        || case
                                            .alternatives
                                            .iter()
                                            .flatten()
                                            .flatten()
                                            .any(has_effect)
                                })
                        }
                        ExprKind::SemanticBlock { .. } => true,
                        ExprKind::Call(_, args) => args.iter().any(|arg| has_effect(&arg.value)),
                        ExprKind::List(values) | ExprKind::Tuple(values) => {
                            values.iter().any(has_effect)
                        }
                        ExprKind::Group(value)
                        | ExprKind::Not(value)
                        | ExprKind::Index(value, _) => has_effect(value),
                        _ => false,
                    }
                }
                if has_effect(body) {
                    return Err(Diagnostic::error("yl.callback_effect", "Expression callbacks must be pure; invoke action-result operations outside the callback", expr.span.clone()));
                }
                Q::Lambda {
                    parameter: parameter.clone(),
                    body: Box::new(self.query_expr_normalized(symbol, body)?),
                }
            }
            ExprKind::Ref(name) if name == "self" => Q::SelfValue,
            ExprKind::Ref(name) if name == "true" || name == "false" => Q::Bool(name == "true"),
            ExprKind::Ref(name) if name == "absent" => Q::Absent,
            ExprKind::Ref(name) => {
                match self.resolve(&symbol.module, &symbol.name, name, &expr.span) {
                    Ok(id)
                        if self.symbols.get(&id).is_some_and(|s| {
                            matches!(s.declaration.kind, DeclKind::Symbol { .. })
                        }) || self.is_enum_variant(&id)
                            || self.registry.symbols.values().any(|value| value == &id) =>
                    {
                        Q::Symbol(id)
                    }
                    Ok(_) => {
                        return Err(Diagnostic::error(
                            "yl.invalid_query_value",
                            "Expected a capture or identity symbol",
                            expr.span.clone(),
                        ))
                    }
                    Err(_) => Q::Capture(name.clone()),
                }
            }
            ExprKind::EnumConstant(ty, variant) => {
                let parent = self.resolve(&symbol.module, &symbol.name, ty, &expr.span)?;
                let id = format!("{parent}::{variant}");
                if !self.is_enum_variant(&id) {
                    return Err(Diagnostic::error(
                        "yl.invalid_enum",
                        "Unknown enum variant",
                        expr.span.clone(),
                    ));
                }
                Q::Symbol(id)
            }
            ExprKind::Absent => Q::Absent,
            ExprKind::Bool(value) => Q::Bool(*value),
            ExprKind::Literal(value) => Q::Text(value.clone()),
            ExprKind::Group(value) => self.query_expr_normalized(symbol, value)?,
            ExprKind::Not(value) => Q::Not(Box::new(self.query_expr_normalized(symbol, value)?)),
            ExprKind::Index(value, index) => Q::Index {
                value: Box::new(self.query_expr_normalized(symbol, value)?),
                index: *index,
            },
            ExprKind::Tuple(values) => Q::Tuple(
                values
                    .iter()
                    .map(|value| self.query_expr_normalized(symbol, value))
                    .collect::<Result<Vec<_>>>()?,
            ),
            ExprKind::List(values) => Q::List(
                values
                    .iter()
                    .map(|value| self.query_expr_normalized(symbol, value))
                    .collect::<Result<Vec<_>>>()?,
            ),
            ExprKind::Call(name, args) if self.query_operations.contains(name) => Q::Invoke {
                name: name.clone(),
                args: args
                    .iter()
                    .map(|arg| self.query_expr_normalized(symbol, &arg.value))
                    .collect::<Result<_>>()?,
            },
            ExprKind::Match { inputs, cases } => {
                self.validate_value_match(symbol, inputs.len(), cases, &expr.span)?;
                Q::Match {
                    inputs: inputs
                        .iter()
                        .map(|value| self.query_expr_normalized(symbol, value))
                        .collect::<Result<_>>()?,
                    cases: cases
                        .iter()
                        .map(|case| {
                            Ok(crate::semantics::queries::QueryValueCase {
                                alternatives: case
                                    .alternatives
                                    .iter()
                                    .map(|row| {
                                        row.iter()
                                            .map(|value| {
                                                value
                                                    .as_ref()
                                                    .map(|value| {
                                                        self.query_expr_normalized(symbol, value)
                                                    })
                                                    .transpose()
                                            })
                                            .collect::<Result<_>>()
                                    })
                                    .collect::<Result<_>>()?,
                                value: self.query_expr_normalized(symbol, &case.value)?,
                            })
                        })
                        .collect::<Result<_>>()?,
                }
            }
            ExprKind::Call(name, args) => Q::Call {
                name: name.clone(),
                args: self.query_arguments(symbol, name, args, &expr.span)?,
            },
            _ => {
                return Err(Diagnostic::error(
                    "yl.invalid_query_value",
                    "Expected a declarative query value",
                    expr.span.clone(),
                ))
            }
        })
    }
}
