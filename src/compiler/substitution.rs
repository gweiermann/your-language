//! Capture-safe substitution of grammar parameters and constraints.

use super::*;

pub(super) fn bind_constraint(
    parameters: &[Parameter],
    arguments: &[Argument],
    caller: &BTreeMap<String, Expr>,
    span: &Span,
) -> Result<BTreeMap<String, Expr>> {
    let mut supplied = BTreeMap::new();
    let mut position = 0;
    let mut named = false;
    for argument in arguments {
        let name = if let Some(name) = &argument.name {
            named = true;
            name.clone()
        } else {
            if named {
                return Err(Diagnostic::error(
                    "yl.invalid_argument",
                    "Positional argument after named argument",
                    argument.span.clone(),
                ));
            }
            let parameter = parameters.get(position).ok_or_else(|| {
                Diagnostic::error(
                    "yl.invalid_argument",
                    "Too many constraint arguments",
                    argument.span.clone(),
                )
            })?;
            position += 1;
            parameter.name.clone()
        };
        if !parameters.iter().any(|p| p.name == name)
            || supplied
                .insert(name, substitute(&argument.value, caller))
                .is_some()
        {
            return Err(Diagnostic::error(
                "yl.invalid_argument",
                "Unknown or duplicate constraint argument",
                argument.span.clone(),
            ));
        }
    }
    let mut bound = BTreeMap::new();
    for parameter in parameters {
        let value = if let Some(value) = supplied.remove(&parameter.name) {
            value
        } else if let Some(default) = &parameter.default {
            substitute(default, &bound)
        } else {
            return Err(Diagnostic::error(
                "yl.invalid_argument",
                format!("Missing constraint argument {}", parameter.name),
                span.clone(),
            ));
        };
        bound.insert(parameter.name.clone(), ungroup(&value).clone());
    }
    Ok(bound)
}
pub(super) fn ungroup(mut expr: &Expr) -> &Expr {
    while let ExprKind::Group(inner) = &expr.kind {
        expr = inner;
    }
    expr
}
pub(super) fn substitute(expr: &Expr, args: &BTreeMap<String, Expr>) -> Expr {
    if let ExprKind::Ref(name) = &expr.kind {
        if let Some(value) = args.get(name) {
            return value.clone();
        }
    }
    let mut result = expr.clone();
    if let ExprKind::Match { inputs, cases } = &mut result.kind {
        for input in inputs {
            *input = substitute(input, args);
        }
        for case in cases {
            for pattern in case.alternatives.iter_mut().flatten().flatten() {
                *pattern = substitute(pattern, args);
            }
            case.value = substitute(&case.value, args);
        }
    }
    if let ExprKind::Group(inner) = &mut result.kind {
        **inner = substitute(inner, args);
    }
    if let ExprKind::Not(inner) = &mut result.kind {
        **inner = substitute(inner, args);
    }
    if let ExprKind::Index(inner, _) = &mut result.kind {
        **inner = substitute(inner, args);
    }
    if let ExprKind::Binary(_, left, right) = &mut result.kind {
        **left = substitute(left, args);
        **right = substitute(right, args);
    }
    if let ExprKind::Call(name, values) = &mut result.kind {
        if let Some((receiver, method)) = name.rsplit_once('.') {
            let optional = receiver.ends_with('?');
            let receiver = receiver.trim_end_matches('?');
            if let Some(Expr {
                kind: ExprKind::Ref(actual),
                ..
            }) = args.get(receiver)
            {
                *name = format!("{actual}{}.{method}", if optional { "?" } else { "" });
            }
        }
        for value in values {
            value.value = substitute(&value.value, args);
        }
    }
    result
}
