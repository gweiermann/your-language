//! Token highlight ownership, including contextual wrapper and trivia spans.

use super::*;

impl Runtime<'_> {
    pub(super) fn highlight(&self, definition: &str, matched: &mut Match) {
        if let Some(category) = self.image.highlights.get(definition) {
            for token in &mut matched.highlights {
                token.token_type.clone_from(category);
            }
        }
    }
    pub(super) fn terminal_highlight(&self, matched: &mut Match, start: usize) {
        if self.highlighting && start < matched.end {
            matched.highlights.push(HighlightSpan {
                span: Span::new(self.file, start, matched.end),
                token_type: String::new(),
            });
        }
    }
    pub(super) fn highlight_trivia(&mut self, tokens: &mut Vec<HighlightSpan>) {
        if !self.highlighting {
            return;
        }
        // Classify trivia only in gaps of the retained parse, so speculative
        // branches and lookaround cannot leak tokens or duplicate comments.
        let mut cursor = 0;
        let mut gaps = Vec::new();
        for token in tokens.iter() {
            if cursor < token.span.start {
                gaps.push((cursor, token.span.start));
            }
            cursor = cursor.max(token.span.end);
        }
        if cursor < self.source.len() {
            gaps.push((cursor, self.source.len()));
        }
        for (start, end) in gaps {
            let mut cursor = start;
            while cursor < end {
                let found = self.image.trivia.iter().find_map(|id| {
                    self.rule(*id, cursor, true, 0)
                        .filter(|matched| cursor < matched.end && matched.end <= end)
                });
                let Some(matched) = found else { break };
                cursor = matched.end;
                tokens.extend(matched.highlights);
            }
        }
        tokens.sort_by_key(|token| (token.span.start, token.span.end));
    }
}
