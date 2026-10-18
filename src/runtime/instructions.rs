//! Execution of indexed parser instructions and structural projections.

use super::*;

impl Runtime<'_> {
    pub(super) fn term(&mut self, term: u32, position: usize, raw: bool) -> Option<Match> {
        self.term_with(term, position, raw, None)
    }
    pub(super) fn term_with(
        &mut self,
        term_id: u32,
        position: usize,
        raw: bool,
        context: Option<&OperatorContext<'_>>,
    ) -> Option<Match> {
        self.partial_highlights.clear();
        // Parser recursion is bounded below, but debug frames hold transactional
        // matches and can exceed a Windows host thread's one-MiB stack first.
        // Grow only the native stack; grammar depth/work limits remain unchanged.
        let matched = stacker::maybe_grow(512 * 1024, 2 * 1024 * 1024, || {
            self.term_inner(term_id, position, raw, context)
        });
        if matched.is_some() {
            self.partial_highlights.clear();
        }
        matched
    }
    pub(super) fn term_inner(
        &mut self,
        term_id: u32,
        position: usize,
        raw: bool,
        context: Option<&OperatorContext<'_>>,
    ) -> Option<Match> {
        self.steps += 1;
        if self.steps > 1_000_000 {
            self.resource_limit = true;
            return None;
        }
        let image = self.image;
        let term = image.instructions.get(term_id as usize)?;
        // Lookaround is zero-width and must inspect the actual boundary, before trivia skipping.
        let start = if raw || matches!(term, Instruction::NotAhead(_) | Instruction::NotBehind(_)) {
            position
        } else {
            self.skip(position)
        };
        if let Some(context) = context {
            if let Some((leaf, left)) = context.left {
                if term_id == leaf {
                    let mut matched = left.clone();
                    matched.fields.clear();
                    matched.diagnostics.clear();
                    return Some(matched);
                }
            }
            if context.right.is_some_and(|leaf| term_id == leaf) {
                return self
                    .rule(context.family, start, raw, context.min_power)
                    .map(|mut matched| {
                        matched.fields.clear();
                        matched
                    });
            }
        }
        match term {
            Instruction::Example { definition, term } => {
                let Some(mut matched) = self.term_with(*term, start, raw, context) else {
                    if let Some(category) = image.highlights.get(definition) {
                        for token in &mut self.partial_highlights {
                            token.token_type.clone_from(category);
                        }
                    }
                    return None;
                };
                self.highlight(definition, &mut matched);
                Some(matched)
            }
            Instruction::Meaning { definition, term } => {
                let mut matched = self.term_with(*term, start, raw, context)?;
                self.trace_occurrence(definition, start, &mut matched);
                Some(matched)
            }
            Instruction::Literal(text) => {
                if self.source.get(start..self.limit)?.starts_with(text) {
                    let mut matched = Match::empty(start + text.len());
                    matched.value = Some(AstValue::Text(text.clone()));
                    matched.origin = Some(Span::new(self.file, start, matched.end));
                    self.terminal_highlight(&mut matched, start);
                    Some(matched)
                } else {
                    self.failure(start, &format!("{text:?}"));
                    None
                }
            }
            Instruction::Regex(pattern) => {
                if let Some((end, value)) = self.regex_match(pattern, start) {
                    let mut result = Match::empty(end);
                    result.value = Some(AstValue::Text(value));
                    result.origin = Some(Span::new(self.file, start, end));
                    self.terminal_highlight(&mut result, start);
                    Some(result)
                } else {
                    self.failure(start, &format!("/{pattern}/"));
                    None
                }
            }
            Instruction::Ref(id) => self.rule(*id, start, raw, 0).map(|mut m| {
                m.fields.clear();
                m
            }),
            Instruction::Sequence(terms) => {
                let mut result = Match::empty(start);
                let mut values = vec![];
                let mut origins = vec![];
                let mut depth = 0;
                for term in terms {
                    let Some(m) = self.term_with(*term, result.end, raw, context) else {
                        if self.highlighting {
                            result.highlights.append(&mut self.partial_highlights);
                            self.partial_highlights = result.highlights;
                        }
                        return None;
                    };
                    result.end = m.end;
                    if let Some(value) = m.value {
                        depth = depth.max(m.value_depth);
                        values.push(value);
                        origins.push(m.origin.clone());
                    }
                    result.fields.extend(m.fields);
                    merge_marked(&mut result.marked, m.marked);
                    result.diagnostics.extend(m.diagnostics);
                    result.occurrences.extend(m.occurrences);
                    result.highlights.extend(m.highlights);
                }
                result.value_depth =
                    self.check_value_depth(depth + usize::from(values.len() > 1), result.end)?;
                result.origin = if values.len() == 1 {
                    origins.into_iter().next().flatten()
                } else {
                    Some(Span::new(self.file, start, result.end))
                };
                result.value = collapse(values);
                Some(result)
            }
            Instruction::Choice(terms) => {
                let mut best = Vec::new();
                for term in terms {
                    if let Some(m) = self.term_with(*term, start, raw, context) {
                        return Some(m);
                    }
                    if self.partial_highlights.last().map(|token| token.span.end)
                        > best.last().map(|token: &HighlightSpan| token.span.end)
                    {
                        best = std::mem::take(&mut self.partial_highlights);
                    }
                }
                self.partial_highlights = best;
                None
            }
            Instruction::Repeat(term, q) => {
                let mut result = Match::empty(start);
                let mut values = vec![];
                let mut origins = vec![];
                let mut depth = 0;
                let mut count = 0;
                loop {
                    let attempt = if *q == Quantifier::Plus && count == 0 {
                        self.term_with(*term, result.end, raw, context)
                    } else {
                        self.continuation(result.end, raw, |parser| {
                            parser.term_with(*term, result.end, raw, context)
                        })
                    };
                    let Some(m) = attempt else { break };
                    if m.end == result.end {
                        break;
                    }
                    result.end = m.end;
                    count += 1;
                    if let Some(value) = m.value {
                        depth = depth.max(m.value_depth);
                        values.push(value);
                        result
                            .element_occurrences
                            .push(m.occurrences.iter().map(|o| o.id).collect());
                        origins.push(m.origin.clone());
                    }
                    result.fields.extend(m.fields);
                    merge_marked(&mut result.marked, m.marked);
                    result.diagnostics.extend(m.diagnostics);
                    result.occurrences.extend(m.occurrences);
                    result.highlights.extend(m.highlights);
                    if *q == Quantifier::Optional {
                        break;
                    }
                }
                if *q == Quantifier::Plus && count == 0 {
                    return None;
                }
                result.value_depth = self.check_value_depth(
                    depth + usize::from(*q != Quantifier::Optional),
                    result.end,
                )?;
                result.element_spans = origins.iter().flatten().cloned().collect();
                result.origin = if *q == Quantifier::Optional {
                    origins.into_iter().next().flatten()
                } else {
                    Some(Span::new(self.file, start, result.end))
                };
                result.value = Some(if *q == Quantifier::Optional {
                    values.into_iter().next().unwrap_or(AstValue::None)
                } else {
                    AstValue::List(values)
                });
                Some(result)
            }
            Instruction::Capture(name, term) => {
                let mut result = self.term_with(*term, start, raw, context)?;
                let value = result.value.clone().unwrap_or(AstValue::None);
                result.fields.insert(
                    name.clone(),
                    Captured {
                        element_occurrences: result.element_occurrences.clone(),
                        element_spans: result.element_spans.clone(),
                        repeated: image.capture_repeats(*term),
                        occurrences: result
                            .occurrences
                            .iter()
                            .map(|occurrence| occurrence.id)
                            .collect(),
                        value,
                        span: Span::new(self.file, start, result.end),
                        origin: result
                            .origin
                            .clone()
                            .unwrap_or_else(|| Span::new(self.file, start, result.end)),
                        depth: result.value_depth,
                    },
                );
                Some(result)
            }
            Instruction::NotAhead(term) => {
                let farthest = self.farthest;
                let expected = self.expected.clone();
                let limit = self.limit;
                self.limit = self.source.len();
                let matches = self.term(*term, start, true).is_some();
                self.limit = limit;
                self.farthest = farthest;
                self.expected = expected;
                if self.resource_limit {
                    return None;
                }
                if matches {
                    self.failure(start, "negative lookahead");
                    None
                } else {
                    Some(Match::empty(start))
                }
            }
            Instruction::NotBehind(term) => {
                let farthest = self.farthest;
                let expected = self.expected.clone();
                let mut matches = false;
                let limit = self.limit;
                self.limit = start;
                if let Instruction::Regex(pattern) = &image.instructions[*term as usize] {
                    // End-constrained regex matching also explores alternatives and
                    // lazy matches that a forward prefix search would stop too early.
                    matches = self
                        .regex_ending_at(pattern, start)
                        .is_some_and(|r| r.is_match(self.source));
                } else {
                    for offset in (0..=start).filter(|p| self.source.is_char_boundary(*p)) {
                        if self
                            .term(*term, offset, true)
                            .is_some_and(|m| m.end == start)
                        {
                            matches = true;
                            break;
                        }
                    }
                }
                self.limit = limit;
                self.farthest = farthest;
                self.expected = expected;
                if self.resource_limit {
                    return None;
                }
                if matches {
                    self.failure(start, "negative lookbehind");
                    None
                } else {
                    Some(Match::empty(start))
                }
            }
            Instruction::Mark { id, term } => {
                let mut matched = self.term_with(*term, start, raw, context)?;
                matched.marked.entry(*id).or_default().push(MarkedValue {
                    element_occurrences: matched.element_occurrences.clone(),
                    element_spans: matched.element_spans.clone(),
                    value: matched.value.clone(),
                    origin: matched.origin.clone().or_else(|| {
                        matched
                            .value
                            .as_ref()
                            .map(|_| Span::new(self.file, start, matched.end))
                    }),
                    fields: matched.fields.clone(),
                    depth: matched.value_depth,
                    occurrences: matched.occurrences.clone(),
                });
                Some(matched)
            }
            Instruction::Project {
                id,
                term,
                quantifier,
                collect_lists,
            } => {
                let mut matched = self.term_with(*term, start, raw, context)?;
                let originals = matched.marked.remove(id).unwrap_or_default();
                matched.fields.clear();
                matched.occurrences.clear();
                let mut values = vec![];
                let mut origins = vec![];
                let mut element_spans = vec![];
                let mut element_occurrences = vec![];
                let mut depth = 0;
                for original in originals {
                    if let Some(value) = original.value {
                        origins.push(original.origin.clone());
                        depth = depth.max(original.depth);
                        if *collect_lists {
                            if let AstValue::List(items) = value {
                                values.extend(items);
                                element_spans.extend(original.element_spans.clone());
                                element_occurrences.extend(original.element_occurrences.clone());
                            } else {
                                values.push(value);
                                element_spans.extend(original.origin.clone());
                                element_occurrences
                                    .push(original.occurrences.iter().map(|o| o.id).collect());
                            }
                        } else if value != AstValue::None {
                            values.push(value);
                            element_spans.extend(original.origin.clone());
                            element_occurrences
                                .push(original.occurrences.iter().map(|o| o.id).collect());
                        }
                    }
                    matched.fields.extend(original.fields);
                    matched.occurrences.extend(original.occurrences);
                }
                matched.value_depth =
                    self.check_value_depth(depth + usize::from(quantifier.is_some()), matched.end)?;
                matched.element_spans = element_spans;
                matched.element_occurrences = element_occurrences;
                matched.origin = if matches!(quantifier, Some(Quantifier::Star | Quantifier::Plus))
                {
                    let spans: Vec<_> = origins.into_iter().flatten().collect();
                    spans
                        .first()
                        .zip(spans.last())
                        .map(|(first, last)| Span::new(self.file, first.start, last.end))
                } else {
                    origins.into_iter().next().flatten()
                };
                matched.value = if matches!(quantifier, Some(Quantifier::Star | Quantifier::Plus)) {
                    Some(AstValue::List(values))
                } else {
                    Some(values.into_iter().next().unwrap_or(AstValue::None))
                };
                Some(matched)
            }
        }
    }
}
