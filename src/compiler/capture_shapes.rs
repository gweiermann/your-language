//! Term capture analysis, projection cardinality, and value shape checks.

use super::*;

/// Return possible captures and captures guaranteed on every successful alternative.
/// `visiting` bounds recursive abstract-family traversal; absence of a guarantee is conservative.
pub(super) fn rule_capture_sets(
    rule: &Rule,
    language: &CompiledLanguage,
    visiting: &mut BTreeSet<String>,
) -> (BTreeSet<String>, BTreeSet<String>) {
    match &rule.body {
        RuleBody::Concrete(t) => capture_sets(t),
        RuleBody::Abstract { bases, operators } => {
            let mut possible = BTreeSet::new();
            let mut guaranteed: Option<BTreeSet<String>> = None;
            for id in bases.iter().chain(operators.iter().map(|o| &o.rule)) {
                if !visiting.insert(id.clone()) {
                    continue;
                }
                if let Some(rule) = language.rules.get(id) {
                    let (p, g) = rule_capture_sets(rule, language, visiting);
                    possible.extend(p);
                    guaranteed = Some(guaranteed.map_or_else(
                        || g.clone(),
                        |prev| prev.intersection(&g).cloned().collect(),
                    ));
                }
                visiting.remove(id);
            }
            (possible, guaranteed.unwrap_or_default())
        }
    }
}
pub(super) fn capture_sets(term: &Term) -> (BTreeSet<String>, BTreeSet<String>) {
    match term {
        Term::Capture(name, term) => {
            let (mut possible, mut guaranteed) = capture_sets(term);
            possible.insert(name.clone());
            if !may_be_none(term) {
                guaranteed.insert(name.clone());
            }
            (possible, guaranteed)
        }
        Term::Sequence(items) => items.iter().map(capture_sets).fold(
            (BTreeSet::new(), BTreeSet::new()),
            |(mut p, mut g), (a, b)| {
                p.extend(a);
                g.extend(b);
                (p, g)
            },
        ),
        Term::Choice(items) => {
            let mut p = BTreeSet::new();
            let mut g: Option<BTreeSet<String>> = None;
            for (a, b) in items.iter().map(capture_sets) {
                p.extend(a);
                g = Some(g.map_or_else(
                    || b.clone(),
                    |prev| prev.intersection(&b).cloned().collect(),
                ));
            }
            (p, g.unwrap_or_default())
        }
        Term::Repeat(term, q) => {
            let (p, g) = capture_sets(term);
            (
                p,
                if *q == Quantifier::Plus {
                    g
                } else {
                    BTreeSet::new()
                },
            )
        }
        Term::Mark { term, .. } | Term::Meaning { term, .. } | Term::Example { term, .. } => {
            capture_sets(term)
        }
        Term::Project {
            id,
            term,
            quantifier,
            ..
        } => {
            let (p, g) = find_marker(term, *id).map(capture_sets).unwrap_or_default();
            (
                p,
                if *quantifier == Some(Quantifier::Star) {
                    BTreeSet::new()
                } else {
                    g
                },
            )
        }
        _ => Default::default(),
    }
}
pub(super) fn may_be_none(term: &Term) -> bool {
    match term {
        Term::Repeat(_, Quantifier::Optional) | Term::NotAhead(_) | Term::NotBehind(_) => true,
        Term::Capture(_, term)
        | Term::Meaning { term, .. }
        | Term::Example { term, .. }
        | Term::Mark { term, .. } => may_be_none(term),
        Term::Project {
            id,
            term,
            quantifier: None,
            ..
        } => find_marker(term, *id).is_none_or(may_be_none),
        Term::Choice(items) => items.iter().any(may_be_none),
        _ => false,
    }
}

