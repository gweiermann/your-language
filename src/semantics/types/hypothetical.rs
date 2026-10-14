//! Pure per-query type overlays. Only explicit addresses replace their inputs;
//! dependent relationships are recomputed without publishing overlay facts.
use super::*;
const QUERY: &str = "std/types#hypothetical";
pub(super) struct ResolveWith;
impl NativeOperation for ResolveWith {
    fn evaluate(&mut self, context: &mut OperationContext<'_>) -> Result<Evaluation, Diagnostic> {
        let result = syntax_key(context.argument("result").unwrap());
        let assumptions = context.argument("assumptions").unwrap().clone();
        let handle = SemanticValue::Tuple(vec![
            SemanticValue::Symbol(QUERY.into()),
            result,
            assumptions,
        ]);
        parse(&handle, context.operation_span)?;
        Ok(Evaluation::Ready(SemanticValue::Deferred {
            service: MODULE.into(),
            key: Box::new(handle),
            span: context.operation_span.clone(),
        }))
    }
}
pub(super) fn parse(value: &SemanticValue, span: &Span) -> Result<Option<Expr>, Diagnostic> {
    let SemanticValue::Tuple(parts) = value else {
        return Ok(None);
    };
    if parts.first() != Some(&SemanticValue::Symbol(QUERY.into())) {
        return Ok(None);
    }
    let fail = || {
        Diagnostic::error(
            "types.assumptions",
            "Expected a result key and a list of (key, temporary type) assumptions",
            span.clone(),
        )
    };
    if parts.len() != 3 {
        return Err(fail());
    }
    let result = Expr::read(&parts[1], span)?;
    let SemanticValue::List(values) = &parts[2] else {
        return Err(fail());
    };
    let mut assumptions = vec![];
    let mut seen = std::collections::BTreeSet::new();
    for value in values {
        let SemanticValue::Tuple(pair) = value else {
            return Err(fail());
        };
        if pair.len() != 2 {
            return Err(fail());
        }
        let target = destination(&pair[0], span)?;
        for key in Possibilities::destination_keys(&target) {
            if !seen.insert(key) {
                return Err(Diagnostic::error(
                    "types.assumptions",
                    "A hypothetical key cannot be assigned twice",
                    span.clone(),
                ));
            }
        }
        assumptions.push((target, Expr::parse(&pair[1], span)?));
    }
    Ok(Some(Expr::Hypothetical(
        Box::new(result),
        assumptions,
        span.clone(),
    )))
}
pub(super) fn evaluate(
    state: &Possibilities,
    result: &Expr,
    assumptions: &[(Destination, Expr)],
    span: &Span,
) -> Result<Vec<Fact>, Diagnostic> {
    calculate(state, result, assumptions, span).map(|(facts, _)| facts)
}
pub(super) fn diagnostics(
    state: &Possibilities,
    expr: &Expr,
) -> Result<Vec<Diagnostic>, Diagnostic> {
    match expr {
        Expr::Hypothetical(result, assumptions, span) => {
            calculate(state, result, assumptions, span).map(|(_, diagnostics)| diagnostics)
        }
        Expr::List(values, _) => {
            let mut output = vec![];
            for value in values {
                output.extend(diagnostics(state, value)?);
            }
            Ok(output)
        }
        _ => Ok(vec![]),
    }
}
fn calculate(
    state: &Possibilities,
    result: &Expr,
    assumptions: &[(Destination, Expr)],
    span: &Span,
) -> Result<(Vec<Fact>, Vec<Diagnostic>), Diagnostic> {
    if state.hypothetical_depth >= 32 {
        return Err(Diagnostic::error(
            "types.hypothetical_cycle",
            "Hypothetical resolution contains recursive queries",
            span.clone(),
        ));
    }
    let mut needed = std::collections::BTreeSet::new();
    let mut dependencies = vec![];
    result.dependencies(0, &mut dependencies);
    needed.extend(dependencies.into_iter().map(|(key, _)| key));
    loop {
        let before = needed.len();
        for (source, target, _) in &state.edges {
            if needed.contains(target) {
                needed.insert(source.clone());
            }
        }
        for transfer in &state.transfers {
            if Possibilities::destination_keys(&transfer.destination)
                .iter()
                .any(|key| needed.contains(key))
            {
                let mut dependencies = vec![];
                transfer.value.dependencies(0, &mut dependencies);
                needed.extend(dependencies.into_iter().map(|(key, _)| key));
            }
        }
        for matching in &state.residual_matches {
            if matching
                .cases
                .iter()
                .flat_map(|case| &case.transfers)
                .any(|transfer| {
                    Possibilities::destination_keys(&transfer.destination)
                        .iter()
                        .any(|key| needed.contains(key))
                })
            {
                for input in &matching.inputs {
                    let mut dependencies = vec![];
                    input.dependencies(0, &mut dependencies);
                    needed.extend(dependencies.into_iter().map(|(key, _)| key));
                }
                for transfer in matching.cases.iter().flat_map(|case| &case.transfers) {
                    let mut dependencies = vec![];
                    transfer.value.dependencies(0, &mut dependencies);
                    needed.extend(dependencies.into_iter().map(|(key, _)| key));
                }
            }
        }
        if before == needed.len() {
            break;
        }
    }
    let mut values = vec![];
    for (target, expr) in assumptions {
        let facts = state.eval(expr, &mut Default::default())?;
        if facts.is_empty() {
            return Ok((vec![], vec![]));
        }
        values.push((target.clone(), facts));
    }
    let signature = (
        result.clone(),
        values
            .iter()
            .map(|(target, facts)| {
                (
                    target.clone(),
                    facts.iter().map(|fact| fact.member.clone()).collect(),
                )
            })
            .collect(),
    );
    if state.hypothetical_stack.contains(&signature) {
        return Ok((state.eval(result, &mut Default::default())?, vec![]));
    }
    let mut stack = state.hypothetical_stack.clone();
    stack.push(signature);
    let overrides: std::collections::BTreeSet<_> = assumptions
        .iter()
        .flat_map(|(target, _)| Possibilities::destination_keys(target))
        .collect();
    let mut affected = overrides.clone();
    loop {
        let old = affected.len();
        for (source, target, _) in &state.edges {
            if affected.contains(source) {
                affected.insert(target.clone());
            }
        }
        if affected.len() == old {
            break;
        }
        if affected.len() > LIMIT {
            return Err(Diagnostic::error(
                "types.complexity",
                "Hypothetical dependencies exceeded the analysis limit",
                span.clone(),
            ));
        }
    }
    let match_rules: std::collections::BTreeSet<_> = state
        .residual_matches
        .iter()
        .flat_map(|matching| &matching.cases)
        .flat_map(|case| {
            case.transfers.iter().map(|transfer| {
                (
                    transfer.rule.file.clone(),
                    transfer.rule.start,
                    transfer.rule.end,
                )
            })
        })
        .collect();
    let mut local = Possibilities {
        facts: state
            .facts
            .iter()
            .map(|(key, facts)| {
                (
                    key.clone(),
                    if affected.contains(key) {
                        BTreeMap::new()
                    } else {
                        facts.clone()
                    },
                )
            })
            .collect(),
        transfers: state.transfers.clone(),
        restrictions: state.restrictions.clone(),
        forwarding: state.forwarding.clone(),
        edges: state.edges.clone(),
        settled: false,
        hypothetical_depth: state.hypothetical_depth + 1,
        hypothetical_stack: stack,
        residual_matches: state.residual_matches.clone(),
        ..Default::default()
    };
    for (target, facts) in values {
        for fact in facts {
            local.contribute(&target, fact)?;
        }
    }
    for _ in 0..LIMIT {
        let mut changed = false;
        let mut diagnostics = vec![];
        for transfer in &state.transfers {
            if match_rules.contains(&(
                transfer.rule.file.clone(),
                transfer.rule.start,
                transfer.rule.end,
            )) {
                continue;
            }
            if !Possibilities::destination_keys(&transfer.destination)
                .iter()
                .any(|key| needed.contains(key))
            {
                continue;
            }
            let facts = local.eval(&transfer.value, &mut Default::default())?;
            diagnostics.extend(self::diagnostics(&local, &transfer.value)?);
            for fact in facts {
                match &transfer.destination {
                    Destination::One(key) if overrides.contains(key) => {}
                    Destination::Positions(keys)
                        if keys.iter().any(|key| overrides.contains(key)) =>
                    {
                        if fact.member == Member::Unknown {
                            continue;
                        }
                        if keys.len() != fact.children.len() {
                            return Err(Diagnostic::error(
                                "types.mismatch",
                                "Hypothetical type list has incompatible arity",
                                span.clone(),
                            ));
                        }
                        for (key, child) in keys.iter().zip(fact.children) {
                            if !overrides.contains(key) {
                                changed |= local.insert(key, child)?
                            }
                        }
                    }
                    target => changed |= local.contribute(target, fact)?,
                }
            }
        }
        changed |= local.propagate_slots()?;
        for matching in &state.residual_matches {
            if !matching
                .cases
                .iter()
                .flat_map(|case| &case.transfers)
                .any(|transfer| {
                    Possibilities::destination_keys(&transfer.destination)
                        .iter()
                        .any(|key| needed.contains(key))
                })
            {
                continue;
            }
            let mut products = vec![vec![]];
            for input in &matching.inputs {
                let facts = local.eval(input, &mut Default::default())?;
                let mut next = vec![];
                for prefix in products {
                    for fact in &facts {
                        let mut tuple = prefix.clone();
                        tuple.push(fact.clone());
                        next.push(tuple);
                        if next.len() > LIMIT {
                            return Err(Diagnostic::error(
                                "types.complexity",
                                "Hypothetical match alternatives exceeded the analysis limit",
                                span.clone(),
                            ));
                        }
                    }
                }
                products = next;
            }
            for tuple in products {
                let selected = matching.cases.iter().find(|case| {
                    case.patterns.iter().any(|pattern| {
                        pattern.len() == tuple.len()
                            && pattern.iter().zip(&tuple).all(|(expected, fact)| {
                                expected
                                    .as_ref()
                                    .is_none_or(|expected| *expected == fact.member)
                            })
                    })
                });
                let Some(case) = selected else { continue };
                if case.effectful
                    || case.diagnostics.iter().any(|diagnostic| {
                        diagnostic.severity == crate::diagnostic::Severity::Warning
                    })
                {
                    return Err(Diagnostic::error("types.hypothetical_effect", "PARKED: Hypothetical type cases cannot replay native effects or warning diagnostics", matching.span.clone()));
                }
                for transfer in &case.transfers {
                    let facts = local.eval(&transfer.value, &mut Default::default())?;
                    for fact in facts {
                        changed |= local.contribute(&transfer.destination, fact)?;
                    }
                }
                for restriction in &case.restrictions {
                    if !local.restrictions.contains(restriction) {
                        local.restrictions.push(restriction.clone());
                        changed = true;
                    }
                }
                for diagnostic in &case.diagnostics {
                    let mut diagnostic = diagnostic.clone();
                    diagnostic.secondary.push((*diagnostic.primary).clone());
                    if let Some(fact) = tuple.first() {
                        diagnostic.primary = Box::new(fact.origin.clone());
                    }
                    if diagnostic.severity == crate::diagnostic::Severity::Error {
                        diagnostics.push(diagnostic);
                    }
                }
            }
        }
        if !changed {
            for restriction in &local.restrictions {
                let allowed = local.eval(&restriction.value, &mut Default::default())?;
                let keys = Possibilities::destination_keys(&restriction.destination);
                for key in keys.iter().filter(|key| affected.contains(*key)) {
                    let actual = local.snapshot(key)?;
                    // Positional contracts require the complete correlated row.
                    let actual = if matches!(restriction.destination, Destination::Positions(_)) {
                        local.eval(
                            &Expr::List(
                                keys.iter()
                                    .map(|key| Expr::Read(key.clone(), span.clone()))
                                    .collect(),
                                span.clone(),
                            ),
                            &mut Default::default(),
                        )?
                    } else {
                        actual
                    };
                    for fact in actual {
                        if !allowed.is_empty()
                            && !allowed
                                .iter()
                                .any(|bound| fact.member.compatible(&bound.member))
                        {
                            let origin = fact
                                .children
                                .iter()
                                .enumerate()
                                .find(|(index, child)| {
                                    !allowed.iter().any(|bound| {
                                        bound.children.get(*index).is_some_and(|expected| {
                                            child.member.compatible(&expected.member)
                                        })
                                    })
                                })
                                .map(|(_, child)| child.origin.clone())
                                .unwrap_or_else(|| fact.origin.clone());
                            let mut diagnostic = Diagnostic::error(
                                "types.mismatch",
                                format!(
                                    "Hypothetical type {} does not match {}",
                                    fact.member.display(),
                                    Possibilities::display(&allowed)
                                ),
                                origin,
                            );
                            diagnostic.help =
                                Some("The related location requires the incompatible type".into());
                            diagnostic.secondary.push(
                                allowed
                                    .first()
                                    .map(|fact| fact.origin.clone())
                                    .unwrap_or_else(|| restriction.rule.clone()),
                            );
                            return Err(diagnostic);
                        }
                    }
                }
            }
            return Ok((local.eval(result, &mut Default::default())?, diagnostics));
        }
    }
    Err(Diagnostic::error(
        "types.complexity",
        "Hypothetical propagation did not converge",
        span.clone(),
    ))
}

