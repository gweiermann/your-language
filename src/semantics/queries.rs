//! Declarative contribution discovery and dependency evaluation over syntax identities.
use super::*;
use std::collections::BTreeSet;

const MODULE: &str = "std/query";

/// Register only generic query primitives. Reference/type policy remains authored YL.
pub fn register(registry: &mut OperationRegistry) -> Result<(), String> {
    struct Primitive;
    impl NativeOperation for Primitive {}
    for (name, parameters) in [
        ("get", vec!["key"]),
        ("getAll", vec!["key"]),
        ("contribute", vec!["key", "value"]),
        ("contributeAll", vec!["key", "values"]),
        ("ancestors", vec!["value"]),
        ("find", vec!["values", "callback"]),
        ("text", vec!["value"]),
        ("isPresent", vec!["value"]),
        ("equal", vec!["left", "right"]),
        ("hasMultiple", vec!["values"]),
        ("first", vec!["value"]),
        ("hypothetical", vec!["key", "assumptions"]),
        ("facts", vec!["key"]),
        ("concat", vec!["values"]),
    ] {
        registry.register(
            MODULE,
            name,
            OperationSignature {
                id: format!("{MODULE}#{name}"),
                parameters: parameters
                    .into_iter()
                    .map(|name| OperationParameter {
                        name: name.into(),
                        kind: ParameterKind::Value,
                        default: None,
                    })
                    .collect(),
                provides_context: false,
                finalization_only: false,
                contribution: matches!(name, "contribute" | "contributeAll"),
            },
            || Box::new(Primitive),
        )?;
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum QueryKey {
    Identity(SemanticIdentity),
    Text(String),
    Symbol(String),
    Bool(bool),
    Tuple(Vec<QueryKey>),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct QueryEntry {
    pub key: QueryKey,
    pub value: SemanticValue,
    pub producer: usize,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct QueryDependency {
    pub producer: usize,
    pub state: String,
    pub reads: Vec<QueryKey>,
    pub waits_for: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq)]
enum Outcome {
    Pending,
    Ready(SemanticValue),
    Conflict(Vec<usize>),
}

#[derive(Clone, Debug, PartialEq)]
enum ProducerState {
    Pending,
    Absent,
    Contribution(QueryKey, SemanticValue),
    Contributions(QueryKey, Vec<SemanticValue>),
    Diagnostic(Diagnostic),
    Conflict(Vec<usize>),
}

#[derive(Clone)]
enum Guard {
    If(QueryExpr, bool),
    Case {
        inputs: Vec<QueryExpr>,
        cases: Vec<QueryCase>,
        selected: usize,
    },
}

#[derive(Clone)]
enum Effect {
    Contribution(QueryExpr, QueryExpr),
    Contributions(QueryExpr, QueryExpr),
    Diagnostic {
        severity: crate::diagnostic::Severity,
        message: Vec<MeaningMessagePart>,
        target: Option<QueryExpr>,
    },
    Observe(QueryExpr),
}

#[derive(Clone)]
struct Producer<'a> {
    owner: &'a Occurrence,
    effect: Effect,
    guards: Vec<Guard>,
    span: Span,
    state: ProducerState,
    footprint: Pattern,
    reads: BTreeSet<QueryKey>,
    waits: BTreeSet<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Pattern {
    Any,
    Exact(QueryKey),
    Tuple(Vec<Pattern>),
    Union(Vec<Pattern>),
    Empty,
}
impl Pattern {
    fn matches(&self, key: &QueryKey) -> bool {
        match self {
            Self::Any => true,
            Self::Exact(value) => value == key,
            Self::Tuple(parts) => match key {
                QueryKey::Tuple(values) if values.len() == parts.len() => {
                    parts.iter().zip(values).all(|(p, v)| p.matches(v))
                }
                _ => false,
            },
            Self::Union(parts) => parts.iter().any(|part| part.matches(key)),
            Self::Empty => false,
        }
    }
    fn overlaps(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Empty, _) | (_, Self::Empty) => false,
            (Self::Any, _) | (_, Self::Any) => true,
            (Self::Exact(key), pattern) | (pattern, Self::Exact(key)) => pattern.matches(key),
            (Self::Union(parts), other) | (other, Self::Union(parts)) => {
                parts.iter().any(|part| part.overlaps(other))
            }
            (Self::Tuple(a), Self::Tuple(b)) => {
                a.len() == b.len() && a.iter().zip(b).all(|(a, b)| a.overlaps(b))
            }
        }
    }
}

#[derive(Default)]
struct Trace {
    reads: BTreeSet<QueryKey>,
    waits: BTreeSet<usize>,
}

type Locals = BTreeMap<String, SemanticValue>;

struct Engine<'a> {
    producers: Vec<Producer<'a>>,
    occurrences: BTreeMap<usize, &'a Occurrence>,
    parents: BTreeMap<usize, Option<usize>>,
    overrides: BTreeMap<QueryKey, SemanticValue>,
    hypothetical_depth: usize,
}