pub(super) fn validate_projections(
    term: &Term,
    active: &mut BTreeSet<u32>,
    span: &Span,
) -> Result<()> {
    match term {
        Term::Project {
            id,
            term,
            quantifier,
            collect_lists,
        } => {
            if !active.insert(*id) {
                return Err(Diagnostic::error(
                    "yl.artifact",
                    "Nested projection reuses its enclosing marker",
                    span.clone(),
                ));
            }
            if !valid_cardinality(
                *quantifier,
                marker_value_cardinality(term, *id, *collect_lists),
            ) {
                return Err(Diagnostic::error(
                    "yl.artifact",
                    "Projection output violates its cardinality",
                    span.clone(),
                ));
            }
            validate_projections(term, active, span)?;
            active.remove(id);
        }
        Term::Mark { id, term } => {
            if !active.contains(id) {
                return Err(Diagnostic::error(
                    "yl.artifact",
                    "Value marker is outside its projection",
                    span.clone(),
                ));
            }
            validate_projections(term, active, span)?;
        }
        Term::Sequence(items) | Term::Choice(items) => {
            for t in items {
                validate_projections(t, active, span)?;
            }
        }
        Term::Repeat(t, _)
        | Term::Meaning { term: t, .. }
        | Term::Example { term: t, .. }
        | Term::Capture(_, t)
        | Term::NotAhead(t)
        | Term::NotBehind(t) => validate_projections(t, active, span)?,
        _ => {}
    }
    Ok(())
}

pub(super) fn captures(term: &Term, span: &Span) -> Result<BTreeSet<String>> {
    let mut names = BTreeSet::new();
    match term {
        Term::Capture(name, inner) => {
            names = captures(inner, span)?;
            if !names.insert(name.clone()) {
                return Err(Diagnostic::error(
                    "yl.duplicate_member",
                    format!("Duplicate capture {name}"),
                    span.clone(),
                ));
            }
        }
        Term::Sequence(items) => {
            for item in items {
                for name in captures(item, span)? {
                    if !names.insert(name.clone()) {
                        return Err(Diagnostic::error(
                            "yl.duplicate_member",
                            format!("Duplicate capture {name}"),
                            span.clone(),
                        ));
                    }
                }
            }
        }
        Term::Choice(items) => {
            for item in items {
                names.extend(captures(item, span)?);
            }
        }
        Term::Repeat(inner, _) => names = captures(inner, span)?,
        Term::Mark { term, .. } | Term::Meaning { term, .. } | Term::Example { term, .. } => {
            names = captures(term, span)?
        }
        Term::Project { id, term, .. } => {
            if let Some(original) = find_marker(term, *id) {
                names = captures(original, span)?;
            }
        }
        _ => {}
    }
    Ok(names)
}

pub(super) fn find_marker(term: &Term, marker: u32) -> Option<&Term> {
    match term {
        Term::Mark { id, term } if *id == marker => Some(term),
        Term::Sequence(items) | Term::Choice(items) => {
            items.iter().find_map(|t| find_marker(t, marker))
        }
        Term::Repeat(t, _)
        | Term::Capture(_, t)
        | Term::Meaning { term: t, .. }
        | Term::Example { term: t, .. }
        | Term::Mark { term: t, .. }
        | Term::Project { term: t, .. } => find_marker(t, marker),
        _ => None,
    }
}
/// Counts binding uses on all successful paths, without choosing a scalar value.
pub(super) fn marker_value_cardinality(
    term: &Term,
    marker: u32,
    flatten: bool,
) -> (usize, Option<usize>) {
    match term {
        Term::Mark { id, term } if *id == marker => match value_shape(term) {
            Some(Quantifier::Optional) => (0, Some(1)),
            Some(Quantifier::Star) if flatten => (0, None),
            Some(Quantifier::Plus) if flatten => (1, None),
            _ => (1, Some(1)),
        },
        Term::Sequence(items) => items
            .iter()
            .map(|t| marker_value_cardinality(t, marker, flatten))
            .fold((0, Some(0)), |(min, max), (a, b)| {
                (
                    min.saturating_add(a),
                    max.zip(b).and_then(|(x, y)| x.checked_add(y)),
                )
            }),
        Term::Choice(items) => items
            .iter()
            .map(|t| marker_value_cardinality(t, marker, flatten))
            .reduce(|(a, b), (c, d)| (a.min(c), b.zip(d).map(|(b, d)| b.max(d))))
            .unwrap_or((0, Some(0))),
        Term::Repeat(t, q) => {
            let (min, max) = marker_value_cardinality(t, marker, flatten);
            match q {
                Quantifier::Optional => (0, max),
                Quantifier::Star => (0, if max == Some(0) { Some(0) } else { None }),
                Quantifier::Plus => (min, if max == Some(0) { Some(0) } else { None }),
            }
        }
        Term::Capture(_, t)
        | Term::Meaning { term: t, .. }
        | Term::Example { term: t, .. }
        | Term::Mark { term: t, .. }
        | Term::Project { term: t, .. } => marker_value_cardinality(t, marker, flatten),
        _ => (0, Some(0)),
    }
}

