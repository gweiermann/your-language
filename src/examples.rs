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
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Preview {
    pub definition: String,
    pub span: Span,
    pub text: Option<String>,
    pub reason: Option<String>,
}
impl CompiledLanguage {
    /// One bounded, parser-validated representative per node and parameterless pattern.
    pub fn example_previews(&self) -> Vec<Preview> {
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
                let mut generator = Generator { language: self, steps: 0, active: BTreeSet::new() };
                generator.term(&definition.term, 0).into_iter().find(|text| accepts(self, &definition.term, text))
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
    parsed.ast.is_some() && !parsed.has_errors()
}
const CANDIDATES: usize = 24;
const BYTES: usize = 4096;
struct Generator<'a> {
    language: &'a CompiledLanguage,
    steps: usize,
    active: BTreeSet<String>,
}
fn bounded(mut strings: Vec<String>) -> Vec<String> {
    strings.retain(|s| s.len() <= BYTES);
    strings.sort_by_key(String::len); // stable: declaration order breaks ties
    let mut seen = BTreeSet::new();
    strings.retain(|s| seen.insert(s.clone()));
    strings.truncate(CANDIDATES);
    strings
}
impl Generator<'_> {
    fn term(&mut self, term: &Term, depth: usize) -> Vec<String> {
        self.steps += 1;
        if depth > 48 || self.steps > 10000 {
            return vec![];
        }
        match term {
            Term::Example { definition, term } => {
                if let Some(example) = self
                    .language
                    .examples
                    .get(definition)
                    .and_then(|d| d.explicit.as_ref())
                {
                    vec![example.text.clone()]
                } else {
                    self.term(term, depth + 1)
                }
            }
            Term::Literal(s) => vec![s.clone()],
            Term::Regex(s) => regex_syntax::Parser::new()
                .parse(s)
                .ok()
                .and_then(|hir| regex_example(&hir))
                .into_iter()
                .collect(),
            Term::Ref(id) => {
                if let Some(example) = self
                    .language
                    .examples
                    .get(id)
                    .and_then(|d| d.explicit.as_ref())
                {
                    return vec![example.text.clone()];
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
                let mut candidates = vec![String::new()];
                for term in terms {
                    let next = self.term(term, depth + 1);
                    let mut combined = vec![];
                    for prefix in &candidates {
                        for suffix in &next {
                            combined.push(format!("{prefix}{suffix}"));
                            if !self.language.trivia.is_empty()
                                && !prefix.is_empty()
                                && !suffix.is_empty()
                            {
                                combined.push(format!("{prefix} {suffix}"));
                            }
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
            Term::Repeat(_, Quantifier::Optional) => vec![String::new()],
            Term::Repeat(term, _)
            | Term::Capture(_, term)
            | Term::Meaning { term, .. }
            | Term::Mark { term, .. } => self.term(term, depth + 1),
            Term::Project {
                term,
                quantifier: Some(Quantifier::Star | Quantifier::Plus),
                ..
            } => {
                let mut inner = term.as_ref().clone();
                suppress_wrapper_repetition(&mut inner);
                let mut inner = &inner;
                while let Term::Meaning { term, .. } | Term::Capture(_, term) = inner {
                    inner = term;
                }
                if let Term::Repeat(item, Quantifier::Optional) = inner {
                    self.term(item, depth + 1)
                } else {
                    self.term(inner, depth + 1)
                }
            }
            Term::Project { term, .. } => self.term(term, depth + 1),
            Term::NotAhead(_) | Term::NotBehind(_) => vec![String::new()],
        }
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
        HirKind::Alternation(parts) => bounded(parts.iter().filter_map(regex_example).collect())
            .into_iter()
            .next(),
    }
}

// Pipe-generated list tails are wrapper machinery, not authored repetitions.
fn suppress_wrapper_repetition(term: &mut Term) {
    match term {
        Term::Mark { .. } => {}
        Term::Repeat(inner, quantifier) => {
            if *quantifier == Quantifier::Star {
                *quantifier = Quantifier::Optional;
            }
            suppress_wrapper_repetition(inner);
        }
        Term::Sequence(terms) | Term::Choice(terms) => {
            for term in terms {
                suppress_wrapper_repetition(term);
            }
        }
        Term::Capture(_, inner) | Term::Meaning { term: inner, .. } => {
            suppress_wrapper_repetition(inner)
        }
        _ => {}
    }
}
