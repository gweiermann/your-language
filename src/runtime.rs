//! Backtracking grammar interpreter with Pratt execution for recursive abstract families.
//! Every attempt owns its captures; failed alternatives cannot leak AST fields/diagnostics.
use crate::{
    compiler::{edge_ref, sequence},
    diagnostic::{Diagnostic, Severity, Span},
    ir::*,
    syntax::{Associativity, Quantifier},
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AstNode {
    #[serde(rename = "type")]
    pub kind: String,
    pub fields: BTreeMap<String, AstValue>,
    pub span: Span,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AstValue {
    Node(Box<AstNode>),
    Text(String),
    List(Vec<AstValue>),
    None,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ParseResult {
    pub ast: Option<AstNode>,
    pub diagnostics: Vec<Diagnostic>,
}
#[derive(Clone)]
struct Captured {
    value: AstValue,
    span: Span,
}
#[derive(Clone)]
struct Match {
    end: usize,
    value: Option<AstValue>,
    fields: BTreeMap<String, Captured>,
    diagnostics: Vec<Diagnostic>,
    marked: BTreeMap<u32, Vec<MarkedValue>>,
}
#[derive(Clone)]
struct MarkedValue {
    value: Option<AstValue>,
    fields: BTreeMap<String, Captured>,
}
impl Match {
    fn empty(end: usize) -> Self {
        Self {
            end,
            value: None,
            fields: BTreeMap::new(),
            diagnostics: vec![],
            marked: BTreeMap::new(),
        }
    }
}
pub fn parse(language: &CompiledLanguage, source: &str) -> ParseResult {
    parse_named(language, "<source>", source)
}
pub fn parse_named(language: &CompiledLanguage, file: &str, source: &str) -> ParseResult {
    let mut parser = Runtime {
        language,
        source,
        file,
        regexes: BTreeMap::new(),
        active: BTreeSet::new(),
        farthest: 0,
        expected: BTreeSet::new(),
        depth: 0,
        resource_limit: false,
        steps: 0,
        limit: source.len(),
    };
    let attempt = parser.rule(&language.entry, 0, false, 0);
    if let Some(mut matched) = attempt {
        let end = parser.skip(matched.end);
        if end == source.len() {
            if let Some(AstValue::Node(node)) = matched.value {
                return ParseResult {
                    ast: Some(*node),
                    diagnostics: matched.diagnostics,
                };
            }
        } else {
            parser.failure(end, "end of input");
        }
        matched.diagnostics.clear();
    }
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
    ParseResult {
        ast: None,
        diagnostics: vec![Diagnostic::error(
            code,
            message,
            Span::new(file, start, end),
        )],
    }
}
struct Runtime<'a> {
    language: &'a CompiledLanguage,
    source: &'a str,
    file: &'a str,
    regexes: BTreeMap<String, regex::Regex>,
    active: BTreeSet<(String, usize, bool, usize, usize)>,
    farthest: usize,
    expected: BTreeSet<String>,
    depth: usize,
    resource_limit: bool,
    steps: usize,
    /// Consumption bound for lookbehind; assertions still inspect the original source.
    limit: usize,
}
impl Runtime<'_> {
    fn failure(&mut self, position: usize, expected: &str) {
        if position > self.farthest {
            self.farthest = position;
            self.expected.clear();
        }
        if position == self.farthest {
            self.expected.insert(expected.into());
        }
    }
    fn regex(&mut self, pattern: &str) -> Option<regex::Regex> {
        if let Some(re) = self.regexes.get(pattern) {
            return Some(re.clone());
        }
        let re = regex::Regex::new(pattern).ok()?;
        self.regexes.insert(pattern.into(), re.clone());
        Some(re)
    }
    fn regex_match(&mut self, pattern: &str, start: usize) -> Option<(usize, String)> {
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
    fn regex_ending_at(&mut self, pattern: &str, end: usize) -> Option<regex::Regex> {
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
    fn skip(&mut self, mut position: usize) -> usize {
        let farthest = self.farthest;
        let expected = self.expected.clone();
        loop {
            let mut end = position;
            for id in &self.language.trivia {
                if let Some(m) = self.rule(id, position, true, 0) {
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
    fn rule(&mut self, id: &str, position: usize, raw: bool, min_power: usize) -> Option<Match> {
        self.steps += 1;
        if self.depth >= 256 || self.steps > 1_000_000 {
            self.resource_limit = true;
            return None;
        }
        let rule = self.language.rules.get(id)?.clone();
        let raw = raw || rule.trivia;
        let start = if raw { position } else { self.skip(position) };
        let key = (id.to_owned(), start, raw, min_power, self.limit);
        if !self.active.insert(key.clone()) {
            return None;
        }
        self.depth += 1;
        let result = match &rule.body {
            RuleBody::Concrete(term) => self
                .term(term, start, raw)
                .map(|m| self.construct(&rule, start, m)),
            RuleBody::Abstract { bases, operators } => self
                .abstract_rule(id, bases, operators, start, raw, min_power)
                .map(|m| self.checks(&rule, start, m)),
        };
        self.depth -= 1;
        self.active.remove(&key);
        result
    }
    fn checks(&mut self, rule: &Rule, start: usize, mut matched: Match) -> Match {
        for check in &rule.constraints {
            let satisfied = match &check.condition {
                Condition::Always => true,
                Condition::Matches { capture, regex } => {
                    matched.fields.get(capture).cloned().is_some_and(|f| {
                        self.regex(regex).is_some_and(|r| {
                            r.is_match(self.source.get(f.span.start..f.span.end).unwrap_or(""))
                        })
                    })
                }
                Condition::Between {
                    trivia,
                    left,
                    right,
                } => {
                    let a = matched.fields.get(left);
                    let b = matched.fields.get(right);
                    if let (Some(a), Some(b)) = (a, b) {
                        let (start, end) = (a.span.end, b.span.start);
                        if start >= end {
                            false
                        } else if let Some(id) = trivia {
                            let mut found = false;
                            for offset in (start..end).filter(|p| self.source.is_char_boundary(*p))
                            {
                                if self
                                    .rule(id, offset, true, 0)
                                    .is_some_and(|m| m.end <= end && m.end > offset)
                                {
                                    found = true;
                                    break;
                                }
                            }
                            found
                        } else {
                            self.skip(start) > start
                        }
                    } else {
                        false
                    }
                }
            };
            if satisfied {
                for emission in &check.emissions {
                    matched.diagnostics.push(Diagnostic {
                        severity: emission.severity.clone(),
                        code: "parse.constraint".into(),
                        message: emission.message.clone(),
                        primary: Box::new(Span::new(self.file, start, matched.end)),
                        secondary: vec![emission.span.clone()],
                        help: None,
                    });
                }
            }
        }
        matched
    }
    fn construct(&mut self, rule: &Rule, start: usize, matched: Match) -> Match {
        let mut matched = self.checks(rule, start, matched);
        let fields = matched
            .fields
            .clone()
            .into_iter()
            .map(|(name, capture)| (name, capture.value))
            .collect();
        matched.value = Some(AstValue::Node(Box::new(AstNode {
            kind: rule.name.clone(),
            fields,
            span: Span::new(self.file, start, matched.end),
        })));
        matched
    }
    fn abstract_rule(
        &mut self,
        id: &str,
        bases: &[String],
        operators: &[Operator],
        position: usize,
        raw: bool,
        min_power: usize,
    ) -> Option<Match> {
        let mut left = None;
        // Prefix alternatives compete with bases. Their recursive edge has the level's binding power.
        for op in operators
            .iter()
            .filter(|o| !o.left && o.right && o.binding_power >= min_power)
        {
            if let Some(m) = self.operator(id, op, position, raw, None) {
                left = Some(m);
                break;
            }
        }
        if left.is_none() {
            for base in bases {
                if let Some(m) = self.rule(base, position, raw, 0) {
                    left = Some(m);
                    break;
                }
            }
        }
        let mut left = left?;
        let mut nonassoc = None;
        loop {
            let mut next = None;
            for op in operators
                .iter()
                .filter(|o| o.left && o.binding_power >= min_power)
            {
                if nonassoc == Some(op.binding_power) {
                    continue;
                }
                if let Some(m) = self.operator(id, op, position, raw, Some(&left)) {
                    if m.end > left.end {
                        next = Some((op, m));
                        break;
                    }
                }
            }
            let Some((op, matched)) = next else {
                break;
            };
            if op.associativity == Associativity::Nonassoc {
                nonassoc = Some(op.binding_power);
            }
            left = matched;
        }
        Some(left)
    }
    fn operator(
        &mut self,
        family: &str,
        operator: &Operator,
        start: usize,
        raw: bool,
        left: Option<&Match>,
    ) -> Option<Match> {
        let rule = self.language.rules.get(&operator.rule)?.clone();
        let RuleBody::Concrete(term) = &rule.body else {
            return None;
        };
        let terms = sequence(term);
        let mut values = vec![];
        let mut result = Match::empty(start);
        for (index, term) in terms.iter().enumerate() {
            let matched = if index == 0 && operator.left {
                let mut m = left?.clone();
                m.fields.clear();
                m.diagnostics.clear();
                self.edge_capture(term, m, start)
            } else if index + 1 == terms.len() && operator.right && edge_ref(term) == Some(family) {
                let min = if operator.associativity == Associativity::Right || !operator.left {
                    operator.binding_power
                } else {
                    operator.binding_power.checked_add(1)?
                };
                let position = if raw {
                    result.end
                } else {
                    self.skip(result.end)
                };
                let m = self.rule(family, position, raw, min)?;
                self.edge_capture(term, m, position)
            } else {
                self.term(term, result.end, raw)?
            };
            result.end = matched.end;
            if let Some(v) = matched.value {
                values.push(v);
            }
            result.fields.extend(matched.fields);
            merge_marked(&mut result.marked, matched.marked);
            result.diagnostics.extend(matched.diagnostics);
        }
        if let Some(left) = left {
            result.diagnostics.splice(0..0, left.diagnostics.clone());
        }
        result.value = collapse(values);
        Some(self.construct(&rule, start, result))
    }
    fn edge_capture(&self, term: &Term, mut matched: Match, start: usize) -> Match {
        if let Term::Capture(name, inner) = term {
            matched = self.edge_capture(inner, matched, start);
            matched.fields.insert(
                name.clone(),
                Captured {
                    value: matched.value.clone().unwrap_or(AstValue::None),
                    span: Span::new(self.file, start, matched.end),
                },
            );
        }
        matched
    }
    fn term(&mut self, term: &Term, position: usize, raw: bool) -> Option<Match> {
        self.steps += 1;
        if self.steps > 1_000_000 {
            self.resource_limit = true;
            return None;
        }
        // Lookaround is zero-width and must inspect the actual boundary, before trivia skipping.
        let start = if raw || matches!(term, Term::NotAhead(_) | Term::NotBehind(_)) {
            position
        } else {
            self.skip(position)
        };
        match term {
            Term::Literal(text) => {
                if self.source.get(start..self.limit)?.starts_with(text) {
                    let mut matched = Match::empty(start + text.len());
                    matched.value = Some(AstValue::Text(text.clone()));
                    Some(matched)
                } else {
                    self.failure(start, &format!("{text:?}"));
                    None
                }
            }
            Term::Regex(pattern) => {
                if let Some((end, value)) = self.regex_match(pattern, start) {
                    let mut result = Match::empty(end);
                    result.value = Some(AstValue::Text(value));
                    Some(result)
                } else {
                    self.failure(start, &format!("/{pattern}/"));
                    None
                }
            }
            Term::Ref(id) => self.rule(id, start, raw, 0).map(|mut m| {
                m.fields.clear();
                m
            }),
            Term::Sequence(terms) => {
                let mut result = Match::empty(start);
                let mut values = vec![];
                for term in terms {
                    let m = self.term(term, result.end, raw)?;
                    result.end = m.end;
                    if let Some(value) = m.value {
                        values.push(value);
                    }
                    result.fields.extend(m.fields);
                    merge_marked(&mut result.marked, m.marked);
                    result.diagnostics.extend(m.diagnostics);
                }
                result.value = collapse(values);
                Some(result)
            }
            Term::Choice(terms) => {
                for term in terms {
                    if let Some(m) = self.term(term, start, raw) {
                        return Some(m);
                    }
                }
                None
            }
            Term::Repeat(term, q) => {
                let mut result = Match::empty(start);
                let mut values = vec![];
                let mut count = 0;
                while let Some(m) = self.term(term, result.end, raw) {
                    if m.end == result.end {
                        break;
                    }
                    result.end = m.end;
                    count += 1;
                    if let Some(value) = m.value {
                        values.push(value);
                    }
                    result.fields.extend(m.fields);
                    merge_marked(&mut result.marked, m.marked);
                    result.diagnostics.extend(m.diagnostics);
                    if *q == Quantifier::Optional {
                        break;
                    }
                }
                if *q == Quantifier::Plus && count == 0 {
                    return None;
                }
                result.value = Some(if *q == Quantifier::Optional {
                    values.into_iter().next().unwrap_or(AstValue::None)
                } else {
                    AstValue::List(values)
                });
                Some(result)
            }
            Term::Capture(name, term) => {
                let mut result = self.term(term, start, raw)?;
                let value = result.value.clone().unwrap_or(AstValue::None);
                result.fields.insert(
                    name.clone(),
                    Captured {
                        value,
                        span: Span::new(self.file, start, result.end),
                    },
                );
                Some(result)
            }
            Term::NotAhead(term) => {
                let farthest = self.farthest;
                let expected = self.expected.clone();
                let limit = self.limit;
                self.limit = self.source.len();
                let matches = self.term(term, start, true).is_some();
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
            Term::NotBehind(term) => {
                let farthest = self.farthest;
                let expected = self.expected.clone();
                let mut matches = false;
                let limit = self.limit;
                self.limit = start;
                if let Term::Regex(pattern) = term.as_ref() {
                    // End-constrained regex matching also explores alternatives and
                    // lazy matches that a forward prefix search would stop too early.
                    matches = self
                        .regex_ending_at(pattern, start)
                        .is_some_and(|r| r.is_match(self.source));
                } else {
                    for offset in (0..=start).filter(|p| self.source.is_char_boundary(*p)) {
                        if self
                            .term(term, offset, true)
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
            Term::Mark { id, term } => {
                let mut matched = self.term(term, start, raw)?;
                matched.marked.entry(*id).or_default().push(MarkedValue {
                    value: matched.value.clone(),
                    fields: matched.fields.clone(),
                });
                Some(matched)
            }
            Term::Project {
                id,
                term,
                quantifier,
            } => {
                let mut matched = self.term(term, start, raw)?;
                let originals = matched.marked.remove(id).unwrap_or_default();
                matched.fields.clear();
                let mut values = vec![];
                for original in originals {
                    if let Some(value) = original.value {
                        values.push(value);
                    }
                    matched.fields.extend(original.fields);
                }
                matched.value = if quantifier.is_some() {
                    Some(AstValue::List(values))
                } else {
                    values.into_iter().next()
                };
                Some(matched)
            }
        }
    }
}
fn merge_marked(
    target: &mut BTreeMap<u32, Vec<MarkedValue>>,
    source: BTreeMap<u32, Vec<MarkedValue>>,
) {
    for (id, values) in source {
        target.entry(id).or_default().extend(values);
    }
}
fn collapse(mut values: Vec<AstValue>) -> Option<AstValue> {
    match values.len() {
        0 => None,
        1 => values.pop(),
        _ => Some(AstValue::List(values)),
    }
}
impl ParseResult {
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
    }
}
