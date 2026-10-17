//! Backtracking grammar interpreter with Pratt execution for recursive abstract families.
//! Every attempt owns its captures; failed alternatives cannot leak AST fields/diagnostics.
use crate::{
    diagnostic::{Diagnostic, Severity, Span},
    image::parser::{
        CheckExpression, ImageBody, ImageOperator, ImageRule, Instruction, ParserImage,
    },
    ir::{CompiledLanguage, HighlightSpan},
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
#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct TerminalAddress {
    file: String,
    start: usize,
    end: usize,
    capture: String,
    position: usize,
}
#[derive(Clone)]
struct Captured {
    element_occurrences: Vec<Vec<usize>>,
    element_spans: Vec<Span>,
    repeated: bool,
    occurrences: Vec<usize>,
    value: AstValue,
    span: Span,
    origin: Span,
    depth: usize,
}
#[derive(Clone)]
struct Match {
    element_occurrences: Vec<Vec<usize>>,
    element_spans: Vec<Span>,
    end: usize,
    value: Option<AstValue>,
    origin: Option<Span>,
    fields: BTreeMap<String, Captured>,
    diagnostics: Vec<Diagnostic>,
    marked: BTreeMap<u32, Vec<MarkedValue>>,
    value_depth: usize,
    occurrences: Vec<crate::semantics::Occurrence>,
    highlights: Vec<HighlightSpan>,
}
#[derive(Clone)]
struct MarkedValue {
    element_occurrences: Vec<Vec<usize>>,
    element_spans: Vec<Span>,
    value: Option<AstValue>,
    origin: Option<Span>,
    fields: BTreeMap<String, Captured>,
    depth: usize,
    occurrences: Vec<crate::semantics::Occurrence>,
}
struct OperatorContext<'a> {
    left: Option<(u32, &'a Match)>,
    right: Option<u32>,
    family: u32,
    min_power: usize,
}
impl Match {
    fn empty(end: usize) -> Self {
        Self {
            element_occurrences: vec![],
            element_spans: vec![],
            end,
            value: None,
            origin: None,
            fields: BTreeMap::new(),
            diagnostics: vec![],
            marked: BTreeMap::new(),
            value_depth: 0,
            occurrences: vec![],
            highlights: vec![],
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
    let (result, occurrences, _) = parse_session(language, file, source, false);
    (result, occurrences)
}
impl CompiledLanguage {
    /// Highlight successful target-language tokens. Failed alternatives, trivia and
    /// lookaround never leak tokens. Wrapper metadata overrides its inner grammar.
    pub fn highlight_named(&self, file: &str, source: &str) -> Vec<HighlightSpan> {
        parse_session(self, file, source, true).2
    }
}
fn parse_session(
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

struct Runtime<'a> {
    image: &'a ParserImage,
    trace: bool,
    highlighting: bool,
    partial_highlights: Vec<HighlightSpan>,
    next_occurrence: usize,
    source: &'a str,
    file: &'a str,
    regexes: BTreeMap<String, regex::Regex>,
    active: BTreeSet<(u32, usize, bool, usize, usize)>,
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
    fn skip(&mut self, position: usize) -> usize {
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
    fn rule(&mut self, id: u32, position: usize, raw: bool, min_power: usize) -> Option<Match> {
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
    fn highlight(&self, definition: &str, matched: &mut Match) {
        if let Some(category) = self.image.highlights.get(definition) {
            for token in &mut matched.highlights {
                token.token_type.clone_from(category);
            }
        }
    }
    fn terminal_highlight(&self, matched: &mut Match, start: usize) {
        if self.highlighting && start < matched.end {
            matched.highlights.push(HighlightSpan {
                span: Span::new(self.file, start, matched.end),
                token_type: String::new(),
            });
        }
    }
    fn highlight_trivia(&mut self, tokens: &mut Vec<HighlightSpan>) {
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
    fn checks(&mut self, rule: &ImageRule, start: usize, mut matched: Match) -> Match {
        for check in &rule.constraints {
            let satisfied = self.evaluate_condition(check.condition, &matched.fields)
                == ConditionValue::Bool(true);
            if satisfied {
                for emission in &check.emissions {
                    matched.diagnostics.push(Diagnostic {
                        severity: emission.severity.clone(),
                        code: "parse.constraint".into(),
                        message: emission.message.clone(),
                        primary: Box::new(Span::new(self.file, start, matched.end)),
                        secondary: vec![emission.span.clone()],
                        annotations: Box::default(),
                        help: None,
                    });
                }
            }
        }
        matched
    }
    fn evaluate_condition(
        &mut self,
        condition: u32,
        fields: &BTreeMap<String, Captured>,
    ) -> ConditionValue {
        let image = self.image;
        match &image.checks[condition as usize] {
            CheckExpression::Always | CheckExpression::CaptureType { .. } => {
                ConditionValue::Bool(true)
            }
            CheckExpression::Bool(value) => ConditionValue::Bool(*value),
            CheckExpression::Text(value) => ConditionValue::Text(value.clone()),
            CheckExpression::Absent => ConditionValue::Absent,
            CheckExpression::EnumValue { ty, variant } => {
                ConditionValue::Enum(ty.clone(), variant.clone())
            }
            CheckExpression::CaptureValue(name) => {
                fields
                    .get(name)
                    .map_or(ConditionValue::Absent, |f| match &f.value {
                        AstValue::Text(value) => ConditionValue::Text(value.clone()),
                        AstValue::None => ConditionValue::Absent,
                        _ => ConditionValue::PresentCapture,
                    })
            }
            CheckExpression::Present(name) => {
                ConditionValue::Bool(fields.get(name).is_some_and(|f| f.value != AstValue::None))
            }
            CheckExpression::Matches { capture, regex }
            | CheckExpression::OptionalMatches { capture, regex } => {
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
            CheckExpression::Not(inner) => match self.evaluate_condition(*inner, fields) {
                ConditionValue::Bool(v) => ConditionValue::Bool(!v),
                _ => ConditionValue::Absent,
            },
            CheckExpression::And(a, b) => {
                let left = self.evaluate_condition(*a, fields);
                if left == ConditionValue::Bool(true) {
                    self.evaluate_condition(*b, fields)
                } else {
                    left
                }
            }
            CheckExpression::Or(a, b) => {
                let left = self.evaluate_condition(*a, fields);
                if left == ConditionValue::Bool(true) {
                    left
                } else {
                    self.evaluate_condition(*b, fields)
                }
            }
            CheckExpression::Equal(a, b) => {
                let a = self.evaluate_condition(*a, fields);
                let b = self.evaluate_condition(*b, fields);
                ConditionValue::Bool(a == b)
            }
            CheckExpression::Between {
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
                            self.rule(*id, offset, true, 0)
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
    fn trace_occurrence(&mut self, definition: &str, start: usize, matched: &mut Match) {
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
    fn construct(
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
    fn abstract_rule(
        &mut self,
        id: u32,
        bases: &[u32],
        operators: &[ImageOperator],
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
                if let Some(m) = self.rule(*base, position, raw, 0) {
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
        family: u32,
        operator: &ImageOperator,
        start: usize,
        raw: bool,
        left: Option<&Match>,
    ) -> Option<Match> {
        let image = self.image;
        let rule = image.rules.get(operator.rule as usize)?;
        let ImageBody::Concrete(term) = &rule.body else {
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
                Some((operator.left_edge?, left?))
            } else {
                None
            },
            right: if operator.right {
                Some(operator.right_edge?)
            } else {
                None
            },
            family,
            min_power,
        };
        let mut result = self.term_with(*term, start, raw, Some(&context))?;
        if let Some(left) = left {
            result.diagnostics.splice(0..0, left.diagnostics.clone());
        }
        self.construct(&rule.definition, rule, start, result)
    }
    fn term(&mut self, term: u32, position: usize, raw: bool) -> Option<Match> {
        self.term_with(term, position, raw, None)
    }
    fn term_with(
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
    fn term_inner(
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsing_and_highlighting_respect_bounds_on_a_one_mib_host_stack() {
        let language = crate::compile_sources(
            "language.yl",
            &BTreeMap::from([(
                "language.yl".into(),
                r#"node Program = "(" child:Program ")" | value:"x" {
                    ide { highlightAs "type" }
                }
                entry Program"#
                    .into(),
            )]),
        )
        .unwrap();
        std::thread::Builder::new()
            .stack_size(1024 * 1024)
            .spawn(move || {
                let source = format!("{}x{}", "(".repeat(30), ")".repeat(30));
                assert!(!parse_named(&language, "program", &source).has_errors());
                assert_eq!(language.highlight_named("program", &source).len(), 61);
                let oversized = format!("{}x{}", "(".repeat(300), ")".repeat(300));
                let rejected = parse_named(&language, "program", &oversized);
                assert!(rejected.ast.is_none());
                assert_eq!(rejected.diagnostics[0].code, "parse.resource_limit");
            })
            .unwrap()
            .join()
            .unwrap();
    }
}
