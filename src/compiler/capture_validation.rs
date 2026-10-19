//! Semantic values and source capture validation.

use super::*;

pub(super) fn semantic_has_query(value: &MeaningValue) -> bool {
    match value {
        MeaningValue::Call(_) => true,
        MeaningValue::Tuple(values) | MeaningValue::List(values) => {
            values.iter().any(semantic_has_query)
        }
        _ => false,
    }
}
pub(super) fn semantic_has_context_query(value: &MeaningValue) -> bool {
    match value {
        MeaningValue::Call(call) => {
            call.contract.provides_context
                || call.arguments.values().any(semantic_has_context_query)
        }
        MeaningValue::Tuple(values) | MeaningValue::List(values) => {
            values.iter().any(semantic_has_context_query)
        }
        _ => false,
    }
}
pub(super) fn semantic_value_is_well_formed(value: &MeaningValue) -> bool {
    match value {
        MeaningValue::Capture(name) | MeaningValue::Symbol(name) => !name.is_empty(),
        MeaningValue::Tuple(values) | MeaningValue::List(values) => {
            values.iter().all(semantic_value_is_well_formed)
        }
        _ => true,
    }
}
pub(super) fn meaning_value_matches(value: &MeaningValue, kind: &ParameterKind) -> bool {
    match (value, kind) {
        (_, ParameterKind::Value) => semantic_value_is_well_formed(value),
        (MeaningValue::Call(_), _) => true,
        (MeaningValue::SelfValue, ParameterKind::Capture) => true,
        (MeaningValue::Capture(name), ParameterKind::Capture)
        | (MeaningValue::Symbol(name), ParameterKind::Symbol) => !name.is_empty(),
        (MeaningValue::Text(_), ParameterKind::Text)
        | (MeaningValue::Bool(_), ParameterKind::Bool) => true,
        _ => false,
    }
}

pub(super) fn symbol_has_capture(symbol: &DeclarationSymbol, name: &str) -> bool {
    let grammar = match &symbol.declaration.kind {
        DeclKind::Node { grammar, .. } => grammar.as_ref(),
        DeclKind::Pattern { grammar, .. } => Some(grammar),
        _ => None,
    };
    grammar.is_some_and(|grammar| syntax_has_capture(grammar, name))
}
pub(super) fn syntax_has_capture(expr: &Expr, name: &str) -> bool {
    match &expr.kind {
        ExprKind::Capture(capture, inner) => capture == name || syntax_has_capture(inner, name),
        ExprKind::Group(inner) | ExprKind::Repeat(inner, _) | ExprKind::Pipe(inner, _, _) => {
            syntax_has_capture(inner, name)
        }
        ExprKind::Sequence(values) | ExprKind::Choice(values) => {
            values.iter().any(|value| syntax_has_capture(value, name))
        }
        _ => false,
    }
}
pub(super) fn validate_semantic_capture_value(
    value: &MeaningValue,
    captures: &BTreeSet<String>,
    span: &Span,
) -> Result<()> {
    match value {
        MeaningValue::Capture(name)
            if !captures.contains(name.split('.').next().unwrap_or(name)) =>
        {
            return Err(Diagnostic::error(
                "yl.artifact",
                format!("Meaning argument references unavailable capture {name}"),
                span.clone(),
            ))
        }
        MeaningValue::Call(call) => {
            for value in call.arguments.values() {
                validate_semantic_capture_value(value, captures, &call.span)?;
            }
        }
        MeaningValue::Tuple(values) | MeaningValue::List(values) => {
            for value in values {
                validate_semantic_capture_value(value, captures, span)?;
            }
        }
        _ => {}
    }
    Ok(())
}
pub(crate) fn validate_meaning_captures(
    meanings: &MeaningDefinition,
    captures: &BTreeSet<String>,
) -> Result<()> {
    for call in meanings.groups.iter().flat_map(MeaningGroup::all_calls) {
        for value in call.arguments.values() {
            validate_semantic_capture_value(value, captures, &call.span)?;
        }
    }

    for group in &meanings.groups {
        if let Some(matching) = &group.matching {
            for case in &matching.cases {
                for action in &case.actions {
                    if let MeaningAction::Emit { message, span, .. } = action {
                        for part in message {
                            if let MeaningMessagePart::Capture(name) = part {
                                if !captures.contains(name) {
                                    return Err(Diagnostic::error(
                                        "yl.artifact",
                                        format!("Diagnostic references unavailable capture {name}"),
                                        span.clone(),
                                    ));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(())
}
