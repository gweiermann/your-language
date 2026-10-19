//! Normalized language and native semantic artifact validation.

use super::*;

pub(crate) fn reject_retired_standard_operation(id: &str, span: &Span) -> Result<()> {
    if id.starts_with("std/variables#")
        || id.starts_with("std/references#")
        || id.starts_with("std/types#")
    {
        return Err(Diagnostic::error("yl.artifact_retired_std",
            format!("Native standard operation {id} has been retired; recompile the language against the authored standard modules"),
            span.clone()));
    }
    Ok(())
}
pub(crate) fn validate_language(language: &CompiledLanguage) -> Result<()> {
    if let Some(image) = &language.image {
        return image.validate();
    }
    if !matches!(language.version, 1..=5)
        || (language.version < 3 && !language.query_meanings.is_empty())
        || (language.version == 1
            && (!language.meanings.is_empty() || !language.query_meanings.is_empty()))
        || !language.rules.contains_key(&language.entry)
    {
        return Err(Diagnostic::error(
            "yl.artifact",
            "Invalid artifact version or entry rule",
            Span::default(),
        ));
    }
    fn term(t: &Term, language: &CompiledLanguage, span: &Span, depth: usize) -> Result<()> {
        if depth > 128 {
            return Err(Diagnostic::error(
                "yl.artifact",
                "Artifact term nesting exceeds resource limit",
                span.clone(),
            ));
        }
        match t {
            Term::Example {
                definition,
                term: inner,
            } => {
                if !language.examples.contains_key(definition)
                    && !language.highlights.contains_key(definition)
                {
                    return Err(Diagnostic::error(
                        "yl.artifact",
                        "Unknown example metadata reference",
                        span.clone(),
                    ));
                }
                term(inner, language, span, depth + 1)
            }
            Term::Meaning {
                definition,
                term: inner,
            } => {
                let meanings = language.meanings.get(definition).ok_or_else(|| {
                    Diagnostic::error(
                        "yl.artifact",
                        format!("Unknown meaning boundary {definition}"),
                        span.clone(),
                    )
                })?;
                validate_meaning_captures(meanings, &capture_sets(inner).0)?;
                if let Some(groups) = language.query_meanings.get(definition) {
                    crate::semantics::queries::validate_groups(
                        groups,
                        &capture_sets(inner).0,
                        language.version,
                    )?;
                }
                term(inner, language, span, depth + 1)
            }
            Term::Ref(id) if !language.rules.contains_key(id) => Err(Diagnostic::error(
                "yl.artifact",
                format!("Unknown rule {id}"),
                span.clone(),
            )),
            Term::Regex(s) => regex::Regex::new(s)
                .map(|_| ())
                .map_err(|e| Diagnostic::error("yl.invalid_regex", e.to_string(), span.clone())),
            Term::Sequence(items) | Term::Choice(items) => {
                for t in items {
                    term(t, language, span, depth + 1)?;
                }
                Ok(())
            }
            Term::Repeat(t, _)
            | Term::Capture(_, t)
            | Term::NotAhead(t)
            | Term::NotBehind(t)
            | Term::Mark { term: t, .. }
            | Term::Project { term: t, .. } => term(t, language, span, depth + 1),
            _ => Ok(()),
        }
    }
    for (family, rule) in &language.rules {
        if let RuleBody::Concrete(t) = &rule.body {
            validate_projections(t, &mut BTreeSet::new(), &rule.span)?;
        }
        match &rule.body {
            RuleBody::Concrete(t) => term(t, language, &rule.span, 0)?,
            RuleBody::Abstract { bases, operators } => {
                for id in bases.iter().chain(operators.iter().map(|o| &o.rule)) {
                    if !language.rules.contains_key(id) {
                        return Err(Diagnostic::error(
                            "yl.artifact",
                            "Unknown abstract member",
                            rule.span.clone(),
                        ));
                    }
                }
                let mut operator_rules = BTreeSet::new();
                for operator in operators {
                    let member = language.rules.get(&operator.rule).ok_or_else(|| {
                        Diagnostic::error(
                            "yl.artifact",
                            "Unknown precedence operator",
                            rule.span.clone(),
                        )
                    })?;
                    let RuleBody::Concrete(grammar) = &member.body else {
                        return Err(Diagnostic::error(
                            "yl.artifact",
                            "Precedence operator is not a concrete rule",
                            rule.span.clone(),
                        ));
                    };
                    let left = edge_ref(grammar, false) == Some(family.as_str());
                    let right = edge_ref(grammar, true) == Some(family.as_str());
                    if operator.binding_power == 0
                        || operator.binding_power > language.rules.len()
                        || (!left && !right)
                        || operator.left != left
                        || operator.right != right
                        || !operator_rules.insert(&operator.rule)
                    {
                        return Err(Diagnostic::error(
                            "yl.artifact",
                            "Invalid precedence binding power, role or duplicate operator",
                            rule.span.clone(),
                        ));
                    }
                }
            }
        }
        if let RuleBody::Concrete(t) = &rule.body {
            captures(t, &rule.span)?;
        }
        let (captures, guaranteed) = rule_capture_sets(rule, language, &mut BTreeSet::new());
        if let Some(meanings) = language.meanings.get(family) {
            validate_meaning_captures(meanings, &captures)?;
        }
        if let Some(groups) = language.query_meanings.get(family) {
            crate::semantics::queries::validate_groups(groups, &captures, language.version)?;
        }
        for check in &rule.constraints {
            let ty = validate_condition(
                &check.condition,
                &captures,
                &guaranteed,
                language,
                rule,
                &rule.span,
            )?;
            if !matches!(ty, ConditionType::Bool(_) | ConditionType::Absent) {
                return Err(Diagnostic::error(
                    "yl.condition_type",
                    "when requires a boolean or optional boolean",
                    rule.span.clone(),
                ));
            }
        }
    }
    for (owner, groups) in &language.query_meanings {
        let placeholders = language.meanings.get(owner).ok_or_else(|| {
            Diagnostic::error(
                "yl.artifact",
                "Query meanings have an unknown occurrence owner",
                Span::default(),
            )
        })?;
        let mut used = BTreeSet::new();
        for group in groups {
            let mut matching = placeholders
                .groups
                .iter()
                .enumerate()
                .filter(|(_, placeholder)| {
                    placeholder.name == group.name && placeholder.span == group.span
                });
            let Some((index, placeholder)) = matching.next() else {
                return Err(Diagnostic::error(
                    "yl.artifact",
                    "Query meaning group has no scheduling placeholder",
                    group.span.clone(),
                ));
            };
            if matching.next().is_some()
                || !used.insert(index)
                || !placeholder.calls.is_empty()
                || placeholder.matching.is_some()
            {
                return Err(Diagnostic::error(
                    "yl.artifact",
                    "Query meaning group has an invalid or ambiguous scheduling placeholder",
                    group.span.clone(),
                ));
            }
        }
    }
    for id in &language.trivia {
        if !language.rules.get(id).is_some_and(|r| r.trivia) {
            return Err(Diagnostic::error(
                "yl.artifact",
                "Invalid trivia rule",
                Span::default(),
            ));
        }
    }
    for (id, definition) in &language.examples {
        if language.rules.contains_key(id) && definition.term != Term::Ref(id.clone()) {
            return Err(Diagnostic::error(
                "yl.artifact",
                "Node example must reference its actual node rule",
                definition.span.clone(),
            ));
        }
        term(&definition.term, language, &definition.span, 0)?;
        validate_projections(&definition.term, &mut BTreeSet::new(), &definition.span)?;
        captures(&definition.term, &definition.span)?;
    }
    crate::examples::validate_explicit(language)?;
    validate_meaning_definitions(&language.meanings, |id| language.rules.contains_key(id))?;
    Ok(())
}
pub(crate) fn validate_meaning_definitions(
    meanings: &BTreeMap<String, MeaningDefinition>,
    contains_rule: impl Fn(&str) -> bool,
) -> Result<()> {
    for (owner, definition) in meanings {
        if owner.is_empty() {
            return Err(Diagnostic::error(
                "yl.artifact",
                "Empty meaning definition identity",
                Span::default(),
            ));
        }
        let mut groups = BTreeSet::new();
        for group in &definition.groups {
            if group
                .name
                .as_ref()
                .is_some_and(|name| !groups.insert(name.clone()))
                || group.name.as_ref().is_some_and(String::is_empty)
            {
                return Err(Diagnostic::error(
                    "yl.artifact",
                    "Duplicate or empty meaning group",
                    group.span.clone(),
                ));
            }
            if let Some(matching) = &group.matching {
                if matching.inputs.is_empty()
                    || matching.cases.is_empty()
                    || !matching.cases.last().is_some_and(|case| {
                        case.alternatives.iter().any(|alternative| {
                            alternative.len() == matching.inputs.len()
                                && alternative.iter().all(Option::is_none)
                        })
                    })
                {
                    return Err(Diagnostic::error(
                        "yl.artifact",
                        "Meaning match needs inputs and a final wildcard",
                        matching.span.clone(),
                    ));
                }
                if matching.inputs.iter().any(|value| matches!(value, MeaningValue::Capture(_)) || matches!(value, MeaningValue::Call(call) if call.contract.provides_context)) {
                    return Err(Diagnostic::error("yl.artifact", "Invalid meaning match input", matching.span.clone()));
                }
                for (index, case) in matching.cases.iter().enumerate() {
                    if case.alternatives.is_empty()
                        || case.alternatives.iter().any(|alternative| {
                            alternative.len() != matching.inputs.len()
                                || alternative.iter().flatten().any(|value| {
                                    !matches!(
                                        value,
                                        MeaningValue::Text(_)
                                            | MeaningValue::Bool(_)
                                            | MeaningValue::Symbol(_)
                                    )
                                })
                                || (index + 1 != matching.cases.len()
                                    && alternative.iter().all(Option::is_none))
                        })
                    {
                        return Err(Diagnostic::error(
                            "yl.artifact",
                            "Invalid meaning match patterns",
                            case.span.clone(),
                        ));
                    }
                    for action in &case.actions {
                        if let MeaningAction::Call(call) = action {
                            if call.contract.provides_context {
                                return Err(Diagnostic::error(
                                    "yl.artifact",
                                    "Deferred action provides a context",
                                    call.span.clone(),
                                ));
                            }
                        }
                    }
                }
            }
            for call in group.all_calls() {
                reject_retired_standard_operation(&call.operation, &call.span)?;
                if (call.contract.finalization_only || call.contract.contribution)
                    && call.contract.provides_context
                {
                    return Err(Diagnostic::error(
                        "yl.artifact",
                        "Finalization-only operations and contributions cannot provide lifecycle contexts",
                        call.span.clone(),
                    ));
                }
                if call.operation.is_empty() || call.operation != call.contract.id {
                    return Err(Diagnostic::error(
                        "yl.artifact",
                        "Meaning operation identity does not match its contract",
                        call.span.clone(),
                    ));
                }
                if call.arguments.values().any(semantic_has_context_query)
                    || (call.contract.provides_context
                        && call.arguments.values().any(semantic_has_query))
                {
                    return Err(Diagnostic::error(
                        "yl.artifact",
                        "Deferred calls cannot provide lifecycle contexts",
                        call.span.clone(),
                    ));
                }
                let mut parameters = BTreeSet::new();
                for parameter in &call.contract.parameters {
                    if parameter.name.is_empty() || !parameters.insert(parameter.name.clone()) {
                        return Err(Diagnostic::error(
                            "yl.artifact",
                            "Invalid operation parameter names",
                            call.span.clone(),
                        ));
                    }
                    if parameter.default.as_ref().is_some_and(|value| {
                        semantic_has_query(value) || !meaning_value_matches(value, &parameter.kind)
                    }) {
                        return Err(Diagnostic::error(
                            "yl.artifact",
                            "Operation parameter default has an incompatible type",
                            call.span.clone(),
                        ));
                    }
                    if !call
                        .arguments
                        .get(&parameter.name)
                        .is_some_and(|value| meaning_value_matches(value, &parameter.kind))
                    {
                        return Err(Diagnostic::error(
                            "yl.artifact",
                            "Missing or incompatible meaning argument",
                            call.span.clone(),
                        ));
                    }
                }
                if call.arguments.len() != parameters.len() {
                    return Err(Diagnostic::error(
                        "yl.artifact",
                        "Unknown meaning argument",
                        call.span.clone(),
                    ));
                }
            }
        }
        for chain in &definition.precedence {
            if chain.selectors.len() < 2 {
                return Err(Diagnostic::error(
                    "yl.artifact",
                    "Meaning precedence requires at least two selectors",
                    chain.span.clone(),
                ));
            }
            for selector in &chain.selectors {
                let target = meanings.get(&selector.definition);
                if selector.definition != "*"
                    && !contains_rule(&selector.definition)
                    && target.is_none()
                {
                    return Err(Diagnostic::error(
                        "yl.artifact",
                        "Unknown meaning precedence definition",
                        chain.span.clone(),
                    ));
                }
                if selector.group.as_ref().is_some_and(|name| {
                    !target.is_some_and(|definition| {
                        definition
                            .groups
                            .iter()
                            .any(|group| group.name.as_ref() == Some(name))
                    })
                }) {
                    return Err(Diagnostic::error(
                        "yl.artifact",
                        "Unknown selected meaning group",
                        chain.span.clone(),
                    ));
                }
            }
        }
    }
    Ok(())
}
