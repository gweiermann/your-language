//! Own a parse attempt and publish its AST, source-linked occurrences, and diagnostics.
//! Example validation borrows the same executable tables without compiler state.

use super::*;

pub(super) fn parse_session(
    language: &CompiledLanguage,
    file: &str,
    source: &str,
    highlighting: bool,
) -> (
    ParseResult,
    Vec<crate::semantics::Occurrence>,
    Vec<HighlightSpan>,
) {
    let transient;
    let image = if let Some(image) = &language.image {
        &image.parser
    } else {
        transient = match ParserImage::compile(language) {
            Ok(image) => image,
            Err(diagnostic) => {
                return (
                    ParseResult {
                        ast: None,
                        diagnostics: vec![diagnostic],
                    },
                    vec![],
                    vec![],
                )
            }
        };
        &transient
    };
    let mut parser = Runtime {
        image,
        source,
        file,
        regexes: image.regexes.clone(),
        active: BTreeSet::new(),
        farthest: 0,
        expected: BTreeSet::new(),
        depth: 0,
        resource_limit: false,
        steps: 0,
        limit: source.len(),
        highlighting,
        partial_highlights: vec![],
        trace: !language.meanings.is_empty(),
        next_occurrence: 0,
    };
    let attempt = parser.rule(image.entry, 0, false, 0);
    if let Some(mut matched) = attempt {
        let end = parser.skip(matched.end);
        if end == source.len() && !parser.resource_limit {
            if let Some(AstValue::Node(node)) = matched.value {
                let mut next = 0;
                fn identify(
                    occurrence: &mut crate::semantics::Occurrence,
                    next: &mut usize,
                    identities: &mut BTreeMap<usize, usize>,
                ) {
                    identities.insert(occurrence.id, *next);
                    occurrence.id = *next;
                    *next += 1;
                    for child in &mut occurrence.children {
                        identify(child, next, identities);
                    }
                }
                let mut identities = BTreeMap::new();
                for occurrence in &mut matched.occurrences {
                    identify(occurrence, &mut next, &mut identities);
                }
                fn remap(
                    occurrence: &mut crate::semantics::Occurrence,
                    identities: &BTreeMap<usize, usize>,
                ) {
                    for capture in occurrence.captures.values_mut() {
                        for roots in &mut capture.element_occurrences {
                            *roots = roots
                                .iter()
                                .filter_map(|id| identities.get(id).copied())
                                .collect();
                        }
                        capture.occurrences = capture
                            .occurrences
                            .iter()
                            .filter_map(|id| identities.get(id).copied())
                            .collect();
                    }
                    for child in &mut occurrence.children {
                        remap(child, identities);
                    }
                }
                for occurrence in &mut matched.occurrences {
                    remap(occurrence, &identities);
                }
                // Allocate terminal addresses only in the retained successful tree.
                // This leaves node/pattern IDs and observable ASTs unchanged and
                // avoids identities from failed alternatives entering analysis.
                fn identify_terminals(
                    occurrence: &mut crate::semantics::Occurrence,
                    next: &mut usize,
                    addresses: &mut BTreeMap<TerminalAddress, usize>,
                ) {
                    use crate::semantics::OccurrenceSelection;
                    fn terminal(
                        value: &AstValue,
                        next: &mut usize,
                        address: (Span, String, usize),
                        addresses: &mut BTreeMap<TerminalAddress, usize>,
                    ) -> OccurrenceSelection {
                        if *value == AstValue::None {
                            OccurrenceSelection::Absent
                        } else {
                            let address = TerminalAddress {
                                file: address.0.file,
                                start: address.0.start,
                                end: address.0.end,
                                capture: address.1,
                                position: address.2,
                            };
                            let id = *addresses.entry(address).or_insert_with(|| {
                                let id = *next;
                                *next += 1;
                                id
                            });
                            OccurrenceSelection::Terminal(id)
                        }
                    }
                    for (name, capture) in &mut occurrence.captures {
                        if capture.selection.is_none()
                            && (capture.occurrences.is_empty() || capture.repeated)
                        {
                            capture.selection = Some(if capture.repeated {
                                match &capture.value {
                                    AstValue::List(items) => OccurrenceSelection::List(
                                        items
                                            .iter()
                                            .enumerate()
                                            .map(|(index, value)| {
                                                if let Some(roots) = capture
                                                    .element_occurrences
                                                    .get(index)
                                                    .filter(|roots| roots.len() == 1)
                                                {
                                                    OccurrenceSelection::One(roots[0])
                                                } else {
                                                    terminal(
                                                        value,
                                                        next,
                                                        (
                                                            capture
                                                                .element_spans
                                                                .get(index)
                                                                .cloned()
                                                                .unwrap_or_else(|| {
                                                                    capture.span.clone()
                                                                }),
                                                            name.clone(),
                                                            index,
                                                        ),
                                                        addresses,
                                                    )
                                                }
                                            })
                                            .collect(),
                                    ),
                                    value => terminal(
                                        value,
                                        next,
                                        (capture.span.clone(), name.clone(), 0),
                                        addresses,
                                    ),
                                }
                            } else {
                                terminal(
                                    &capture.value,
                                    next,
                                    (capture.span.clone(), name.clone(), 0),
                                    addresses,
                                )
                            });
                        }
                    }
                    for capture in occurrence.captures.values_mut() {
                        capture.element_occurrences.clear();
                    }
                    for child in &mut occurrence.children {
                        identify_terminals(child, next, addresses);
                    }
                }
                let mut next_terminal = 0;
                let mut terminal_addresses = BTreeMap::new();
                for occurrence in &mut matched.occurrences {
                    identify_terminals(occurrence, &mut next_terminal, &mut terminal_addresses);
                }
                parser.highlight_trivia(&mut matched.highlights);
                return (
                    ParseResult {
                        ast: Some(*node),
                        diagnostics: matched.diagnostics,
                    },
                    matched.occurrences,
                    matched
                        .highlights
                        .into_iter()
                        .filter(|token| !token.token_type.is_empty())
                        .collect(),
                );
            }
        } else {
            parser.failure(end, "end of input");
        }
        if highlighting {
            parser.partial_highlights = matched.highlights;
        }
        matched.diagnostics.clear();
    }
    let mut highlights = std::mem::take(&mut parser.partial_highlights);
    parser.highlight_trivia(&mut highlights);
    highlights.retain(|token| !token.token_type.is_empty());
    let start = parser.farthest.min(source.len());
    let end = source
        .get(start..)
        .and_then(|s| s.chars().next())
        .map_or(start, |c| start + c.len_utf8());
    let (code, message) = if parser.resource_limit {
        (
            "parse.resource_limit",
            "Parser resource limit reached".into(),
        )
    } else {
        (
            "parse.unexpected_token",
            format!(
                "Expected {}",
                parser.expected.into_iter().collect::<Vec<_>>().join(" or ")
            ),
        )
    };
    (
        ParseResult {
            ast: None,
            diagnostics: vec![Diagnostic::error(
                code,
                message,
                Span::new(file, start, end),
            )],
        },
        vec![],
        highlights,
    )
}
/// Validate representative text by borrowing executable tables, without a compiler workspace.
pub(crate) fn accepts_preview(image: &ParserImage, root: u32, source: &str) -> bool {
    let parser = || Runtime {
        image,
        source,
        file: "<example>",
        regexes: image.regexes.clone(),
        active: BTreeSet::new(),
        farthest: 0,
        expected: BTreeSet::new(),
        depth: 0,
        resource_limit: false,
        steps: 0,
        limit: source.len(),
        highlighting: false,
        partial_highlights: vec![],
        trace: true,
        next_occurrence: 0,
    };
    let mut direct = parser();
    if let Some(matched) = direct.term(root, 0, false) {
        if direct.skip(matched.end) == source.len()
            && !direct.resource_limit
            && !matched
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.severity == Severity::Error)
        {
            return true;
        }
    }
    let Instruction::Ref(member) = image.instructions[root as usize] else {
        return false;
    };
    for (family_id, family) in image.rules.iter().enumerate() {
        let ImageBody::Abstract { operators, .. } = &family.body else {
            continue;
        };
        if !operators.iter().any(|operator| operator.rule == member) {
            continue;
        }
        let mut contextual = parser();
        if let Some(matched) = contextual.rule(family_id as u32, 0, false, 0) {
            if contextual.skip(matched.end) == source.len()
                && !contextual.resource_limit
                && !matched
                    .diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.severity == Severity::Error)
                && matches!(matched.value, Some(AstValue::Node(ref node)) if node.kind == image.rules[member as usize].name)
                && matched.occurrences.iter().any(|root| {
                    root.definition == family.definition
                        && root.children.first().is_some_and(|child| {
                            child.definition == image.rules[member as usize].definition
                        })
                })
            {
                return true;
            }
        }
    }
    false
}