fn source_identity(source: &SourceCapture) -> Option<SemanticIdentity> {
    match source.occurrence_selection()? {
        OccurrenceSelection::One(id) => Some(SemanticIdentity::Occurrence(id)),
        _ => None,
    }
}

fn address(value: &SemanticValue, span: &Span) -> Result<Option<QueryKey>, Diagnostic> {
    Ok(match value {
        SemanticValue::Absent => None,
        SemanticValue::Identity(id) => Some(QueryKey::Identity(id.clone())),
        SemanticValue::Source(source) => {
            source_identity(source).map(QueryKey::Identity).or_else(|| {
                // Terminal-only captures retain their source address until the core
                // terminal-identity seam supplies an explicit selection.
                Some(QueryKey::Tuple(vec![
                    QueryKey::Symbol("core/syntax#terminal".into()),
                    QueryKey::Text(source.span.file.clone()),
                    QueryKey::Text(source.span.start.to_string()),
                    QueryKey::Text(source.span.end.to_string()),
                ]))
            })
        }
        SemanticValue::Text(value) => Some(QueryKey::Text(value.clone())),
        SemanticValue::Symbol(value) => Some(QueryKey::Symbol(value.clone())),
        SemanticValue::Bool(value) => Some(QueryKey::Bool(*value)),
        SemanticValue::Tuple(values) => {
            let mut key = vec![];
            for value in values {
                let Some(part) = address(value, span)? else {
                    return Ok(None);
                };
                key.push(part);
            }
            Some(QueryKey::Tuple(key))
        }
        _ => {
            return Err(Diagnostic::error(
                "query.key",
                "Expected an identity, scalar, or nested tuple key",
                span.clone(),
            ))
        }
    })
}

fn primitive(name: &str) -> &str {
    name.strip_prefix("std/query#").unwrap_or(name)
}

