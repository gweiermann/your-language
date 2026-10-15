//! Experimental representative syntax generation. It never executes meaning operations.
use crate::{
    diagnostic::{Diagnostic, Span},
    ir::{CompiledLanguage, Rule, RuleBody, Term},
    syntax::{Example, Quantifier},
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExampleDefinition {
    pub term: Term,
    pub explicit: Option<Example>,
    pub span: Span,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Preview {
    pub definition: String,
    pub span: Span,
    pub text: Option<String>,
    pub reason: Option<String>,
}
impl CompiledLanguage {
    /// One bounded, parser-validated representative per node and parameterless pattern.
    pub fn example_previews(&self) -> Vec<Preview> {
        if let Some(image) = &self.image {
            return image.previews.clone();
        }
        // Check the selected trivia itself, without automatic trivia skipping.
        // This is a presentation choice at fragment boundaries, never a keyword rule.
        let mut trivia_language = self.clone();
        trivia_language.trivia.clear();
        let spaces = self
            .trivia
            .iter()
            .any(|id| accepts(&trivia_language, &Term::Ref(id.clone()), " "));
        let mut definitions: BTreeMap<_, _> = self
            .rules
            .iter()
            .filter(|(_, r)| !r.trivia)
            .map(|(id, r)| {
                (
                    id.clone(),
                    ExampleDefinition {
                        term: Term::Ref(id.clone()),
                        explicit: None,
                        span: r.span.clone(),
                    },
                )
            })
            .collect();
        definitions.extend(self.examples.clone());
        definitions.into_iter().map(|(id, definition)| {
            let text = if let Some(explicit) = &definition.explicit { Some(explicit.text.clone()) } else {
                let mut generator = Generator { language: self, spaces, steps: 0, active: BTreeSet::new(), projection: None };
                let readable = generator.term(&definition.term, 0).into_iter().find(|candidate| accepts(self, &definition.term, &candidate.text)).map(|candidate| candidate.text);
                if readable.is_some() || !spaces { readable } else {
                    // Keep a concatenation pass within the same step budget so a
                    // many-fragment adjacency constraint is not starved by the beam.
                    generator.spaces = false;
                    generator.term(&definition.term, 0).into_iter().find(|candidate| accepts(self, &definition.term, &candidate.text)).map(|candidate| candidate.text)
                }
            };
            Preview { definition: id, span: definition.span, reason: text.is_none().then(|| "No valid representative within generation limits; supply an explicit example".into()), text }
        }).collect()
    }
}
pub(crate) fn validate_explicit(language: &CompiledLanguage) -> Result<(), Diagnostic> {
    for definition in language.examples.values() {
        if let Some(example) = &definition.explicit {
            if !accepts(language, &definition.term, &example.text) {
                return Err(Diagnostic::error(
                    "yl.invalid_example",
                    "Example must parse completely without syntax errors or constraint errors",
                    example.span.clone(),
                ));
            }
        }
    }
    Ok(())
}
fn accepts(language: &CompiledLanguage, term: &Term, text: &str) -> bool {
    let mut isolated = language.clone();
    isolated.entry = "<example>".into();
    isolated.rules.insert(
        isolated.entry.clone(),
        Rule {
            name: isolated.entry.clone(),
            trivia: false,
            body: RuleBody::Concrete(term.clone()),
            constraints: vec![],
            span: Span::default(),
        },
    );
    let parsed = crate::parse(&isolated, text);
    if parsed.ast.is_some() && !parsed.has_errors() {
        return true;
    }
    // Recursive operator members are parsed in their Pratt family's context.
    // Require the resulting root to be this member, not another family alternative.
    if let Term::Ref(id) = term {
        if let Some(member) = language.rules.get(id) {
            for (family, rule) in &language.rules {
                if let RuleBody::Abstract { operators, .. } = &rule.body {
                    if operators.iter().any(|operator| &operator.rule == id) {
                        let mut context = language.clone();
                        context.entry = family.clone();
                        context.meanings.entry(family.clone()).or_default();
                        let (result, occurrences) =
                            crate::runtime::parse_occurrences(&context, "<example>", text);
                        if !result.has_errors()
                            && result.ast.is_some_and(|ast| ast.kind == member.name)
                            && occurrences.iter().any(|root| {
                                root.definition == *family
                                    && root
                                        .children
                                        .first()
                                        .is_some_and(|child| child.definition == *id)
                            })
                        {
                            return true;
                        }
                    }
                }
            }
        }
    }
    false
}
const CANDIDATES: usize = 24;
const BYTES: usize = 4096;

struct Generator<'a> {
    language: &'a CompiledLanguage,
    steps: usize,
    active: BTreeSet<String>,
    projection: Option<u32>,
    spaces: bool,
}
#[derive(Clone)]
struct Candidate {
    text: String,
    marks: BTreeMap<u32, usize>,
    unspaced: usize,
}
impl Candidate {
    fn new(text: String) -> Self {
        Self {
            text,
            marks: BTreeMap::new(),
            unspaced: 0,
        }
    }
    fn join(&self, other: &Self, separator: &str) -> Self {
        let mut marks = self.marks.clone();
        for (id, count) in &other.marks {
            *marks.entry(*id).or_default() += count;
        }
        Self {
            text: format!("{}{separator}{}", self.text, other.text),
            marks,
            unspaced: self.unspaced + other.unspaced,
        }
    }
}
fn bounded(mut candidates: Vec<Candidate>) -> Vec<Candidate> {
    candidates.retain(|s| s.text.len() <= BYTES);
    // Prefer readable separation when selected trivia permits it. Keep bounded
    // concatenated alternatives for adjacency-sensitive syntax/constraints.
    candidates.sort_by_key(|s| (s.unspaced, s.text.len())); // stable: declaration order breaks ties
    let mut seen = BTreeSet::new();
    candidates.retain(|s| seen.insert((s.text.clone(), s.marks.clone())));
    candidates.truncate(CANDIDATES);
    candidates
}
impl Generator<'_> {
    fn term(&mut self, term: &Term, depth: usize) -> Vec<Candidate> {
        self.steps += 1;
        if depth > 48 || self.steps > 10000 {
            return vec![];
        }
        let candidates = match term {
            Term::Example { definition, term } => {
                if let Some(example) = self
                    .language
                    .examples
                    .get(definition)
                    .and_then(|d| d.explicit.as_ref())
                {
                    vec![Candidate::new(example.text.clone())]
                } else {
                    self.term(term, depth + 1)
                }
            }
            Term::Literal(s) => vec![Candidate::new(s.clone())],
            Term::Regex(s) => regex_syntax::Parser::new()
                .parse(s)
                .ok()
                .and_then(|hir| regex_example(&hir))
                .map(Candidate::new)
                .into_iter()
                .collect(),
            Term::Ref(id) => {
                if let Some(example) = self
                    .language
                    .examples
                    .get(id)
                    .and_then(|d| d.explicit.as_ref())
                {
                    return bounded(vec![Candidate::new(example.text.clone())]);
                }
                if !self.active.insert(id.clone()) {
                    return vec![];
                }
                let result = match self.language.rules.get(id).map(|r| &r.body) {
                    Some(RuleBody::Concrete(term)) => self.term(term, depth + 1),
                    Some(RuleBody::Abstract { bases, .. }) => {
                        let mut all = vec![];
                        for base in bases {
                            all.extend(self.term(&Term::Ref(base.clone()), depth + 1));
                        }
                        bounded(all)
                    }
                    None => vec![],
                };
                self.active.remove(id);
                result
            }
            Term::Sequence(terms) => {
                let mut candidates = vec![Candidate::new(String::new())];
                for term in terms {
                    let next = self.term(term, depth + 1);
                    let mut combined = vec![];
                    for prefix in &candidates {
                        for suffix in &next {
                            let gap = self.spaces
                                && !prefix.text.is_empty()
                                && !suffix.text.is_empty()
                                && !prefix.text.ends_with(char::is_whitespace)
                                && !suffix.text.starts_with(char::is_whitespace);
                            let mut adjacent = prefix.join(suffix, "");
                            if gap {
                                adjacent.unspaced += 1;
                                combined.push(prefix.join(suffix, " "));
                            }
                            combined.push(adjacent);
                        }
                    }
                    candidates = bounded(combined);
                }
                candidates
            }
            Term::Choice(terms) => {
                let mut candidates = vec![];
                for term in terms {
                    candidates.extend(self.term(term, depth + 1));
                }
                bounded(candidates)
            }
            Term::Repeat(term, quantifier) => match (quantifier, self.projection) {
                (Quantifier::Optional, None) => vec![Candidate::new(String::new())],
                (Quantifier::Optional | Quantifier::Star, Some(_)) => {
                    let mut candidates = vec![Candidate::new(String::new())];
                    candidates.extend(self.term(term, depth + 1));
                    candidates
                }
                (Quantifier::Star | Quantifier::Plus, None) => {
                    // Runtime repetitions require progress; empty text would match zero items.
                    self.term(term, depth + 1)
                        .into_iter()
                        .filter(|candidate| !candidate.text.is_empty())
                        .collect()
                }
                _ => self.term(term, depth + 1),
            },
            Term::Capture(_, term) | Term::Meaning { term, .. } => self.term(term, depth + 1),
            Term::Mark { id, term } => {
                // The original item still follows authored repetition/optional defaults.
                let previous = self.projection;
                if previous == Some(*id) {
                    self.projection = None;
                }
                let mut candidates = self.term(term, depth + 1);
                self.projection = previous;
                for candidate in &mut candidates {
                    *candidate.marks.entry(*id).or_default() += 1;
                }
                candidates
            }
            Term::Project {
                id,
                term,
                quantifier: Some(Quantifier::Star | Quantifier::Plus),
                ..
            } => {
                let previous = self.projection.replace(*id);
                let mut candidates = self.term(term, depth + 1);
                self.projection = previous;
                // Choose one original item regardless of wrapper nesting or empty alternatives.
                candidates.retain(|candidate| candidate.marks.get(id) == Some(&1));
                candidates
            }
            Term::Project { term, .. } => self.term(term, depth + 1),
            Term::NotAhead(_) | Term::NotBehind(_) => vec![Candidate::new(String::new())],
        };
        bounded(candidates)
    }
}
fn regex_example(hir: &regex_syntax::hir::Hir) -> Option<String> {
    use regex_syntax::hir::{Class, HirKind};
    match hir.kind() {
        HirKind::Empty | HirKind::Look(_) => Some(String::new()),
        HirKind::Literal(literal) => String::from_utf8(literal.0.to_vec()).ok(),
        HirKind::Class(Class::Unicode(class)) => {
            class.ranges().first().map(|r| r.start().to_string())
        }
        HirKind::Class(Class::Bytes(class)) => class
            .ranges()
            .first()
            .and_then(|r| char::from_u32(r.start() as u32))
            .map(|c| c.to_string()),
        HirKind::Capture(capture) => regex_example(&capture.sub),
        HirKind::Repetition(repeat) => {
            let item = regex_example(&repeat.sub)?;
            if repeat.min > 256 || item.len().saturating_mul(repeat.min as usize) > BYTES {
                None
            } else {
                Some(item.repeat(repeat.min as usize))
            }
        }
        HirKind::Concat(parts) => parts
            .iter()
            .map(regex_example)
            .collect::<Option<Vec<_>>>()
            .map(|p| p.concat())
            .filter(|s| s.len() <= BYTES),
        HirKind::Alternation(parts) => {
            let mut alternatives: Vec<_> = parts.iter().filter_map(regex_example).collect();
            alternatives.sort_by_key(String::len);
            alternatives.into_iter().next()
        }
    }
}
