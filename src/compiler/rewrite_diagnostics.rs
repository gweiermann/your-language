//! Diagnostic emissions in structural rewrite cases.

use super::*;

pub(super) fn validate_case_emission(emission: &Constraint) -> Result<()> {
    let ConstraintKind::Call(_name, args) = &emission.kind else {
        return Err(Diagnostic::error(
            "yl.invalid_constraint",
            "Expected diagnostic emission",
            emission.span.clone(),
        ));
    };
    let [argument] = args.as_slice() else {
        return Err(Diagnostic::error(
            "yl.invalid_argument",
            "Diagnostic takes one positional string",
            emission.span.clone(),
        ));
    };
    if argument.name.is_some() || !matches!(argument.value.kind, ExprKind::Literal(_)) {
        return Err(Diagnostic::error(
            "yl.invalid_argument",
            "Diagnostic takes one positional string",
            argument.span.clone(),
        ));
    }
    Ok(())
}
