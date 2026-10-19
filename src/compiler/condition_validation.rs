//! Condition typing and optional capture presence guarantees.

use super::*;

#[derive(Clone, Debug, PartialEq)]
pub(super) enum ConditionType {
    Bool(bool),
    Text,
    Enum(String),
    Absent,
}
pub(super) fn condition_presence(condition: &Condition, truth: bool) -> BTreeSet<String> {
    match condition {
        Condition::Present(name) if truth => BTreeSet::from([name.clone()]),
        Condition::Not(inner) => condition_presence(inner, !truth),
        Condition::And(a, b) if truth => {
            let mut set = condition_presence(a, true);
            set.extend(condition_presence(b, true));
            set
        }
        Condition::Or(a, b) if !truth => {
            let mut set = condition_presence(a, false);
            set.extend(condition_presence(b, false));
            set
        }
        Condition::Equal(a, b) if !truth => match (a.as_ref(), b.as_ref()) {
            (Condition::CaptureValue(name), Condition::Absent)
            | (Condition::Absent, Condition::CaptureValue(name)) => BTreeSet::from([name.clone()]),
            _ => BTreeSet::new(),
        },
        Condition::OptionalMatches { capture, .. } if truth => BTreeSet::from([capture.clone()]),
        _ => BTreeSet::new(),
    }
}
pub(super) fn validate_condition(
    condition: &Condition,
    captures: &BTreeSet<String>,
    guaranteed: &BTreeSet<String>,
    language: &CompiledLanguage,
    rule: &Rule,
    span: &Span,
) -> Result<ConditionType> {
    let bad = |message: &str| Diagnostic::error("yl.condition_type", message, span.clone());
    let capture = |name: &String, required: bool| -> Result<()> {
        if !captures.contains(name) {
            return Err(Diagnostic::error(
                "yl.unknown_capture",
                format!("Constraint references unknown capture {name}"),
                span.clone(),
            ));
        }
        if required && !guaranteed.contains(name) {
            return Err(Diagnostic::error(
                "yl.optional_capture",
                format!("Capture {name} may be absent; use ?. or an isPresent guard"),
                span.clone(),
            ));
        }
        Ok(())
    };
    let validate =
        |c: &Condition| validate_condition(c, captures, guaranteed, language, rule, span);
    Ok(match condition {
        Condition::Always | Condition::Bool(_) => ConditionType::Bool(false),
        Condition::Text(_) => ConditionType::Text,
        Condition::Absent => ConditionType::Absent,
        Condition::EnumValue { ty, variant } => {
            if ty.is_empty() || variant.is_empty() {
                return Err(bad("Invalid normalized enum value"));
            }
            ConditionType::Enum(ty.clone())
        }
        Condition::CaptureValue(name) => {
            capture(name, false)?;
            let terms = capture_terms(rule, language, name, &mut BTreeSet::new());
            if terms.iter().any(|t| !string_term(t)) {
                return Err(bad("Equality supports scalar strings, booleans and enum variants; node/list values are not comparable"));
            }
            ConditionType::Text
        }
        Condition::CaptureType {
            capture: name,
            rule: expected,
        } => {
            capture(name, true)?;
            let terms = capture_terms(rule, language, name, &mut BTreeSet::new());
            if terms.is_empty()
                || terms
                    .iter()
                    .any(|t| !node_term(t, expected, language, &mut BTreeSet::new()))
            {
                return Err(Diagnostic::error(
                    "yl.invalid_typed_argument",
                    "Constraint capture does not produce the required node type",
                    span.clone(),
                ));
            }
            ConditionType::Bool(false)
        }
        Condition::Present(name) => {
            capture(name, false)?;
            ConditionType::Bool(false)
        }
        Condition::Matches {
            capture: name,
            regex,
        }
        | Condition::OptionalMatches {
            capture: name,
            regex,
        } => {
            let optional = matches!(condition, Condition::OptionalMatches { .. });
            capture(name, !optional)?;
            regex::Regex::new(regex)
                .map_err(|e| Diagnostic::error("yl.invalid_regex", e.to_string(), span.clone()))?;
            ConditionType::Bool(optional && !guaranteed.contains(name))
        }
        Condition::Between {
            trivia,
            left,
            right,
        } => {
            capture(left, true)?;
            capture(right, true)?;
            if let Some(id) = trivia {
                if !language.rules.get(id).is_some_and(|r| r.trivia) {
                    return Err(bad("between owner is not declared trivia"));
                }
            }
            ConditionType::Bool(false)
        }
        Condition::Not(inner) => {
            let t = validate(inner)?;
            if !matches!(t, ConditionType::Bool(_) | ConditionType::Absent) {
                return Err(bad("! requires a boolean or optional boolean"));
            }
            t
        }
        Condition::And(a, b) | Condition::Or(a, b) => {
            let left = validate(a)?;
            let mut known = guaranteed.clone();
            known.extend(condition_presence(
                a,
                matches!(condition, Condition::And(..)),
            ));
            let right = validate_condition(b, captures, &known, language, rule, span)?;
            let left = if left == ConditionType::Absent {
                ConditionType::Bool(true)
            } else {
                left
            };
            let right = if right == ConditionType::Absent {
                ConditionType::Bool(true)
            } else {
                right
            };
            let (ConditionType::Bool(a), ConditionType::Bool(b)) = (left, right) else {
                return Err(bad("Logical operators require boolean operands"));
            };
            ConditionType::Bool(a || b)
        }
        Condition::Equal(a, b) => {
            if let (Condition::CaptureValue(name), Condition::Absent)
            | (Condition::Absent, Condition::CaptureValue(name)) = (a.as_ref(), b.as_ref())
            {
                capture(name, false)?;
                return Ok(ConditionType::Bool(false));
            }
            let (a, b) = (validate(a)?, validate(b)?);
            let compatible = matches!(
                (&a, &b),
                (ConditionType::Absent, _)
                    | (_, ConditionType::Absent)
                    | (ConditionType::Bool(_), ConditionType::Bool(_))
            ) || a == b;
            if !compatible {
                return Err(bad("Equality operands must have matching types"));
            }
            ConditionType::Bool(false)
        }
    })
}
