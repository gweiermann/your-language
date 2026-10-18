//! Bounded semantic occurrence tracing and concrete AST construction.

use super::*;

impl Runtime<'_> {
    pub(super) fn check_value_depth(&mut self, depth: usize, end: usize) -> Option<usize> {
        if depth > 128 {
            self.resource_limit = true;
            self.failure(end, "AST value within resource limit");
            None
        } else {
            Some(depth)
        }
    }
    pub(super) fn trace_occurrence(&mut self, definition: &str, start: usize, matched: &mut Match) {
        if !self.trace {
            return;
        }
        let captures = matched
            .fields
            .iter()
            .map(|(name, capture)| {
                (
                    name.clone(),
                    crate::semantics::SourceCapture {
                        element_occurrences: capture.element_occurrences.clone(),
                        element_spans: capture.element_spans.clone(),
                        selection: None,
                        repeated: capture.repeated,
                        value: capture.value.clone(),
                        occurrences: capture.occurrences.clone(),
                        span: capture.origin.clone(),
                        text: self
                            .source
                            .get(capture.origin.start..capture.origin.end)
                            .unwrap_or_default()
                            .into(),
                    },
                )
            })
            .collect();
        let occurrence = crate::semantics::Occurrence {
            id: self.next_occurrence,
            definition: definition.into(),
            span: Span::new(self.file, start, matched.end),
            captures,
            children: std::mem::take(&mut matched.occurrences),
        };
        self.next_occurrence += 1;
        matched.occurrences.push(occurrence);
    }
    pub(super) fn construct(
        &mut self,
        id: &str,
        rule: &ImageRule,
        start: usize,
        matched: Match,
    ) -> Option<Match> {
        let mut matched = self.checks(rule, start, matched);
        matched.value_depth = self.check_value_depth(
            matched.fields.values().map(|f| f.depth).max().unwrap_or(0) + 1,
            matched.end,
        )?;
        self.trace_occurrence(id, start, &mut matched);
        self.highlight(id, &mut matched);
        let fields = matched
            .fields
            .clone()
            .into_iter()
            .map(|(name, capture)| (name, capture.value))
            .collect();
        matched.origin = Some(Span::new(self.file, start, matched.end));
        matched.value = Some(AstValue::Node(Box::new(AstNode {
            kind: rule.name.clone(),
            fields,
            span: Span::new(self.file, start, matched.end),
        })));
        Some(matched)
    }
}
