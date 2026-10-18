//! Semantic group and ordering selector normalization.

use super::*;

impl Compiler {
    pub(super) fn lower_meanings(
        &self,
        symbol: &DeclarationSymbol,
        meanings: &Meanings,
    ) -> Result<MeaningDefinition> {
        let mut groups = vec![];
        let mut names = BTreeSet::new();
        for group in &meanings.groups {
            if self.is_query_group(symbol, group)? {
                groups.push(MeaningGroup {
                    name: group.name.clone(),
                    calls: vec![],
                    matching: None,
                    span: group.span.clone(),
                });
                continue;
            }
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
            let calls = group
                .calls
                .iter()
                .map(|call| self.lower_meaning_actions(symbol, call))
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .flatten()
                .collect();
            let matching = group
                .matching
                .as_ref()
                .map(|matching| self.lower_meaning_match(symbol, matching))
                .transpose()?;
            groups.push(MeaningGroup {
                name: group.name.clone(),
                calls,
                matching,
                span: group.span.clone(),
            });
        }
        let mut precedence = vec![];
        for chain in &meanings.precedence {
            let mut selectors = vec![];
            for selector in &chain.selectors {
                let ExprKind::Ref(name) = &selector.kind else {
                    return Err(Diagnostic::error(
                        "yl.invalid_meaning_selector",
                        "Expected a definition selector",
                        selector.span.clone(),
                    ));
                };
                if name == "_" {
                    return Err(Diagnostic::error(
                        "yl.invalid_meaning_selector",
                        "Wildcard meaning selectors are not supported",
                        selector.span.clone(),
                    ));
                }
                let (definition, group) = name
                    .split_once("::semantics::")
                    .map_or((name.as_str(), None), |(definition, group)| {
                        (definition, Some(group.to_owned()))
                    });
                let definition =
                    self.resolve(&symbol.module, &symbol.name, definition, &selector.span)?;
                let target = self.symbols.get(&definition).ok_or_else(|| {
                    Diagnostic::error(
                        "yl.invalid_meaning_selector",
                        "Meaning selectors require a node or pattern",
                        selector.span.clone(),
                    )
                })?;
                let target_meanings = match &target.declaration.kind {
                    DeclKind::Node { meanings, .. } | DeclKind::Pattern { meanings, .. } => {
                        meanings
                    }
                    _ => {
                        return Err(Diagnostic::error(
                            "yl.invalid_meaning_selector",
                            "Meaning selectors require a node or pattern",
                            selector.span.clone(),
                        ))
                    }
                };
                if let Some(group) = &group {
                    if !target_meanings.as_ref().is_some_and(|meanings| {
                        meanings
                            .groups
                            .iter()
                            .any(|candidate| candidate.name.as_ref() == Some(group))
                    }) {
                        return Err(Diagnostic::error(
                            "yl.unknown_meaning_group",
                            format!("Unknown meaning group {group}"),
                            selector.span.clone(),
                        ));
                    }
                }
                selectors.push(MeaningSelector { definition, group });
            }
            precedence.push(MeaningChain {
                selectors,
                span: chain.span.clone(),
            });
        }
        Ok(MeaningDefinition {
            boundary: meanings.boundary,
            groups,
            precedence,
        })
    }
}
