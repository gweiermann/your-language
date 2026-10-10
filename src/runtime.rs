//! Backtracking grammar interpreter with Pratt execution for recursive abstract families.
//! Every attempt owns its captures; failed alternatives cannot leak AST fields/diagnostics.
use crate::{
    compiler::edge_term,
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
    origin: Span,
    depth: usize,
}
#[derive(Clone)]
struct Match {
    end: usize,
    value: Option<AstValue>,
    origin: Option<Span>,
    fields: BTreeMap<String, Captured>,
    diagnostics: Vec<Diagnostic>,
    marked: BTreeMap<u32, Vec<MarkedValue>>,
    value_depth: usize,
    occurrences: Vec<crate::semantics::Occurrence>,
}
#[derive(Clone)]
struct MarkedValue {
    value: Option<AstValue>,
    origin: Option<Span>,
    fields: BTreeMap<String, Captured>,
    depth: usize,
    occurrences: Vec<crate::semantics::Occurrence>,
}
struct OperatorContext<'a> {
    left: Option<(&'a Term, &'a Match)>,
    right: Option<&'a Term>,
    family: &'a str,
    min_power: usize,
}
impl Match {
    fn empty(end: usize) -> Self {
        Self {
            end,
            value: None,
            origin: None,
            fields: BTreeMap::new(),
            diagnostics: vec![],
            marked: BTreeMap::new(),
            value_depth: 0,
            occurrences: vec![],
        }
    }
}
pub fn parse(language: &CompiledLanguage, source: &str) -> ParseResult {
    parse_named(language, "<source>", source)
}
pub fn parse_named(language: &CompiledLanguage, file: &str, source: &str) -> ParseResult {
    parse_occurrences(language, file, source).0
}
pub(crate) fn parse_occurrences(
    language: &CompiledLanguage,
    file: &str,
    source: &str,
) -> (ParseResult, Vec<crate::semantics::Occurrence>) {
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
        trace: !language.meanings.is_empty(),
    };
    let attempt = parser.rule(&language.entry, 0, false, 0);
    if let Some(mut matched) = attempt {
        let end = parser.skip(matched.end);
        if end == source.len() && !parser.resource_limit {
            if let Some(AstValue::Node(node)) = matched.value {
                let mut next = 0;
                fn identify(occurrence: &mut crate::semantics::Occurrence, next: &mut usize) {
                    occurrence.id = *next;
                    *next += 1;
                    for child in &mut occurrence.children {
                        identify(child, next);
                    }
                }
                for occurrence in &mut matched.occurrences {
                    identify(occurrence, &mut next);
                }
                return (
                    ParseResult {
                        ast: Some(*node),
                        diagnostics: matched.diagnostics,
                    },
                    matched.occurrences,
                );
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
    )
}
struct Runtime<'a> {
    language: &'a CompiledLanguage,
    trace: bool,
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
    /// A continuation that can be omitted does not introduce an EOF requirement.
    /// Keep failures after a consumed prefix: an unfinished operator/iteration
    /// still needs its operand/item, even when backtracking accepts the prefix.
    fn continuation(
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
                .and_then(|m| self.construct(id, &rule, start, m)),
            RuleBody::Abstract { bases, operators } => self
                .abstract_rule(id, bases, operators, start, raw, min_power)
                .map(|m| {
                    let mut m = self.checks(&rule, start, m);
                    self.trace_occurrence(id, start, &mut m);
                    m
                }),
        };
        self.depth -= 1;
        self.active.remove(&key);
        result
    }
    fn checks(&mut self, rule: &Rule, start: usize, mut matched: Match) -> Match {
        for check in &rule.constraints {
            let satisfied = self.evaluate_condition(&check.condition, &matched.fields)
                == ConditionValue::Bool(true);
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
    fn evaluate_condition(
        &mut self,
        condition: &Condition,
        fields: &BTreeMap<String, Captured>,
    ) -> ConditionValue {
        match condition {
            Condition::Always | Condition::CaptureType { .. } => ConditionValue::Bool(true),
            Condition::Bool(value) => ConditionValue::Bool(*value),
            Condition::Text(value) => ConditionValue::Text(value.clone()),
            Condition::Absent => ConditionValue::Absent,
            Condition::EnumValue { ty, variant } => {
                ConditionValue::Enum(ty.clone(), variant.clone())
            }
            Condition::CaptureValue(name) => {
                fields
                    .get(name)
                    .map_or(ConditionValue::Absent, |f| match &f.value {
                        AstValue::Text(value) => ConditionValue::Text(value.clone()),
                        AstValue::None => ConditionValue::Absent,
                        _ => ConditionValue::PresentCapture,
                    })
            }
            Condition::Present(name) => {
                ConditionValue::Bool(fields.get(name).is_some_and(|f| f.value != AstValue::None))
            }
            Condition::Matches { capture, regex }
            | Condition::OptionalMatches { capture, regex } => {
                let Some(field) = fields.get(capture).filter(|f| f.value != AstValue::None) else {
                    return ConditionValue::Absent;
                };
                let text = if let AstValue::Text(value) = &field.value {
                    value.clone()
                } else {
                    self.source
                        .get(field.span.start..field.span.end)
                        .unwrap_or("")
                        .into()
                };
                ConditionValue::Bool(self.regex(regex).is_some_and(|r| r.is_match(&text)))
            }
            Condition::Not(inner) => match self.evaluate_condition(inner, fields) {
                ConditionValue::Bool(v) => ConditionValue::Bool(!v),
                _ => ConditionValue::Absent,
            },
            Condition::And(a, b) => {
                let left = self.evaluate_condition(a, fields);
                if left == ConditionValue::Bool(true) {
                    self.evaluate_condition(b, fields)
                } else {
                    left
                }
            }
            Condition::Or(a, b) => {
                let left = self.evaluate_condition(a, fields);
                if left == ConditionValue::Bool(true) {
                    left
                } else {
                    self.evaluate_condition(b, fields)
                }
            }
            Condition::Equal(a, b) => {
                let a = self.evaluate_condition(a, fields);
                let b = self.evaluate_condition(b, fields);
                ConditionValue::Bool(a == b)
            }
            Condition::Between {
                trivia,
                left,
                right,
            } => {
                let (Some(a), Some(b)) = (fields.get(left), fields.get(right)) else {
                    return ConditionValue::Absent;
                };
                let (start, end) = (a.span.end, b.span.start);
                let found = if start >= end {
                    false
                } else if let Some(id) = trivia {
                    (start..end)
                        .filter(|p| self.source.is_char_boundary(*p))
                        .any(|offset| {
                            self.rule(id, offset, true, 0)
                                .is_some_and(|m| m.end <= end && m.end > offset)
                        })
                } else {
                    self.skip(start) > start
                };
                ConditionValue::Bool(found)
            }
        }
    }
    fn check_value_depth(&mut self, depth: usize, end: usize) -> Option<usize> {
        if depth > 128 {
            self.resource_limit = true;
            self.failure(end, "AST value within resource limit");
            None
        } else {
            Some(depth)
        }
    }
    fn trace_occurrence(&self, definition: &str, start: usize, matched: &mut Match) {
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
                        value: capture.value.clone(),
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
            id: 0,
            definition: definition.into(),
            span: Span::new(self.file, start, matched.end),
            captures,
            children: std::mem::take(&mut matched.occurrences),
        };
        matched.occurrences.push(occurrence);
    }
    fn construct(&mut self, id: &str, rule: &Rule, start: usize, matched: Match) -> Option<Match> {
        let mut matched = self.checks(rule, start, matched);
        matched.value_depth = self.check_value_depth(
            matched.fields.values().map(|f| f.depth).max().unwrap_or(0) + 1,
            matched.end,
        )?;
        self.trace_occurrence(id, start, &mut matched);
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
                if let Some(m) = self.continuation(left.end, raw, |parser| {
                    parser.operator(id, op, position, raw, Some(&left))
                }) {
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
        let min_power = if operator.associativity == Associativity::Nonassoc
            || (operator.left && operator.associativity != Associativity::Right)
        {
            operator.binding_power.checked_add(1)?
        } else {
            operator.binding_power
        };
        let context = OperatorContext {
            left: if operator.left {
                Some((edge_term(term, false)?, left?))
            } else {
                None
            },
            right: if operator.right {
                Some(edge_term(term, true)?)
            } else {
                None
            },
            family,
            min_power,
        };
        let mut result = self.term_with(term, start, raw, Some(&context))?;
        if let Some(left) = left {
            result.diagnostics.splice(0..0, left.diagnostics.clone());
        }
        self.construct(&operator.rule, &rule, start, result)
    }
    fn term(&mut self, term: &Term, position: usize, raw: bool) -> Option<Match> {
        self.term_with(term, position, raw, None)
    }
    fn term_with(
        &mut self,
        term: &Term,
        position: usize,
        raw: bool,
        context: Option<&OperatorContext<'_>>,
    ) -> Option<Match> {
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
        if let Some(context) = context {
            if let Some((leaf, left)) = context.left {
                if std::ptr::eq(term, leaf) {
                    let mut matched = left.clone();
                    matched.fields.clear();
                    matched.diagnostics.clear();
                    return Some(matched);
                }
            }
            if context.right.is_some_and(|leaf| std::ptr::eq(term, leaf)) {
                return self
                    .rule(context.family, start, raw, context.min_power)
                    .map(|mut matched| {
                        matched.fields.clear();
                        matched
                    });
            }
        }
        match term {
            Term::Meaning { definition, term } => {
                let mut matched = self.term_with(term, start, raw, context)?;
                self.trace_occurrence(definition, start, &mut matched);
                Some(matched)
            }
            Term::Literal(text) => {
                if self.source.get(start..self.limit)?.starts_with(text) {
                    let mut matched = Match::empty(start + text.len());
                    matched.value = Some(AstValue::Text(text.clone()));
                    matched.origin = Some(Span::new(self.file, start, matched.end));
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
                    result.origin = Some(Span::new(self.file, start, end));
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
                let mut origins = vec![];
                let mut depth = 0;
                for term in terms {
                    let m = self.term_with(term, result.end, raw, context)?;
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
            Term::Choice(terms) => {
                for term in terms {
                    if let Some(m) = self.term_with(term, start, raw, context) {
                        return Some(m);
                    }
                }
                None
            }
            Term::Repeat(term, q) => {
                let mut result = Match::empty(start);
                let mut values = vec![];
                let mut origins = vec![];
                let mut depth = 0;
                let mut count = 0;
                loop {
                    let attempt = if *q == Quantifier::Plus && count == 0 {
                        self.term_with(term, result.end, raw, context)
                    } else {
                        self.continuation(result.end, raw, |parser| {
                            parser.term_with(term, result.end, raw, context)
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
                        origins.push(m.origin.clone());
                    }
                    result.fields.extend(m.fields);
                    merge_marked(&mut result.marked, m.marked);
                    result.diagnostics.extend(m.diagnostics);
                    result.occurrences.extend(m.occurrences);
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
            Term::Capture(name, term) => {
                let mut result = self.term_with(term, start, raw, context)?;
                let value = result.value.clone().unwrap_or(AstValue::None);
                result.fields.insert(
                    name.clone(),
                    Captured {
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
                let mut matched = self.term_with(term, start, raw, context)?;
                matched.marked.entry(*id).or_default().push(MarkedValue {
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
            Term::Project {
                id,
                term,
                quantifier,
                collect_lists,
            } => {
                let mut matched = self.term_with(term, start, raw, context)?;
                let originals = matched.marked.remove(id).unwrap_or_default();
                matched.fields.clear();
                matched.occurrences.clear();
                let mut values = vec![];
                let mut origins = vec![];
                let mut depth = 0;
                for original in originals {
                    if let Some(value) = original.value {
                        origins.push(original.origin.clone());
                        depth = depth.max(original.depth);
                        if *collect_lists {
                            if let AstValue::List(items) = value {
                                values.extend(items);
                            } else {
                                values.push(value);
                            }
                        } else if value != AstValue::None {
                            values.push(value);
                        }
                    }
                    matched.fields.extend(original.fields);
                    matched.occurrences.extend(original.occurrences);
                }
                matched.value_depth =
                    self.check_value_depth(depth + usize::from(quantifier.is_some()), matched.end)?;
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
#[derive(PartialEq)]
enum ConditionValue {
    PresentCapture,
    Bool(bool),
    Text(String),
    Enum(String, String),
    Absent,
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