impl<'a> Engine<'a> {
    fn new(language: &CompiledLanguage, occurrences: &'a [Occurrence]) -> Self {
        fn visit<'a>(
            owner: &'a Occurrence,
            parent: Option<usize>,
            all: &mut BTreeMap<usize, &'a Occurrence>,
            parents: &mut BTreeMap<usize, Option<usize>>,
        ) {
            all.insert(owner.id, owner);
            parents.insert(owner.id, parent);
            for child in &owner.children {
                visit(child, Some(owner.id), all, parents);
            }
        }
        let mut engine = Self {
            producers: vec![],
            occurrences: BTreeMap::new(),
            parents: BTreeMap::new(),
            overrides: BTreeMap::new(),
            hypothetical_depth: 0,
        };
        for owner in occurrences {
            visit(owner, None, &mut engine.occurrences, &mut engine.parents);
        }
        // Discovery completes for the entire committed occurrence forest before
        // any get can observe absence. Conditional branches are included too.
        for owner in engine.occurrences.values().copied().collect::<Vec<_>>() {
            if let Some(groups) = language.query_meanings.get(&owner.definition) {
                for group in groups {
                    engine.discover(owner, &group.actions, &[]);
                }
            }
        }
        engine.infer_footprints();
        engine
    }

    fn discover(&mut self, owner: &'a Occurrence, actions: &[QueryAction], guards: &[Guard]) {
        for action in actions {
            let (effect, span) = match action {
                QueryAction::Call { name, args, span } if primitive(name) == "contribute" => {
                    (Effect::Contribution(args[0].clone(), args[1].clone()), span)
                }
                QueryAction::Call { name, args, span } if primitive(name) == "contributeAll" => (
                    Effect::Contributions(args[0].clone(), args[1].clone()),
                    span,
                ),
                QueryAction::Call { name, args, span } => (
                    Effect::Observe(QueryExpr::Call {
                        name: name.clone(),
                        args: args.clone(),
                    }),
                    span,
                ),
                QueryAction::Emit {
                    severity,
                    message,
                    target,
                    span,
                } => (
                    Effect::Diagnostic {
                        severity: severity.clone(),
                        message: message.clone(),
                        target: target.clone(),
                    },
                    span,
                ),
                QueryAction::If {
                    condition,
                    then_actions,
                    else_actions,
                    ..
                } => {
                    let mut nested = guards.to_vec();
                    nested.push(Guard::If(condition.clone(), true));
                    self.discover(owner, then_actions, &nested);
                    *nested.last_mut().expect("new guard") = Guard::If(condition.clone(), false);
                    self.discover(owner, else_actions, &nested);
                    continue;
                }
                QueryAction::Match { inputs, cases, .. } => {
                    for (selected, case) in cases.iter().enumerate() {
                        let mut nested = guards.to_vec();
                        nested.push(Guard::Case {
                            inputs: inputs.clone(),
                            cases: cases.clone(),
                            selected,
                        });
                        self.discover(owner, &case.actions, &nested);
                    }
                    continue;
                }
            };
            self.producers.push(Producer {
                owner,
                effect,
                guards: guards.to_vec(),
                span: span.clone(),
                state: ProducerState::Pending,
                footprint: Pattern::Any,
                reads: BTreeSet::new(),
                waits: BTreeSet::new(),
            });
        }
    }

    fn ancestors(
        &self,
        value: &SemanticValue,
        span: &Span,
    ) -> Result<Vec<SemanticValue>, Diagnostic> {
        let identity = match value {
            SemanticValue::Identity(id) => Some(id.clone()),
            SemanticValue::Source(source) => source_identity(source),
            _ => None,
        };
        let Some(SemanticIdentity::Occurrence(id)) = identity else {
            return Err(Diagnostic::error(
                "query.ancestors",
                "Expected a syntax occurrence",
                span.clone(),
            ));
        };
        let mut current = self.parents.get(&id).copied().flatten();
        let mut values = vec![];
        while let Some(id) = current {
            values.push(SemanticValue::Identity(SemanticIdentity::Occurrence(id)));
            current = self.parents.get(&id).copied().flatten();
        }
        Ok(values)
    }

    fn lookup(&self, key: &QueryKey, all: bool, facts: bool, trace: &mut Trace) -> Outcome {
        trace.reads.insert(key.clone());
        if let Some(value) = self.overrides.get(key) {
            return Outcome::Ready(if all || facts {
                SemanticValue::List(vec![value.clone()])
            } else {
                value.clone()
            });
        }
        let mut values = vec![];
        let mut producers = vec![];
        let mut pending = false;
        for (id, producer) in self.producers.iter().enumerate() {
            if !matches!(
                producer.effect,
                Effect::Contribution(..) | Effect::Contributions(..)
            ) || !producer.footprint.matches(key)
            {
                continue;
            }
            match &producer.state {
                ProducerState::Pending => {
                    pending = true;
                    trace.waits.insert(id);
                }
                ProducerState::Contribution(actual, value) if actual == key => {
                    values.push(value.clone());
                    producers.push(id);
                }
                ProducerState::Contributions(actual, items) if actual == key => {
                    values.extend(items.clone());
                    producers.extend(std::iter::repeat(id).take(items.len()));
                }
                ProducerState::Conflict(waiting) => {
                    pending = true;
                    trace.waits.extend(waiting);
                }
                _ => {}
            }
        }
        if facts {
            let mut unique = vec![];
            for value in values {
                if !unique.contains(&value) {
                    unique.push(value);
                }
            }
            return Outcome::Ready(SemanticValue::List(unique));
        }
        if pending {
            return Outcome::Pending;
        }
        if all {
            return Outcome::Ready(SemanticValue::List(values));
        }
        match values.len() {
            0 => Outcome::Ready(SemanticValue::Absent),
            1 => Outcome::Ready(values.remove(0)),
            _ => Outcome::Conflict(producers),
        }
    }

    fn eval(
        &self,
        expression: &QueryExpr,
        owner: &Occurrence,
        locals: &Locals,
        trace: &mut Trace,
    ) -> Result<Outcome, Diagnostic> {
        let ready = |value| Ok(Outcome::Ready(value));
        match expression {
            QueryExpr::SelfValue => ready(SemanticValue::Identity(SemanticIdentity::Occurrence(
                owner.id,
            ))),
            QueryExpr::Capture(name) => ready(SemanticValue::Source(owner.capture_path(name)?)),
            QueryExpr::Local(name) => {
                locals
                    .get(name)
                    .cloned()
                    .map(Outcome::Ready)
                    .ok_or_else(|| {
                        Diagnostic::error(
                            "query.local",
                            format!("Unbound callback parameter {name}"),
                            owner.span.clone(),
                        )
                    })
            }
            QueryExpr::Absent => ready(SemanticValue::Absent),
            QueryExpr::Bool(value) => ready(SemanticValue::Bool(*value)),
            QueryExpr::Text(value) => ready(SemanticValue::Text(value.clone())),
            QueryExpr::Symbol(value) => ready(SemanticValue::Symbol(value.clone())),
            QueryExpr::Tuple(items) | QueryExpr::List(items) => {
                let mut values = vec![];
                for item in items {
                    match self.eval(item, owner, locals, trace)? {
                        Outcome::Ready(value) => values.push(value),
                        outcome => return Ok(outcome),
                    }
                }
                ready(if matches!(expression, QueryExpr::Tuple(_)) {
                    SemanticValue::Tuple(values)
                } else {
                    SemanticValue::List(values)
                })
            }
            QueryExpr::Not(value) => match self.eval(value, owner, locals, trace)? {
                Outcome::Ready(SemanticValue::Bool(value)) => ready(SemanticValue::Bool(!value)),
                Outcome::Ready(_) => Err(Diagnostic::error(
                    "query.condition",
                    "Condition must be boolean",
                    owner.span.clone(),
                )),
                value => Ok(value),
            },
            QueryExpr::Lambda { .. } => Err(Diagnostic::error(
                "query.callback",
                "A callback is only valid as find's second argument",
                owner.span.clone(),
            )),
            QueryExpr::Call { name, args } => {
                self.call(primitive(name), args, owner, locals, trace)
            }
        }
    }

    fn call(
        &self,
        name: &str,
        args: &[QueryExpr],
        owner: &Occurrence,
        locals: &Locals,
        trace: &mut Trace,
    ) -> Result<Outcome, Diagnostic> {
        let bad = || {
            Diagnostic::error(
                "query.argument",
                format!("Invalid arguments for {name}"),
                owner.span.clone(),
            )
        };
        if name == "find" {
            let values = match self.eval(&args[0], owner, locals, trace)? {
                Outcome::Ready(SemanticValue::List(values)) => values,
                Outcome::Ready(_) => return Err(bad()),
                pending => return Ok(pending),
            };
            let QueryExpr::Lambda { parameter, body } = &args[1] else {
                return Err(bad());
            };
            let mut locals = locals.clone();
            for value in values {
                locals.insert(parameter.clone(), value);
                match self.eval(body, owner, &locals, trace)? {
                    Outcome::Ready(SemanticValue::Absent) => {}
                    result => return Ok(result),
                }
            }
            return Ok(Outcome::Ready(SemanticValue::Absent));
        }
        let mut values = vec![];
        for arg in args {
            match self.eval(arg, owner, locals, trace)? {
                Outcome::Ready(value) => values.push(value),
                value => return Ok(value),
            }
        }
        let result = match (name, values.as_slice()) {
            ("get" | "getAll" | "facts", [value]) => {
                let Some(key) = address(value, &owner.span)? else {
                    return Ok(Outcome::Ready(SemanticValue::Absent));
                };
                return Ok(self.lookup(&key, name == "getAll", name == "facts", trace));
            }
            ("ancestors", [value]) => SemanticValue::List(self.ancestors(value, &owner.span)?),
            ("text", [SemanticValue::Source(source)]) => SemanticValue::Text(source.text.clone()),
            ("text", [SemanticValue::Text(text)]) => SemanticValue::Text(text.clone()),
            ("isPresent", [value]) => SemanticValue::Bool(!matches!(value, SemanticValue::Absent)),
            ("equal", [left, right]) => SemanticValue::Bool(left == right),
            ("hasMultiple", [SemanticValue::List(values)]) => SemanticValue::Bool(values.len() > 1),
            ("first", [SemanticValue::List(values)]) => {
                values.first().cloned().unwrap_or(SemanticValue::Absent)
            }
            ("concat", [SemanticValue::List(values)]) => {
                let texts = values
                    .iter()
                    .map(|value| match value {
                        SemanticValue::Text(text) => Ok(text.as_str()),
                        _ => Err(bad()),
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                SemanticValue::Text(texts.concat())
            }
            ("hypothetical", [key, SemanticValue::List(assumptions)]) => {
                return self.hypothetical(key, assumptions, trace, &owner.span);
            }
            _ => return Err(bad()),
        };
        Ok(Outcome::Ready(result))
    }

    fn abstract_expr(
        &self,
        expr: &QueryExpr,
        owner: &Occurrence,
        locals: &Locals,
        depth: usize,
    ) -> Pattern {
        if depth > 24 {
            return Pattern::Any;
        }
        if let Ok(Outcome::Ready(value)) = self.eval(expr, owner, locals, &mut Trace::default()) {
            if let Ok(key) = address(&value, &owner.span) {
                return key.map(Pattern::Exact).unwrap_or(Pattern::Empty);
            }
        }
        match expr {
            QueryExpr::Tuple(values) => Pattern::Tuple(
                values
                    .iter()
                    .map(|value| self.abstract_expr(value, owner, locals, depth + 1))
                    .collect(),
            ),
            QueryExpr::Call { name, args } if primitive(name) == "get" => {
                let key = self.abstract_expr(&args[0], owner, locals, depth + 1);
                let candidates = self.producers.iter().filter(|producer| {
                    matches!(
                        producer.effect,
                        Effect::Contribution(..) | Effect::Contributions(..)
                    ) && key.overlaps(&producer.footprint)
                });
                let mut possibilities = vec![];
                for producer in candidates {
                    if let Effect::Contribution(_, value) = &producer.effect {
                        let pattern =
                            self.abstract_expr(value, producer.owner, &Locals::new(), depth + 1);
                        if matches!(pattern, Pattern::Any) {
                            return Pattern::Any;
                        }
                        if !possibilities.contains(&pattern) {
                            possibilities.push(pattern);
                        }
                    } else {
                        return Pattern::Any;
                    }
                }
                // No candidate can produce this key after discovery closes.
                if possibilities.is_empty() {
                    Pattern::Empty
                } else {
                    Pattern::Union(possibilities)
                }
            }
            QueryExpr::Call { name, args } if primitive(name) == "find" => {
                let Ok(Outcome::Ready(SemanticValue::List(values))) =
                    self.eval(&args[0], owner, locals, &mut Trace::default())
                else {
                    return Pattern::Any;
                };
                let QueryExpr::Lambda { parameter, body } = &args[1] else {
                    return Pattern::Any;
                };
                let mut candidates = vec![];
                for value in values {
                    let mut locals = locals.clone();
                    locals.insert(parameter.clone(), value);
                    let candidate = self.abstract_expr(body, owner, &locals, depth + 1);
                    if matches!(candidate, Pattern::Any) {
                        return Pattern::Any;
                    }
                    if !matches!(candidate, Pattern::Empty) && !candidates.contains(&candidate) {
                        candidates.push(candidate);
                    }
                }
                if candidates.is_empty() {
                    Pattern::Empty
                } else {
                    Pattern::Union(candidates)
                }
            }
            _ => Pattern::Any,
        }
    }

    fn infer_footprints(&mut self) -> bool {
        // Start with conservative structural patterns and refine with discovered
        // candidates. A pending tuple component never becomes definite absence.
        let footprints = self
            .producers
            .iter()
            .map(|producer| match &producer.effect {
                Effect::Contribution(key, _) | Effect::Contributions(key, _) => {
                    self.abstract_expr(key, producer.owner, &Locals::new(), 0)
                }
                _ => Pattern::Empty,
            })
            .collect::<Vec<_>>();
        let mut changed = false;
        for (producer, footprint) in self.producers.iter_mut().zip(footprints) {
            changed |= producer.footprint != footprint;
            producer.footprint = footprint;
        }
        changed
    }

    fn guard(
        &self,
        guard: &Guard,
        owner: &Occurrence,
        trace: &mut Trace,
    ) -> Result<Outcome, Diagnostic> {
        match guard {
            Guard::If(condition, expected) => {
                match self.eval(condition, owner, &Locals::new(), trace)? {
                    Outcome::Ready(SemanticValue::Bool(value)) => {
                        Ok(Outcome::Ready(SemanticValue::Bool(value == *expected)))
                    }
                    Outcome::Ready(_) => Err(Diagnostic::error(
                        "query.condition",
                        "An if condition must be boolean",
                        owner.span.clone(),
                    )),
                    value => Ok(value),
                }
            }
            Guard::Case {
                inputs,
                cases,
                selected,
            } => {
                let mut values = vec![];
                for input in inputs {
                    match self.eval(input, owner, &Locals::new(), trace)? {
                        Outcome::Ready(value) => values.push(value),
                        pending => return Ok(pending),
                    }
                }
                for (index, case) in cases.iter().enumerate() {
                    for alternative in &case.alternatives {
                        let mut matches = alternative.len() == values.len();
                        for (pattern, value) in alternative.iter().zip(&values) {
                            if let Some(pattern) = pattern {
                                match self.eval(pattern, owner, &Locals::new(), trace)? {
                                    Outcome::Ready(pattern) => matches &= pattern == *value,
                                    pending => return Ok(pending),
                                }
                            }
                        }
                        if matches {
                            return Ok(Outcome::Ready(SemanticValue::Bool(index == *selected)));
                        }
                    }
                }
                Ok(Outcome::Ready(SemanticValue::Bool(false)))
            }
        }
    }

    fn evaluate_producer(
        &self,
        producer: &Producer<'a>,
        trace: &mut Trace,
    ) -> Result<ProducerState, Diagnostic> {
        let mut pending = false;
        for guard in &producer.guards {
            match self.guard(guard, producer.owner, trace)? {
                Outcome::Ready(SemanticValue::Bool(false)) => return Ok(ProducerState::Absent),
                Outcome::Pending => pending = true,
                Outcome::Conflict(ids) => return Ok(ProducerState::Conflict(ids)),
                _ => {}
            }
        }
        if pending {
            return Ok(ProducerState::Pending);
        }
        match &producer.effect {
            Effect::Contribution(key, value) | Effect::Contributions(key, value) => {
                let key = match self.eval(key, producer.owner, &Locals::new(), trace)? {
                    Outcome::Ready(value) => address(&value, &producer.span)?,
                    Outcome::Pending => return Ok(ProducerState::Pending),
                    Outcome::Conflict(ids) => return Ok(ProducerState::Conflict(ids)),
                };
                let Some(key) = key else {
                    return Ok(ProducerState::Absent);
                };
                match self.eval(value, producer.owner, &Locals::new(), trace)? {
                    Outcome::Ready(SemanticValue::Absent) => Ok(ProducerState::Absent),
                    Outcome::Ready(SemanticValue::List(values))
                        if matches!(producer.effect, Effect::Contributions(..)) =>
                    {
                        let mut unique = vec![];
                        for value in values {
                            if !unique.contains(&value) {
                                unique.push(value);
                            }
                        }
                        Ok(ProducerState::Contributions(key, unique))
                    }
                    Outcome::Ready(_) if matches!(producer.effect, Effect::Contributions(..)) => {
                        Err(Diagnostic::error(
                            "query.argument",
                            "contributeAll expects a list",
                            producer.span.clone(),
                        ))
                    }
                    Outcome::Ready(value) => Ok(ProducerState::Contribution(key, value)),
                    Outcome::Pending => Ok(ProducerState::Pending),
                    Outcome::Conflict(ids) => Ok(ProducerState::Conflict(ids)),
                }
            }
            Effect::Diagnostic {
                severity,
                message,
                target,
            } => {
                let span = if let Some(target) = target {
                    match self.eval(target, producer.owner, &Locals::new(), trace)? {
                        Outcome::Ready(SemanticValue::Source(source)) => source.span,
                        Outcome::Ready(SemanticValue::Identity(SemanticIdentity::Occurrence(
                            id,
                        ))) => self
                            .occurrences
                            .get(&id)
                            .map(|owner| owner.span.clone())
                            .unwrap_or_else(|| producer.owner.span.clone()),
                        Outcome::Ready(_) => producer.owner.span.clone(),
                        Outcome::Pending => return Ok(ProducerState::Pending),
                        Outcome::Conflict(ids) => return Ok(ProducerState::Conflict(ids)),
                    }
                } else {
                    producer.owner.span.clone()
                };
                let mut text = String::new();
                for part in message {
                    match part {
                        MeaningMessagePart::Text(value) => text.push_str(value),
                        MeaningMessagePart::Capture(name) => {
                            text.push_str(&producer.owner.capture_path(name)?.text)
                        }
                    }
                }
                let mut diagnostic = Diagnostic::error("query.diagnostic", text, span);
                diagnostic.severity = severity.clone();
                diagnostic.secondary.push(producer.span.clone());
                Ok(ProducerState::Diagnostic(diagnostic))
            }
            Effect::Observe(expr) => {
                match self.eval(expr, producer.owner, &Locals::new(), trace)? {
                    Outcome::Pending => Ok(ProducerState::Pending),
                    Outcome::Conflict(ids) => Ok(ProducerState::Conflict(ids)),
                    Outcome::Ready(_) => Ok(ProducerState::Absent),
                }
            }
        }
    }

    fn settle(&mut self) -> Vec<Diagnostic> {
        let mut failures = BTreeMap::new();
        let limit = (self.producers.len() + 1).saturating_mul(8).max(32);
        for _ in 0..limit {
            let states = self
                .producers
                .iter()
                .enumerate()
                .map(|(id, producer)| {
                    let mut trace = Trace::default();
                    let state = match self.evaluate_producer(producer, &mut trace) {
                        Ok(state) => state,
                        Err(error) => {
                            failures.insert(id, error);
                            ProducerState::Absent
                        }
                    };
                    (state, trace)
                })
                .collect::<Vec<_>>();
            let mut changed = false;
            for (producer, (state, trace)) in self.producers.iter_mut().zip(states) {
                changed |= producer.state != state;
                producer.state = state;
                producer.reads = trace.reads;
                producer.waits = trace.waits;
            }
            changed |= self.infer_footprints();
            if !changed {
                return failures.into_values().collect();
            }
        }
        let span = self
            .producers
            .first()
            .map(|producer| producer.owner.span.clone())
            .unwrap_or_default();
        let mut diagnostics = failures.into_values().collect::<Vec<_>>();
        diagnostics.push(Diagnostic::error(
            "query.nonconvergent",
            "Declarative query propagation did not converge",
            span,
        ));
        diagnostics
    }

    fn hypothetical(
        &self,
        key: &SemanticValue,
        assumptions: &[SemanticValue],
        trace: &mut Trace,
        span: &Span,
    ) -> Result<Outcome, Diagnostic> {
        if self.hypothetical_depth >= 32 {
            return Err(Diagnostic::error(
                "query.hypothetical_cycle",
                "Recursive hypothetical query exceeded the evaluation limit",
                span.clone(),
            ));
        }
        let Some(key) = address(key, span)? else {
            return Ok(Outcome::Ready(SemanticValue::Absent));
        };
        let mut copy = Self {
            producers: self.producers.clone(),
            occurrences: self.occurrences.clone(),
            parents: self.parents.clone(),
            overrides: self.overrides.clone(),
            hypothetical_depth: self.hypothetical_depth + 1,
        };
        for assumption in assumptions {
            let SemanticValue::Tuple(parts) = assumption else {
                return Err(Diagnostic::error(
                    "query.assumption",
                    "Each assumption is a (key, value) tuple",
                    span.clone(),
                ));
            };
            if parts.len() != 2 {
                return Err(Diagnostic::error(
                    "query.assumption",
                    "Each assumption is a (key, value) tuple",
                    span.clone(),
                ));
            }
            let Some(key) = address(&parts[0], span)? else {
                return Err(Diagnostic::error(
                    "query.assumption",
                    "An assumption key cannot be absent",
                    span.clone(),
                ));
            };
            copy.overrides.insert(key, parts[1].clone());
        }
        for producer in &mut copy.producers {
            producer.state = ProducerState::Pending;
            producer.reads.clear();
            producer.waits.clear();
        }
        let failures = copy.settle();
        if let Some(failure) = failures.into_iter().next() {
            return Err(failure);
        }
        let result = copy.lookup(&key, false, false, trace);
        // Dependencies on shared reads remain visible to the caller. All writes
        // and diagnostic contributions live only in the isolated calculation.
        for producer in &copy.producers {
            trace.reads.extend(
                producer
                    .reads
                    .iter()
                    .filter(|key| !copy.overrides.contains_key(*key))
                    .cloned(),
            );
        }
        Ok(result)
    }
}

pub(super) fn execute(
    language: &CompiledLanguage,
    occurrences: &[Occurrence],
    graph: &mut SemanticGraph,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if language.query_meanings.is_empty() {
        return;
    }
    let mut engine = Engine::new(language, occurrences);
    diagnostics.extend(engine.settle());
    for (id, producer) in engine.producers.iter().enumerate() {
        let state = match &producer.state {
            ProducerState::Contribution(key, value) => {
                graph.query_entries.push(QueryEntry {
                    key: key.clone(),
                    value: value.clone(),
                    producer: id,
                    span: producer.owner.span.clone(),
                });
                "present"
            }
            ProducerState::Contributions(key, values) => {
                for value in values {
                    graph.query_entries.push(QueryEntry {
                        key: key.clone(),
                        value: value.clone(),
                        producer: id,
                        span: producer.owner.span.clone(),
                    });
                }
                "present"
            }
            ProducerState::Diagnostic(diagnostic) => {
                diagnostics.push(diagnostic.clone());
                "present"
            }
            ProducerState::Absent => "absent",
            ProducerState::Pending => {
                let mut diagnostic = Diagnostic::error(
                    "query.cycle",
                    "A declarative query cannot settle its dependencies",
                    producer.owner.span.clone(),
                );
                diagnostic.secondary = producer
                    .waits
                    .iter()
                    .filter_map(|id| engine.producers.get(*id))
                    .map(|producer| producer.owner.span.clone())
                    .collect();
                diagnostics.push(diagnostic);
                "pending"
            }
            ProducerState::Conflict(_) => "conflict",
        };
        graph.query_dependencies.push(QueryDependency {
            producer: id,
            state: state.into(),
            reads: producer.reads.iter().cloned().collect(),
            waits_for: producer.waits.iter().copied().collect(),
        });
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum QueryExpr {
    SelfValue,
    Capture(String),
    Local(String),
    Tuple(Vec<QueryExpr>),
    List(Vec<QueryExpr>),
    Call {
        name: String,
        args: Vec<QueryExpr>,
    },
    Lambda {
        parameter: String,
        body: Box<QueryExpr>,
    },
    Bool(bool),
    Text(String),
    Symbol(String),
    Absent,
    Not(Box<QueryExpr>),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct QueryGroup {
    pub name: Option<String>,
    pub actions: Vec<QueryAction>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum QueryAction {
    Call {
        name: String,
        args: Vec<QueryExpr>,
        span: Span,
    },
    If {
        condition: QueryExpr,
        then_actions: Vec<QueryAction>,
        else_actions: Vec<QueryAction>,
        span: Span,
    },
    Match {
        inputs: Vec<QueryExpr>,
        cases: Vec<QueryCase>,
        span: Span,
    },
    Emit {
        severity: crate::diagnostic::Severity,
        message: Vec<MeaningMessagePart>,
        span: Span,
        target: Option<QueryExpr>,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct QueryCase {
    pub alternatives: Vec<Vec<Option<QueryExpr>>>,
    pub actions: Vec<QueryAction>,
    pub span: Span,
}
