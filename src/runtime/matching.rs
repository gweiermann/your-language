//! Backtracking continuations, regex terminals, trivia consumption, and rule dispatch.

use super::*;

impl Runtime<'_> {
    /// A continuation that can be omitted does not introduce an EOF requirement.
    /// Keep failures after a consumed prefix: an unfinished operator/iteration
    /// still needs its operand/item, even when backtracking accepts the prefix.
    pub(super) fn continuation(
        &mut self,
        position: usize,
        raw: bool,
        attempt: impl FnOnce(&mut Self) -> Option<Match>,
    ) -> Option<Match> {
        let start = if raw { position } else { self.skip(position) };
        let saved = (start == self.source.len()).then(|| (self.farthest, self.expected.clone()));
        let result = attempt(self);
        if let Some((farthest, expected)) = saved {
            self.farthest = farthest;
            self.expected = expected;
        }
        result
    }

    pub(super) fn failure(&mut self, position: usize, expected: &str) {
        if position > self.farthest {
            self.farthest = position;
            self.expected.clear();
        }
        if position == self.farthest {
            self.expected.insert(expected.into());
        }
    }
    pub(super) fn regex(&mut self, pattern: &str) -> Option<regex::Regex> {
        if let Some(re) = self.regexes.get(pattern) {
            return Some(re.clone());
        }
        let re = regex::Regex::new(pattern).ok()?;
        self.regexes.insert(pattern.into(), re.clone());
        Some(re)
    }
    pub(super) fn regex_match(&mut self, pattern: &str, start: usize) -> Option<(usize, String)> {
        let regex = self.regex(pattern)?;
        let matched = regex
            .find_at(self.source, start)
            .filter(|m| m.start() == start)?;
        if matched.end() <= self.limit {
            return Some((matched.end(), matched.as_str().into()));
        }
        // A greedy regex may cross the left-context bound. Constrain its end while
        // retaining the real suffix, so $ and word boundaries see the original source.
        // HIR printing removes inline flags/comments before safely composing the regex.
        for end in (start..=self.limit)
            .rev()
            .filter(|p| self.source.is_char_boundary(*p))
        {
            self.steps += 1;
            if self.steps > 1_000_000 {
                self.resource_limit = true;
                return None;
            }
            let bounded = self.regex_ending_at(pattern, end)?;
            if let Some(matched) = bounded
                .captures_at(self.source, start)
                .and_then(|c| c.get(1))
                .filter(|m| m.start() == start && m.end() == end)
            {
                return Some((end, matched.as_str().into()));
            }
        }
        None
    }
    pub(super) fn regex_ending_at(&mut self, pattern: &str, end: usize) -> Option<regex::Regex> {
        let normalized = regex_syntax::Parser::new().parse(pattern).ok()?.to_string();
        let suffix = regex::escape(self.source.get(end..)?);
        match regex::Regex::new(&format!("({normalized})(?:{suffix})\\z")) {
            Ok(regex) => Some(regex),
            Err(_) => {
                self.resource_limit = true;
                None
            }
        }
    }
    pub(super) fn skip(&mut self, position: usize) -> usize {
        let saved = std::mem::take(&mut self.partial_highlights);
        let end = self.skip_inner(position);
        self.partial_highlights = saved;
        end
    }
    fn skip_inner(&mut self, mut position: usize) -> usize {
        let farthest = self.farthest;
        let expected = self.expected.clone();
        loop {
            let mut end = position;
            for id in &self.image.trivia {
                if let Some(m) = self.rule(*id, position, true, 0) {
                    end = end.max(m.end);
                }
            }
            if end == position {
                self.farthest = farthest;
                self.expected = expected;
                return position;
            }
            position = end;
        }
    }
    pub(super) fn rule(
        &mut self,
        id: u32,
        position: usize,
        raw: bool,
        min_power: usize,
    ) -> Option<Match> {
        self.partial_highlights.clear();
        self.steps += 1;
        if self.depth >= 256 || self.steps > 1_000_000 {
            self.resource_limit = true;
            return None;
        }
        let image = self.image;
        let rule = image.rules.get(id as usize)?;
        let raw = raw || rule.trivia;
        let start = if raw { position } else { self.skip(position) };
        let key = (id, start, raw, min_power, self.limit);
        if !self.active.insert(key) {
            return None;
        }
        self.depth += 1;
        let mut result = match &rule.body {
            ImageBody::Concrete(term) => self
                .term(*term, start, raw)
                .and_then(|m| self.construct(&rule.definition, rule, start, m)),
            ImageBody::Abstract { bases, operators } => self
                .abstract_rule(id, bases, operators, start, raw, min_power)
                .map(|m| {
                    let mut m = self.checks(rule, start, m);
                    self.trace_occurrence(&rule.definition, start, &mut m);
                    m
                }),
        };
        if let Some(matched) = &mut result {
            self.highlight(&rule.definition, matched);
        } else if let Some(category) = image.highlights.get(&rule.definition) {
            for token in &mut self.partial_highlights {
                token.token_type.clone_from(category);
            }
        }
        self.depth -= 1;
        self.active.remove(&key);
        result
    }
}