pub(super) fn value_shape(term: &Term) -> Option<Quantifier> {
    match term {
        Term::Repeat(_, q) => Some(*q),
        Term::Capture(_, t)
        | Term::Meaning { term: t, .. }
        | Term::Example { term: t, .. }
        | Term::Mark { term: t, .. } => value_shape(t),
        Term::Project { quantifier, .. } => *quantifier,
        _ => None,
    }
}
pub(super) fn valid_cardinality(
    quantifier: Option<Quantifier>,
    (min, max): (usize, Option<usize>),
) -> bool {
    match quantifier {
        None => min == 1 && max == Some(1),
        Some(Quantifier::Optional) => max.is_some_and(|n| n <= 1),
        Some(Quantifier::Star) => true,
        Some(Quantifier::Plus) => min >= 1,
    }
}

pub(super) fn capture_terms<'a>(
    rule: &'a Rule,
    language: &'a CompiledLanguage,
    name: &str,
    visiting: &mut BTreeSet<String>,
) -> Vec<&'a Term> {
    fn walk<'a>(term: &'a Term, name: &str) -> Vec<&'a Term> {
        match term {
            Term::Capture(n, inner) if n == name => vec![inner],
            Term::Sequence(items) | Term::Choice(items) => {
                items.iter().flat_map(|t| walk(t, name)).collect()
            }
            Term::Capture(_, inner)
            | Term::Repeat(inner, _)
            | Term::Meaning { term: inner, .. }
            | Term::Example { term: inner, .. }
            | Term::Mark { term: inner, .. } => walk(inner, name),
            Term::Project { id, term, .. } => find_marker(term, *id)
                .map(|t| walk(t, name))
                .unwrap_or_default(),
            _ => vec![],
        }
    }
    match &rule.body {
        RuleBody::Concrete(t) => walk(t, name),
        RuleBody::Abstract { bases, operators } => bases
            .iter()
            .chain(operators.iter().map(|o| &o.rule))
            .flat_map(|id| {
                if !visiting.insert(id.clone()) {
                    return vec![];
                }
                let result = language
                    .rules
                    .get(id)
                    .map(|r| capture_terms(r, language, name, visiting))
                    .unwrap_or_default();
                visiting.remove(id);
                result
            })
            .collect(),
    }
}
pub(super) fn string_term(term: &Term) -> bool {
    match term {
        Term::Example { term, .. } => string_term(term),
        Term::Literal(_) | Term::Regex(_) => true,
        Term::Repeat(t, Quantifier::Optional)
        | Term::Capture(_, t)
        | Term::Mark { term: t, .. } => string_term(t),
        Term::Choice(items) => items.iter().all(string_term),
        Term::Project {
            id,
            term,
            quantifier: None | Some(Quantifier::Optional),
            ..
        } => find_marker(term, *id).is_some_and(string_term),
        _ => false,
    }
}
pub(super) fn node_term(
    term: &Term,
    expected: &str,
    language: &CompiledLanguage,
    visiting: &mut BTreeSet<String>,
) -> bool {
    match term {
        Term::Ref(id) if id == expected => true,
        Term::Ref(id) => {
            if !visiting.insert(expected.into()) {
                return false;
            }
            let result = language.rules.get(expected).is_some_and(|r| match &r.body {
                RuleBody::Abstract { bases, operators } => bases
                    .iter()
                    .chain(operators.iter().map(|o| &o.rule))
                    .any(|member| node_term(&Term::Ref(id.clone()), member, language, visiting)),
                _ => false,
            });
            visiting.remove(expected);
            result
        }
        Term::Capture(_, t)
        | Term::Meaning { term: t, .. }
        | Term::Example { term: t, .. }
        | Term::Mark { term: t, .. } => node_term(t, expected, language, visiting),
        Term::Choice(items) => items
            .iter()
            .all(|t| node_term(t, expected, language, visiting)),
        Term::Project {
            id,
            term,
            quantifier: None,
            ..
        } => find_marker(term, *id).is_some_and(|t| node_term(t, expected, language, visiting)),
        _ => false,
    }
}