#[derive(Clone, PartialEq)]
pub(super) struct TypeMatch {
    inputs: Vec<Expr>,
    cases: Vec<TypeCase>,
    span: Span,
}
#[derive(Clone, PartialEq)]
struct TypeCase {
    patterns: Vec<Vec<Option<Member>>>,
    transfers: Vec<Transfer>,
    restrictions: Vec<Restriction>,
    diagnostics: Vec<Diagnostic>,
    effectful: bool,
}
pub(super) fn retain(
    state: &mut Possibilities,
    matching: &ResidualMatch,
) -> Result<(), Diagnostic> {
    let Ok(inputs) = matching
        .inputs
        .iter()
        .map(|value| Expr::parse(value, &matching.span))
        .collect::<Result<Vec<_>, _>>()
    else {
        return Ok(());
    };
    let mut cases = vec![];
    for case in &matching.cases {
        let mut patterns = vec![];
        for alternative in &case.alternatives {
            let mut pattern = vec![];
            for value in alternative {
                pattern.push(if let Some(value) = value {
                    let Ok(Expr::Constant(member, _)) = Expr::parse(value, &matching.span) else {
                        return Ok(());
                    };
                    Some(member)
                } else {
                    None
                });
            }
            patterns.push(pattern);
        }
        let mut retained = TypeCase {
            patterns,
            transfers: vec![],
            restrictions: vec![],
            diagnostics: vec![],
            effectful: false,
        };
        for action in &case.actions {
            match action {
                ResidualAction::Diagnostic(diagnostic) => {
                    retained.diagnostics.push(diagnostic.clone())
                }
                ResidualAction::Call {
                    operation,
                    arguments,
                    span,
                } if operation == "std/types#annotate" || operation == "std/types#restrictTo" => {
                    let target = destination(&arguments["key"], span)?;
                    let value = Expr::parse(&arguments["type"], span)?;
                    if operation == "std/types#annotate" {
                        retained.transfers.push(Transfer {
                            destination: target,
                            value,
                            span: span.clone(),
                            rule: span.clone(),
                        });
                    } else {
                        retained.restrictions.push(Restriction {
                            destination: target,
                            value,
                            span: span.clone(),
                            target_span: span.clone(),
                            rule: span.clone(),
                        });
                    }
                }
                // Effects outside this library are never replayed.
                _ => retained.effectful = true,
            }
        }
        cases.push(retained);
    }
    let retained = TypeMatch {
        inputs,
        cases,
        span: matching.span.clone(),
    };
    if state.residual_matches.contains(&retained) {
        return Ok(());
    }
    for case in &retained.cases {
        for transfer in &case.transfers {
            for input in &retained.inputs {
                state.connect(&transfer.destination, input, &retained.span)?;
            }
        }
    }
    state.residual_matches.push(retained);
    Ok(())
}
