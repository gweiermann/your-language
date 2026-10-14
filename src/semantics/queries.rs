//! Declarative contribution discovery and dependency evaluation over syntax identities.
use super::*;
use std::collections::BTreeSet;
use std::{cell::RefCell, rc::Rc};

const MODULE: &str = "std/query";

/// Register only generic query primitives. Reference/type policy remains authored YL.
pub fn register(registry: &mut OperationRegistry) -> Result<(), String> {
    struct Primitive;
    impl NativeOperation for Primitive {}
    for (name, parameters) in [
        ("get", vec!["key"]),
        ("getEarlier", vec!["key"]),
        ("getAll", vec!["key"]),
        ("contribute", vec!["key", "value"]),
        ("contributeAll", vec!["key", "values"]),
        ("ancestors", vec!["value"]),
        ("children", vec!["value"]),
        ("find", vec!["values", "callback"]),
        ("map", vec!["values", "callback"]),
        ("filter", vec!["values", "callback"]),
        ("choose", vec!["condition", "then", "else"]),
        ("values", vec!["value"]),
        ("alternatives", vec!["values"]),
        ("flatten", vec!["values"]),
        ("last", vec!["value"]),
        ("tail", vec!["value"]),
        ("isList", vec!["value"]),
        ("hypotheticalAll", vec!["key", "assumptions"]),
        ("hypotheticalChecked", vec!["key", "assumptions"]),
        ("every", vec!["values", "callback"]),
        ("text", vec!["value"]),
        ("isPresent", vec!["value"]),
        ("equal", vec!["left", "right"]),
        ("hasMultiple", vec!["values"]),
        ("first", vec!["value"]),
        ("hypothetical", vec!["key", "assumptions"]),
        ("facts", vec!["key"]),
        ("concat", vec!["values"]),
        ("contains", vec!["values", "value"]),
        ("unique", vec!["values"]),
        ("isSubset", vec!["left", "right"]),
        ("lookup", vec!["fields", "key"]),
        ("keys", vec!["fields"]),
        ("zip", vec!["left", "right"]),
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
    Many(Vec<ProducerState>),
}

impl ProducerState {
    fn leaves(&self) -> Vec<&Self> {
        match self {
            Self::Many(states) => states.iter().collect(),
            state => vec![state],
        }
    }
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
        message: Vec<QueryMessagePart>,
        target: Option<QueryExpr>,
        related: Vec<QueryExpr>,
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
    diagnostics: Vec<Diagnostic>,
    order: (usize, usize),
    approximate: bool,
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

#[derive(Clone, Default)]
struct Trace {
    reads: BTreeSet<QueryKey>,
    waits: BTreeSet<usize>,
    producers: BTreeSet<usize>,
    diagnostics: Vec<Diagnostic>,
    order: Option<(usize, usize)>,
    branch_values: Vec<(QueryExpr, SemanticValue)>,
    approximate: bool,
}

impl Trace {
    fn absorb(&mut self, other: &Self) {
        self.reads.extend(other.reads.iter().cloned());
        self.waits.extend(other.waits.iter().copied());
        self.producers.extend(other.producers.iter().copied());
        self.approximate |= other.approximate;
        for diagnostic in &other.diagnostics {
            if !self.diagnostics.contains(diagnostic) {
                self.diagnostics.push(diagnostic.clone());
            }
        }
    }
}

type Locals = BTreeMap<String, SemanticValue>;
type HypotheticalMemo = BTreeMap<String, (Outcome, BTreeSet<QueryKey>, Vec<Diagnostic>)>;

struct HypotheticalGuard(Rc<RefCell<BTreeSet<String>>>, String);
impl Drop for HypotheticalGuard {
    fn drop(&mut self) {
        self.0.borrow_mut().remove(&self.1);
    }
}

struct Engine<'a> {
    producers: Vec<Producer<'a>>,
    occurrences: BTreeMap<usize, &'a Occurrence>,
    parents: BTreeMap<usize, Option<usize>>,
    terminal_parents: BTreeMap<usize, usize>,
    overrides: BTreeMap<QueryKey, SemanticValue>,
    override_origins: BTreeMap<QueryKey, Span>,
    hypothetical_active: Rc<RefCell<BTreeSet<String>>>,
    hypothetical_depth: usize,
    source: &'a str,
    hypothetical_cache: Rc<RefCell<HypotheticalMemo>>,
}

fn source_identity(source: &SourceCapture) -> Option<SemanticIdentity> {
    match source.occurrence_selection()? {
        OccurrenceSelection::One(id) => Some(SemanticIdentity::Occurrence(id)),
        OccurrenceSelection::Terminal(id) => Some(SemanticIdentity::Service {
            service: "core/syntax".into(),
            id,
        }),
        _ => None,
    }
}

fn equal_values(left: &SemanticValue, right: &SemanticValue) -> bool {
    fn normalize(value: &SemanticValue) -> SemanticValue {
        fn selection(value: OccurrenceSelection) -> SemanticValue {
            match value {
                OccurrenceSelection::One(id) => {
                    SemanticValue::Identity(SemanticIdentity::Occurrence(id))
                }
                OccurrenceSelection::Terminal(id) => {
                    SemanticValue::Identity(SemanticIdentity::Service {
                        service: "core/syntax".into(),
                        id,
                    })
                }
                OccurrenceSelection::List(items) => {
                    SemanticValue::List(items.into_iter().map(selection).collect())
                }
                OccurrenceSelection::Absent => SemanticValue::Absent,
            }
        }
        match value {
            SemanticValue::Source(source) => source
                .occurrence_selection()
                .map(selection)
                .unwrap_or_else(|| value.clone()),
            SemanticValue::Tuple(items) => {
                SemanticValue::Tuple(items.iter().map(normalize).collect())
            }
            SemanticValue::List(items) => {
                SemanticValue::List(items.iter().map(normalize).collect())
            }
            value => value.clone(),
        }
    }
    normalize(left) == normalize(right)
}

fn address(value: &SemanticValue, span: &Span) -> Result<Option<QueryKey>, Diagnostic> {
    Ok(match value {
        SemanticValue::Absent => None,
        SemanticValue::Identity(id) => Some(QueryKey::Identity(id.clone())),
        SemanticValue::Source(source) if source.value == AstValue::None => None,
        SemanticValue::Source(source) => Some(QueryKey::Identity(
            source_identity(source).ok_or_else(|| {
                Diagnostic::error(
                    "query.key",
                    "A key requires one syntax identity, not a repeated capture",
                    source.span.clone(),
                )
            })?,
        )),
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

const VALUE_NODE_LIMIT: usize = 16_384;
const VALUE_BYTE_LIMIT: usize = 1024 * 1024;

/// Bound owned compound values before another propagation round can clone them.
fn value_cost(value: &SemanticValue) -> Result<(usize, usize), &'static str> {
    enum Part<'a> {
        Value(&'a SemanticValue),
        Ast(&'a AstValue),
        Selection(&'a OccurrenceSelection),
    }
    let mut pending = vec![(Part::Value(value), 0usize)];
    let (mut nodes, mut bytes) = (0usize, 0usize);
    while let Some((part, depth)) = pending.pop() {
        nodes += 1;
        if nodes > VALUE_NODE_LIMIT || depth > 128 {
            return Err("Query value exceeds its node/depth limit");
        }
        match part {
            Part::Value(
                SemanticValue::Tuple(items)
                | SemanticValue::List(items)
                | SemanticValue::Alternatives { values: items, .. },
            ) => pending.extend(items.iter().map(|item| (Part::Value(item), depth + 1))),
            Part::Value(SemanticValue::Text(text) | SemanticValue::Symbol(text)) => {
                bytes = bytes.saturating_add(text.len())
            }
            Part::Value(SemanticValue::Deferred { key, .. }) => {
                pending.push((Part::Value(key), depth + 1))
            }
            Part::Value(SemanticValue::Source(source)) => {
                bytes = bytes.saturating_add(source.text.len());
                pending.push((Part::Ast(&source.value), depth + 1));
                if let Some(selection) = &source.selection {
                    pending.push((Part::Selection(selection), depth + 1));
                }
            }
            Part::Ast(AstValue::Node(node)) => pending.extend(
                node.fields
                    .values()
                    .map(|item| (Part::Ast(item), depth + 1)),
            ),
            Part::Ast(AstValue::List(items)) => {
                pending.extend(items.iter().map(|item| (Part::Ast(item), depth + 1)))
            }
            Part::Ast(AstValue::Text(text)) => bytes = bytes.saturating_add(text.len()),
            Part::Selection(OccurrenceSelection::List(items)) => {
                pending.extend(items.iter().map(|item| (Part::Selection(item), depth + 1)))
            }
            _ => {}
        }
        if bytes > VALUE_BYTE_LIMIT {
            return Err("Query value exceeds its one-MiB content limit");
        }
    }
    Ok((nodes, bytes))
}

fn add_value_cost(
    cost: &mut (usize, usize),
    value: &SemanticValue,
    span: &Span,
) -> Result<(), Diagnostic> {
    let next = value_cost(value)
        .map_err(|message| Diagnostic::error("query.resource", message, span.clone()))?;
    cost.0 = cost.0.saturating_add(next.0);
    cost.1 = cost.1.saturating_add(next.1);
    if cost.0 > VALUE_NODE_LIMIT || cost.1 > VALUE_BYTE_LIMIT {
        return Err(Diagnostic::error(
            "query.resource",
            "Query collection exceeds its value budget",
            span.clone(),
        ));
    }
    Ok(())
}

fn add_key_cost(cost: &mut (usize, usize), key: &QueryKey, span: &Span) -> Result<(), Diagnostic> {
    let mut pending = vec![(key, 0usize)];
    while let Some((key, depth)) = pending.pop() {
        cost.0 += 1;
        match key {
            QueryKey::Tuple(items) => pending.extend(items.iter().map(|item| (item, depth + 1))),
            QueryKey::Text(text) | QueryKey::Symbol(text) => {
                cost.1 = cost.1.saturating_add(text.len())
            }
            _ => {}
        }
        if depth > 128 || cost.0 > VALUE_NODE_LIMIT || cost.1 > VALUE_BYTE_LIMIT {
            return Err(Diagnostic::error(
                "query.resource",
                "Branch keys exceed the evaluation limit",
                span.clone(),
            ));
        }
    }
    Ok(())
}

impl<'a> Engine<'a> {
    fn display_value(&self, value: &SemanticValue) -> String {
        match value {
            SemanticValue::Source(source) => source.text.clone(),
            SemanticValue::Text(text) => text.clone(),
            SemanticValue::Symbol(text) => text.rsplit('#').next().unwrap_or(text).into(),
            SemanticValue::Alternatives { values, .. } => values
                .iter()
                .map(|value| self.display_value(value))
                .collect::<Vec<_>>()
                .join(" | "),
            SemanticValue::Bool(value) => value.to_string(),
            SemanticValue::Absent => "absent".into(),
            SemanticValue::Identity(SemanticIdentity::Occurrence(id)) => self
                .occurrences
                .get(id)
                .and_then(|owner| self.source.get(owner.span.start..owner.span.end))
                .unwrap_or("")
                .into(),
            SemanticValue::Tuple(items) => format!(
                "({})",
                items
                    .iter()
                    .map(|value| self.display_value(value))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            SemanticValue::List(items) => format!(
                "[{}]",
                items
                    .iter()
                    .map(|value| self.display_value(value))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            value => serde_json::to_string(value).unwrap_or_else(|_| "<value>".into()),
        }
    }

    fn capture_value(&self, capture: SourceCapture) -> SemanticValue {
        fn materialize(
            engine: &Engine<'_>,
            selected: OccurrenceSelection,
            value: AstValue,
            outer: &SourceCapture,
            position: &mut usize,
        ) -> SemanticValue {
            if let OccurrenceSelection::List(items) = selected {
                let values = match value {
                    AstValue::List(items) => items,
                    _ => vec![],
                };
                return SemanticValue::List(
                    items
                        .into_iter()
                        .enumerate()
                        .map(|(index, selected)| {
                            materialize(
                                engine,
                                selected,
                                values.get(index).cloned().unwrap_or(AstValue::None),
                                outer,
                                position,
                            )
                        })
                        .collect(),
                );
            }
            if matches!(selected, OccurrenceSelection::Absent) {
                return SemanticValue::Absent;
            }
            let span = match &selected {
                OccurrenceSelection::One(id) => engine
                    .occurrences
                    .get(id)
                    .map(|owner| owner.span.clone())
                    .unwrap_or_else(|| outer.span.clone()),
                OccurrenceSelection::Terminal(_) => {
                    let span = outer
                        .element_spans
                        .get(*position)
                        .cloned()
                        .unwrap_or_else(|| outer.span.clone());
                    span
                }
                _ => outer.span.clone(),
            };
            *position += 1;
            let occurrences = match selected {
                OccurrenceSelection::One(id) => vec![id],
                _ => vec![],
            };
            SemanticValue::Source(SourceCapture {
                element_occurrences: vec![],
                element_spans: vec![],
                selection: Some(selected),
                repeated: false,
                value,
                span: span.clone(),
                text: engine.source.get(span.start..span.end).unwrap_or("").into(),
                occurrences,
            })
        }
        if capture.repeated {
            if let Some(selection) = capture.occurrence_selection() {
                return materialize(self, selection, capture.value.clone(), &capture, &mut 0);
            }
        }
        if capture.value == AstValue::None {
            SemanticValue::Absent
        } else {
            SemanticValue::Source(capture)
        }
    }

    fn new(
        language: &CompiledLanguage,
        occurrences: &'a [Occurrence],
        source: &'a str,
        ranks: &BTreeMap<(usize, usize), usize>,
    ) -> Self {
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
            terminal_parents: BTreeMap::new(),
            overrides: BTreeMap::new(),
            override_origins: BTreeMap::new(),
            hypothetical_active: Rc::default(),
            hypothetical_depth: 0,
            source,
            hypothetical_cache: Rc::default(),
        };
        for owner in occurrences {
            visit(owner, None, &mut engine.occurrences, &mut engine.parents);
        }
        fn terminals(
            selection: &OccurrenceSelection,
            owner: usize,
            parents: &mut BTreeMap<usize, usize>,
        ) {
            match selection {
                OccurrenceSelection::Terminal(id) => {
                    parents.insert(*id, owner);
                }
                OccurrenceSelection::List(items) => {
                    for item in items {
                        terminals(item, owner, parents);
                    }
                }
                _ => {}
            }
        }
        for owner in engine.occurrences.values() {
            for capture in owner.captures.values() {
                if let Some(selection) = capture.occurrence_selection() {
                    terminals(&selection, owner.id, &mut engine.terminal_parents);
                }
            }
        }
        // Discovery completes for the entire committed occurrence forest before
        // any get can observe absence. Conditional branches are included too.
        for owner in engine.occurrences.values().copied().collect::<Vec<_>>() {
            if let Some(groups) = language.query_meanings.get(&owner.definition) {
                for group in groups {
                    let original = language.meanings[&owner.definition]
                        .groups
                        .iter()
                        .position(|original| {
                            original.name == group.name && original.span == group.span
                        })
                        .expect("validated query group");
                    let rank = ranks[&(owner.id, original)];
                    engine.discover(owner, &group.actions, &[], rank);
                }
            }
        }
        engine.infer_footprints();
        engine
    }

    fn discover(
        &mut self,
        owner: &'a Occurrence,
        actions: &[QueryAction],
        guards: &[Guard],
        rank: usize,
    ) {
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
                    related,
                    span,
                } => (
                    Effect::Diagnostic {
                        severity: severity.clone(),
                        message: message.clone(),
                        target: target.clone(),
                        related: related.clone(),
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
                    self.discover(owner, then_actions, &nested, rank);
                    *nested.last_mut().expect("new guard") = Guard::If(condition.clone(), false);
                    self.discover(owner, else_actions, &nested, rank);
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
                        self.discover(owner, &case.actions, &nested, rank);
                    }
                    continue;
                }
            };
            self.producers.push(Producer {
                owner,
                order: (rank, self.producers.len()),
                approximate: false,
                effect,
                guards: guards.to_vec(),
                span: span.clone(),
                state: ProducerState::Pending,
                footprint: Pattern::Any,
                reads: BTreeSet::new(),
                waits: BTreeSet::new(),
                diagnostics: vec![],
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
        let mut current = match identity {
            Some(SemanticIdentity::Occurrence(id)) => self.parents.get(&id).copied().flatten(),
            Some(SemanticIdentity::Service { service, id }) if service == "core/syntax" => {
                self.terminal_parents.get(&id).copied()
            }
            _ => {
                return Err(Diagnostic::error(
                    "query.ancestors",
                    "Expected a syntax occurrence",
                    span.clone(),
                ))
            }
        };
        let mut values = vec![];
        while let Some(id) = current {
            values.push(SemanticValue::Identity(SemanticIdentity::Occurrence(id)));
            current = self.parents.get(&id).copied().flatten();
        }
        Ok(values)
    }

    fn lookup(&self, key: &QueryKey, all: bool, facts: bool, trace: &mut Trace) -> Outcome {
        self.lookup_visible(key, all, facts, false, trace)
    }

    fn lookup_visible(
        &self,
        key: &QueryKey,
        all: bool,
        facts: bool,
        earlier: bool,
        trace: &mut Trace,
    ) -> Outcome {
        if earlier && trace.order.is_none() {
            return Outcome::Pending;
        }
        trace.reads.insert(key.clone());
        trace.approximate |= facts;
        if let Some(value) = self.overrides.get(key) {
            return Outcome::Ready(if all || facts {
                SemanticValue::List(match value {
                    SemanticValue::Alternatives { values, .. } => values.clone(),
                    value => vec![value.clone()],
                })
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
            ) || (earlier && Some(producer.order) >= trace.order)
                || !producer.footprint.matches(key)
            {
                continue;
            }
            trace.producers.insert(id);
            trace.approximate |= producer.approximate;
            for state in producer.state.leaves() {
                match state {
                    ProducerState::Pending => {
                        pending = true;
                        trace.waits.insert(id);
                    }
                    ProducerState::Contribution(actual, value) if actual == key => {
                        if !values.iter().zip(&producers).any(|(previous, producer)| {
                            *producer == id && equal_values(previous, value)
                        }) {
                            values.push(value.clone());
                            producers.push(id);
                        }
                    }
                    ProducerState::Contributions(actual, items) if actual == key => {
                        for value in items {
                            if !values.iter().zip(&producers).any(|(previous, producer)| {
                                *producer == id && equal_values(previous, value)
                            }) {
                                values.push(value.clone());
                                producers.push(id);
                            }
                        }
                    }
                    ProducerState::Conflict(waiting) => {
                        pending = true;
                        trace.waits.extend(waiting);
                    }
                    _ => {}
                }
            }
        }
        if facts {
            let mut unique = vec![];
            for value in values {
                if !unique.iter().any(|item| equal_values(item, &value)) {
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
        let result = stacker::maybe_grow(512 * 1024, 2 * 1024 * 1024, || {
            self.eval_inner(expression, owner, locals, trace)
        })?;
        if let Outcome::Ready(value) = &result {
            value_cost(value).map_err(|message| {
                Diagnostic::error("query.resource", message, owner.span.clone())
            })?;
        }
        Ok(result)
    }

    fn eval_inner(
        &self,
        expression: &QueryExpr,
        owner: &Occurrence,
        locals: &Locals,
        trace: &mut Trace,
    ) -> Result<Outcome, Diagnostic> {
        if let Some((_, value)) = trace
            .branch_values
            .iter()
            .rev()
            .find(|(input, _)| input == expression)
        {
            return Ok(Outcome::Ready(value.clone()));
        }
        let ready = |value| Ok(Outcome::Ready(value));
        match expression {
            QueryExpr::SelfValue => ready(SemanticValue::Identity(SemanticIdentity::Occurrence(
                owner.id,
            ))),
            QueryExpr::Capture(name) => ready(self.capture_value(owner.capture_path(name)?)),
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
                let mut cost = (1, 0);
                for item in items {
                    match self.eval(item, owner, locals, trace)? {
                        Outcome::Ready(value) => {
                            add_value_cost(&mut cost, &value, &owner.span)?;
                            values.push(value)
                        }
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
                Outcome::Ready(SemanticValue::Absent) => ready(SemanticValue::Absent),
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

    // Address evaluation can rule out a path even when another component is
    // pending. Ordinary data tuples still preserve absent fields and shapes.
    fn eval_key(
        &self,
        expression: &QueryExpr,
        owner: &Occurrence,
        locals: &Locals,
        trace: &mut Trace,
    ) -> Result<Outcome, Diagnostic> {
        let QueryExpr::Tuple(parts) = expression else {
            return self.eval(expression, owner, locals, trace);
        };
        let mut values = vec![];
        let mut cost = (1, 0);
        let mut blocked = None;
        for part in parts {
            match self.eval_key(part, owner, locals, trace)? {
                Outcome::Ready(value) if address(&value, &owner.span)?.is_none() => {
                    return Ok(Outcome::Ready(SemanticValue::Absent));
                }
                Outcome::Ready(value) => {
                    add_value_cost(&mut cost, &value, &owner.span)?;
                    values.push(value);
                }
                outcome => blocked = Some(outcome),
            }
        }
        Ok(blocked.unwrap_or(Outcome::Ready(SemanticValue::Tuple(values))))
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
        if matches!(name, "get" | "getEarlier" | "getAll" | "facts") {
            return match self.eval_key(&args[0], owner, locals, trace)? {
                Outcome::Ready(value) => match address(&value, &owner.span)? {
                    Some(key) => Ok(self.lookup_visible(
                        &key,
                        name == "getAll",
                        name == "facts",
                        name == "getEarlier",
                        trace,
                    )),
                    None => Ok(Outcome::Ready(SemanticValue::Absent)),
                },
                outcome => Ok(outcome),
            };
        }
        if name == "choose" {
            return match self.eval(&args[0], owner, locals, trace)? {
                Outcome::Ready(SemanticValue::Bool(condition)) => {
                    self.eval(&args[if condition { 1 } else { 2 }], owner, locals, trace)
                }
                Outcome::Ready(SemanticValue::Absent) => Ok(Outcome::Ready(SemanticValue::Absent)),
                Outcome::Ready(_) => Err(bad()),
                value => Ok(value),
            };
        }
        if matches!(name, "find" | "map" | "every" | "filter") {
            let values = match self.eval(&args[0], owner, locals, trace)? {
                Outcome::Ready(SemanticValue::List(values)) => values,
                Outcome::Ready(SemanticValue::Absent) => {
                    return Ok(Outcome::Ready(SemanticValue::Absent))
                }
                Outcome::Ready(_) => return Err(bad()),
                pending => return Ok(pending),
            };
            let QueryExpr::Lambda { parameter, body } = &args[1] else {
                return Err(bad());
            };
            let mut locals = locals.clone();
            let mut mapped = vec![];
            let mut cost = (1, 0);
            let mut pending = false;
            for value in values {
                locals.insert(parameter.clone(), value.clone());
                let result = self.eval(body, owner, &locals, trace)?;
                match (name, result) {
                    ("find", Outcome::Ready(SemanticValue::Absent)) => {}
                    ("find", result) => return Ok(result),
                    ("map", Outcome::Ready(value)) => {
                        add_value_cost(&mut cost, &value, &owner.span)?;
                        mapped.push(value)
                    }
                    ("filter", Outcome::Ready(SemanticValue::Bool(true))) => mapped.push(value),
                    ("filter", Outcome::Ready(SemanticValue::Bool(false))) => {}
                    ("filter", Outcome::Ready(SemanticValue::Absent)) => {}
                    ("every", Outcome::Ready(SemanticValue::Absent)) => {
                        return Ok(Outcome::Ready(SemanticValue::Absent))
                    }
                    ("every", Outcome::Ready(SemanticValue::Bool(false))) => {
                        return Ok(Outcome::Ready(SemanticValue::Bool(false)))
                    }
                    ("every", Outcome::Ready(SemanticValue::Bool(true))) => {}
                    (_, Outcome::Pending) => pending = true,
                    (_, Outcome::Conflict(ids)) => return Ok(Outcome::Conflict(ids)),
                    _ => return Err(bad()),
                }
            }
            return Ok(if pending {
                Outcome::Pending
            } else {
                Outcome::Ready(match name {
                    "map" | "filter" => SemanticValue::List(mapped),
                    "every" => SemanticValue::Bool(true),
                    _ => SemanticValue::Absent,
                })
            });
        }
        // Presence inspectors and explicit value materialization observe absence.
        // Other pure primitives end only this dependent expression, not its caller.
        let strict = !matches!(name, "isPresent" | "equal" | "isList" | "values");
        let mut values = vec![];
        let mut blocked = None;
        for arg in args {
            match self.eval(arg, owner, locals, trace)? {
                Outcome::Ready(SemanticValue::Absent) if strict => {
                    return Ok(Outcome::Ready(SemanticValue::Absent));
                }
                Outcome::Ready(value) => values.push(value),
                outcome if strict => blocked = Some(outcome),
                outcome => return Ok(outcome),
            }
        }
        if let Some(blocked) = blocked {
            return Ok(blocked);
        }
        let result = match (name, values.as_slice()) {
            ("ancestors", [value]) => SemanticValue::List(self.ancestors(value, &owner.span)?),
            ("children", [value]) => {
                let identity = match value {
                    SemanticValue::Identity(identity) => Some(identity.clone()),
                    SemanticValue::Source(source) => source_identity(source),
                    _ => None,
                };
                let children = match identity {
                    Some(SemanticIdentity::Occurrence(id)) => self
                        .occurrences
                        .get(&id)
                        .ok_or_else(bad)?
                        .children
                        .iter()
                        .map(|owner| {
                            SemanticValue::Identity(SemanticIdentity::Occurrence(owner.id))
                        })
                        .collect(),
                    Some(SemanticIdentity::Service { service, .. }) if service == "core/syntax" => {
                        vec![]
                    }
                    _ => return Err(bad()),
                };
                SemanticValue::List(children)
            }
            ("text", [SemanticValue::Source(source)]) => SemanticValue::Text(source.text.clone()),
            ("text", [SemanticValue::Text(text)]) => SemanticValue::Text(text.clone()),
            ("text", [SemanticValue::Identity(SemanticIdentity::Occurrence(id))]) => {
                let owner = self.occurrences.get(id).ok_or_else(bad)?;
                SemanticValue::Text(
                    self.source
                        .get(owner.span.start..owner.span.end)
                        .unwrap_or("")
                        .into(),
                )
            }
            ("isPresent", [value]) => SemanticValue::Bool(!matches!(value, SemanticValue::Absent)),
            ("equal", [left, right]) => SemanticValue::Bool(equal_values(left, right)),
            ("hasMultiple", [SemanticValue::List(values)]) => SemanticValue::Bool(values.len() > 1),
            ("values", [SemanticValue::Alternatives { values, .. }]) => {
                SemanticValue::List(values.clone())
            }
            ("values", [SemanticValue::Absent]) => SemanticValue::List(vec![]),
            ("values", [value]) => SemanticValue::List(vec![value.clone()]),
            ("alternatives", [SemanticValue::List(values)]) => {
                let mut unique = vec![];
                for value in values {
                    if !unique.iter().any(|item| equal_values(item, value)) {
                        unique.push(value.clone());
                    }
                }
                match unique.len() {
                    0 => SemanticValue::Absent,
                    1 => unique.remove(0),
                    _ => SemanticValue::Alternatives {
                        service: "core/query".into(),
                        values: unique,
                    },
                }
            }
            ("flatten", [SemanticValue::List(values)]) => {
                let mut items = vec![];
                for value in values {
                    match value {
                        SemanticValue::List(values) => items.extend(values.clone()),
                        _ => return Err(bad()),
                    }
                }
                SemanticValue::List(items)
            }
            ("isList", [value]) => SemanticValue::Bool(matches!(value, SemanticValue::List(_))),
            ("tail", [SemanticValue::List(values) | SemanticValue::Tuple(values)]) => {
                SemanticValue::List(values.iter().skip(1).cloned().collect())
            }
            ("last", [SemanticValue::List(values) | SemanticValue::Tuple(values)]) => {
                values.last().cloned().unwrap_or(SemanticValue::Absent)
            }
            ("first", [SemanticValue::List(values) | SemanticValue::Tuple(values)]) => {
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
            ("contains", [SemanticValue::List(values), value]) => {
                SemanticValue::Bool(values.iter().any(|item| equal_values(item, value)))
            }
            ("unique", [SemanticValue::List(values)]) => {
                let mut unique = vec![];
                for value in values {
                    if !unique.iter().any(|item| equal_values(item, value)) {
                        unique.push(value.clone());
                    }
                }
                SemanticValue::List(unique)
            }
            ("isSubset", [SemanticValue::List(left), SemanticValue::List(right)]) => {
                SemanticValue::Bool(
                    left.iter()
                        .all(|value| right.iter().any(|item| equal_values(value, item))),
                )
            }
            ("lookup", [SemanticValue::List(fields), key]) => {
                let mut result = None;
                for field in fields {
                    let SemanticValue::Tuple(pair) = field else {
                        return Err(bad());
                    };
                    if pair.len() != 2 {
                        return Err(bad());
                    }
                    if equal_values(&pair[0], key) {
                        if result.is_some() {
                            return Err(Diagnostic::error(
                                "query.map_duplicate",
                                "Map lookup has multiple entries for the same key",
                                owner.span.clone(),
                            ));
                        }
                        result = Some(pair[1].clone());
                    }
                }
                result.unwrap_or(SemanticValue::Absent)
            }
            ("keys", [SemanticValue::List(fields)]) => {
                let mut keys = vec![];
                for field in fields {
                    let SemanticValue::Tuple(pair) = field else {
                        return Err(bad());
                    };
                    if pair.len() != 2 {
                        return Err(bad());
                    }
                    keys.push(pair[0].clone());
                }
                SemanticValue::List(keys)
            }
            ("zip", [SemanticValue::List(left), SemanticValue::List(right)]) => {
                SemanticValue::List(
                    left.iter()
                        .zip(right)
                        .map(|(left, right)| {
                            SemanticValue::Tuple(vec![left.clone(), right.clone()])
                        })
                        .collect(),
                )
            }
            (
                "hypothetical" | "hypotheticalAll" | "hypotheticalChecked",
                [key, SemanticValue::List(assumptions)],
            ) => {
                return self.hypothetical(
                    key,
                    assumptions,
                    trace,
                    &owner.span,
                    name != "hypothetical",
                    name == "hypotheticalChecked",
                );
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
        let mut trace = Trace::default();
        if let Ok(Outcome::Ready(value)) = self.eval(expr, owner, locals, &mut trace) {
            if !trace.approximate {
                if let Ok(key) = address(&value, &owner.span) {
                    return key.map(Pattern::Exact).unwrap_or(Pattern::Empty);
                }
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
                    locals.insert(parameter.clone(), value.clone());
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

    fn guard_branches(
        &self,
        guard: &Guard,
        owner: &Occurrence,
        mut trace: Trace,
    ) -> Result<Vec<(Outcome, Trace)>, Diagnostic> {
        match guard {
            Guard::If(condition, expected) => {
                let outcome = match self.eval(condition, owner, &Locals::new(), &mut trace)? {
                    Outcome::Ready(SemanticValue::Bool(value)) => {
                        Outcome::Ready(SemanticValue::Bool(value == *expected))
                    }
                    Outcome::Ready(SemanticValue::Absent) => {
                        Outcome::Ready(SemanticValue::Bool(false))
                    }
                    Outcome::Ready(_) => {
                        return Err(Diagnostic::error(
                            "query.condition",
                            "An if condition must be boolean",
                            owner.span.clone(),
                        ))
                    }
                    value => value,
                };
                Ok(vec![(outcome, trace)])
            }
            Guard::Case {
                inputs,
                cases,
                selected,
            } => {
                let mut rows = vec![(vec![], trace)];
                let mut blocked = vec![];
                for input in inputs {
                    let mut next = vec![];
                    for (prefix, mut context) in rows {
                        let candidates =
                            match self.eval(input, owner, &Locals::new(), &mut context)? {
                                Outcome::Ready(SemanticValue::Alternatives { values, .. }) => {
                                    values
                                }
                                Outcome::Ready(value) => vec![value],
                                pending => {
                                    blocked.push((pending, context));
                                    continue;
                                }
                            };
                        if next.len().saturating_add(candidates.len()) > 4096 {
                            return Err(Diagnostic::error(
                                "query.resource",
                                "Match alternatives exceed the evaluation limit",
                                owner.span.clone(),
                            ));
                        }
                        for value in candidates {
                            let mut context = context.clone();
                            context.branch_values.push((input.clone(), value.clone()));
                            let mut values = prefix.clone();
                            values.push(value);
                            next.push((values, context));
                        }
                    }
                    rows = next;
                }
                for (values, mut context) in rows {
                    let mut outcome = Outcome::Ready(SemanticValue::Bool(false));
                    'cases: for (index, case) in cases.iter().enumerate() {
                        for alternative in &case.alternatives {
                            let mut matches = alternative.len() == values.len();
                            for (pattern, value) in alternative.iter().zip(&values) {
                                if let Some(pattern) = pattern {
                                    match self.eval(pattern, owner, &Locals::new(), &mut context)? {
                                        Outcome::Ready(pattern) => {
                                            matches &= equal_values(&pattern, value)
                                        }
                                        pending => {
                                            outcome = pending;
                                            break 'cases;
                                        }
                                    }
                                }
                            }
                            if matches {
                                outcome = Outcome::Ready(SemanticValue::Bool(index == *selected));
                                break 'cases;
                            }
                        }
                    }
                    blocked.push((outcome, context));
                }
                Ok(blocked)
            }
        }
    }

    fn evaluate_producer(
        &self,
        producer: &Producer<'a>,
        trace: &mut Trace,
    ) -> Result<ProducerState, Diagnostic> {
        trace.order = Some(producer.order);
        let mut contexts = vec![(Outcome::Ready(SemanticValue::Bool(true)), trace.clone())];
        for guard in &producer.guards {
            let mut next = vec![];
            for (previous, context) in contexts {
                for (outcome, context) in self.guard_branches(guard, producer.owner, context)? {
                    trace.absorb(&context);
                    match outcome {
                        Outcome::Ready(SemanticValue::Bool(false)) => {}
                        Outcome::Ready(_) => next.push((previous.clone(), context)),
                        blocked => next.push((blocked, context)),
                    }
                    if next.len() > 4096 {
                        return Err(Diagnostic::error(
                            "query.resource",
                            "Nested match alternatives exceed the evaluation limit",
                            producer.owner.span.clone(),
                        ));
                    }
                }
            }
            contexts = next;
        }
        let mut states = vec![];
        let mut cost = (0, 0);
        for (guard, mut context) in contexts {
            let state = match guard {
                Outcome::Pending => ProducerState::Pending,
                Outcome::Conflict(ids) => ProducerState::Conflict(ids),
                _ => self.evaluate_effect(producer, &mut context)?,
            };
            trace.absorb(&context);
            if states.contains(&state) || matches!(state, ProducerState::Absent) {
                continue;
            }
            match &state {
                ProducerState::Contribution(key, value) => {
                    add_key_cost(&mut cost, key, &producer.span)?;
                    add_value_cost(&mut cost, value, &producer.span)?
                }
                ProducerState::Contributions(key, values) => {
                    add_key_cost(&mut cost, key, &producer.span)?;
                    for value in values {
                        add_value_cost(&mut cost, value, &producer.span)?;
                    }
                }
                ProducerState::Diagnostic(diagnostic) => {
                    cost.0 += 1;
                    cost.1 = cost.1.saturating_add(diagnostic.message.len());
                    if cost.0 > VALUE_NODE_LIMIT || cost.1 > VALUE_BYTE_LIMIT {
                        return Err(Diagnostic::error(
                            "query.resource",
                            "Branch diagnostics exceed the evaluation limit",
                            producer.span.clone(),
                        ));
                    }
                }
                _ => {}
            }
            states.push(state);
        }
        Ok(match states.len() {
            0 => ProducerState::Absent,
            1 => states.remove(0),
            _ => ProducerState::Many(states),
        })
    }

    fn evaluate_effect(
        &self,
        producer: &Producer<'a>,
        trace: &mut Trace,
    ) -> Result<ProducerState, Diagnostic> {
        match &producer.effect {
            Effect::Contribution(key, value) | Effect::Contributions(key, value) => {
                let key = match self.eval_key(key, producer.owner, &Locals::new(), trace)? {
                    Outcome::Ready(value) => address(&value, &producer.span)?,
                    Outcome::Pending => {
                        // A missing value cannot contribute even to a pending key.
                        return Ok(
                            match self.eval(value, producer.owner, &Locals::new(), trace)? {
                                Outcome::Ready(SemanticValue::Absent) => ProducerState::Absent,
                                Outcome::Ready(SemanticValue::List(values))
                                    if values.is_empty()
                                        && matches!(producer.effect, Effect::Contributions(..)) =>
                                {
                                    ProducerState::Absent
                                }
                                _ => ProducerState::Pending,
                            },
                        );
                    }
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
                            if !matches!(value, SemanticValue::Absent)
                                && !unique.iter().any(|item| equal_values(item, &value))
                            {
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
                related,
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
                    let fragment = match part {
                        QueryMessagePart::Text(value) => value.clone(),
                        QueryMessagePart::Value(value) => {
                            match self.eval(value, producer.owner, &Locals::new(), trace)? {
                                Outcome::Ready(value) => self.display_value(&value),
                                Outcome::Pending => return Ok(ProducerState::Pending),
                                Outcome::Conflict(ids) => return Ok(ProducerState::Conflict(ids)),
                            }
                        }
                    };
                    if text.len().saturating_add(fragment.len()) > VALUE_BYTE_LIMIT {
                        return Err(Diagnostic::error(
                            "query.resource",
                            "Diagnostic message exceeds the evaluation limit",
                            producer.span.clone(),
                        ));
                    }
                    text.push_str(&fragment);
                }
                let mut diagnostic = Diagnostic::error("query.diagnostic", text, span);
                diagnostic.severity = severity.clone();
                diagnostic.secondary.push(producer.span.clone());
                for related in related {
                    match self.eval(related, producer.owner, &Locals::new(), trace)? {
                        Outcome::Ready(SemanticValue::Source(source)) => {
                            diagnostic.secondary.push(source.span)
                        }
                        Outcome::Ready(SemanticValue::Identity(SemanticIdentity::Occurrence(
                            id,
                        ))) => {
                            if let Some(owner) = self.occurrences.get(&id) {
                                diagnostic.secondary.push(owner.span.clone());
                            }
                        }
                        Outcome::Ready(SemanticValue::Absent) => {}
                        Outcome::Ready(_) => {
                            return Err(Diagnostic::error(
                                "query.diagnostic_target",
                                "Related diagnostic location must be syntax or absent",
                                producer.owner.span.clone(),
                            ))
                        }
                        Outcome::Pending => return Ok(ProducerState::Pending),
                        Outcome::Conflict(ids) => return Ok(ProducerState::Conflict(ids)),
                    }
                }
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
                        Ok(state) => {
                            failures.remove(&id);
                            state
                        }
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
                producer.diagnostics = trace.diagnostics;
                changed |= producer.approximate != trace.approximate;
                producer.approximate = trace.approximate;
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
        all: bool,
        checked: bool,
    ) -> Result<Outcome, Diagnostic> {
        if self.hypothetical_depth >= 12 {
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
            terminal_parents: self.terminal_parents.clone(),
            overrides: self.overrides.clone(),
            override_origins: self.override_origins.clone(),
            hypothetical_active: self.hypothetical_active.clone(),
            hypothetical_depth: self.hypothetical_depth + 1,
            source: self.source,
            hypothetical_cache: self.hypothetical_cache.clone(),
        };
        let mut explicit = BTreeSet::new();
        for assumption in assumptions {
            let SemanticValue::Tuple(parts) = assumption else {
                return Err(Diagnostic::error(
                    "query.assumption",
                    "Each assumption is a (key, value) tuple",
                    span.clone(),
                ));
            };
            if !(2..=3).contains(&parts.len()) {
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
            if !explicit.insert(key.clone()) {
                return Err(Diagnostic::error(
                    "query.assumption",
                    "An assumption key appears more than once",
                    span.clone(),
                ));
            }
            copy.override_origins.remove(&key);
            if let Some(origin) = parts.get(2) {
                let origin = match origin {
                    SemanticValue::Source(source) => Some(source.span.clone()),
                    SemanticValue::Identity(SemanticIdentity::Occurrence(id)) => {
                        copy.occurrences.get(id).map(|owner| owner.span.clone())
                    }
                    SemanticValue::Absent => None,
                    _ => {
                        return Err(Diagnostic::error(
                            "query.assumption",
                            "An assumption origin must be syntax or absent",
                            span.clone(),
                        ))
                    }
                };
                if let Some(origin) = origin {
                    copy.override_origins.insert(key.clone(), origin);
                }
            }
            copy.overrides.insert(key, parts[1].clone());
        }
        let memo_key = serde_json::to_string(&(
            key.clone(),
            all,
            checked,
            checked.then_some(span),
            copy.override_origins.iter().collect::<Vec<_>>(),
            copy.overrides.iter().collect::<Vec<_>>(),
        ))
        .map_err(|error| Diagnostic::error("query.assumption", error.to_string(), span.clone()))?;
        if let Some((result, reads, diagnostics)) = self.hypothetical_cache.borrow().get(&memo_key)
        {
            trace.reads.extend(reads.clone());
            trace.diagnostics.extend(diagnostics.clone());
            return Ok(result.clone());
        }
        if !self
            .hypothetical_active
            .borrow_mut()
            .insert(memo_key.clone())
        {
            return Err(Diagnostic::error(
                "query.hypothetical_cycle",
                "A hypothetical query depends on its own result",
                span.clone(),
            ));
        }
        let _active_guard = HypotheticalGuard(self.hypothetical_active.clone(), memo_key.clone());
        for producer in &mut copy.producers {
            producer.state = ProducerState::Pending;
            producer.reads.clear();
            producer.waits.clear();
            producer.diagnostics.clear();
            producer.approximate = false;
        }
        copy.infer_footprints();
        // Demand only producers of the requested answer and the producers read
        // while evaluating them. Replaying unrelated hypothetical callers would
        // recursively invoke the same query despite no dependency on its result.
        let mut active = BTreeSet::new();
        let mut converged = false;
        let mut failures = BTreeMap::new();
        for _ in 0..(copy.producers.len() + 1).saturating_mul(8).max(32) {
            let mut root_trace = Trace::default();
            copy.lookup(&key, all, false, &mut root_trace);
            let old_count = active.len();
            active.extend(root_trace.producers);
            if checked {
                let owners: BTreeSet<_> = active
                    .iter()
                    .map(|id| copy.producers[*id].owner.id)
                    .collect();
                active.extend(
                    copy.producers
                        .iter()
                        .enumerate()
                        .filter_map(|(id, producer)| {
                            (owners.contains(&producer.owner.id)
                                && matches!(producer.effect, Effect::Diagnostic { .. }))
                            .then_some(id)
                        }),
                );
            }
            let mut changed = old_count != active.len();
            let mut updates = vec![];
            for id in active.iter().copied() {
                let mut reads = Trace::default();
                let state = match copy.evaluate_producer(&copy.producers[id], &mut reads) {
                    Ok(state) => {
                        failures.remove(&id);
                        state
                    }
                    Err(error) => {
                        failures.insert(id, error);
                        ProducerState::Absent
                    }
                };
                updates.push((id, state, reads));
            }
            for (id, state, reads) in updates {
                changed |= copy.producers[id].state != state;
                let old_count = active.len();
                active.extend(reads.producers);
                changed |= old_count != active.len();
                copy.producers[id].state = state;
                copy.producers[id].reads = reads.reads;
                copy.producers[id].waits = reads.waits;
                copy.producers[id].diagnostics = reads.diagnostics;
                changed |= copy.producers[id].approximate != reads.approximate;
                copy.producers[id].approximate = reads.approximate;
            }
            changed |= copy.infer_footprints();
            if !changed {
                converged = true;
                break;
            }
        }
        if !converged {
            return Err(Diagnostic::error(
                "query.nonconvergent",
                "Hypothetical query propagation did not converge",
                span.clone(),
            ));
        }
        if let Some(error) = failures.into_values().next() {
            return Err(error);
        }
        let result = copy.lookup(&key, all, false, trace);
        let mut receipts = vec![];
        if checked && matches!(result, Outcome::Ready(_)) {
            for id in &active {
                for leaf in copy.producers[*id].state.leaves() {
                    if let ProducerState::Diagnostic(diagnostic) = leaf {
                        let mut diagnostic = diagnostic.clone();
                        diagnostic.secondary.push(*diagnostic.primary.clone());
                        let mut keys = copy.producers[*id].reads.clone();
                        let mut seen = BTreeSet::new();
                        let mut origins = vec![];
                        while let Some(key) = keys.pop_first() {
                            if !seen.insert(key.clone()) {
                                continue;
                            }
                            if let Some(origin) = copy.override_origins.get(&key) {
                                if !origins.contains(origin) {
                                    origins.push(origin.clone());
                                }
                            } else {
                                for producer in &copy.producers {
                                    if producer.footprint.matches(&key) {
                                        keys.extend(producer.reads.clone());
                                    }
                                }
                            }
                        }
                        *diagnostic.primary = if origins.len() == 1 {
                            origins.remove(0)
                        } else {
                            span.clone()
                        };
                        if !receipts.contains(&diagnostic) {
                            receipts.push(diagnostic);
                        }
                    }
                }
                receipts.extend(copy.producers[*id].diagnostics.clone());
            }
            trace.diagnostics.extend(receipts.clone());
        }
        // Dependencies on shared reads remain visible to the caller. All writes
        // and diagnostic contributions live only in the isolated calculation.
        let mut shared_reads = BTreeSet::new();
        for producer in &copy.producers {
            shared_reads.extend(
                producer
                    .reads
                    .iter()
                    .filter(|key| !copy.overrides.contains_key(*key))
                    .cloned(),
            );
            trace.reads.extend(
                producer
                    .reads
                    .iter()
                    .filter(|key| !copy.overrides.contains_key(*key))
                    .cloned(),
            );
        }
        if matches!(result, Outcome::Ready(_)) {
            self.hypothetical_cache
                .borrow_mut()
                .insert(memo_key, (result.clone(), shared_reads, receipts));
        }
        Ok(result)
    }
}

pub(super) fn execute(
    language: &CompiledLanguage,
    occurrences: &[Occurrence],
    graph: &mut SemanticGraph,
    diagnostics: &mut Vec<Diagnostic>,
    source: &str,
) {
    if language.query_meanings.is_empty() {
        return;
    }
    let ranks = match super::scheduler::query_group_ranks(language, occurrences) {
        Ok(ranks) => ranks,
        Err(errors) => {
            diagnostics.extend(errors);
            return;
        }
    };
    let mut engine = Engine::new(language, occurrences, source, &ranks);
    diagnostics.extend(engine.settle());
    for (id, producer) in engine.producers.iter().enumerate() {
        if !matches!(
            producer.state,
            ProducerState::Pending | ProducerState::Conflict(_)
        ) {
            for diagnostic in &producer.diagnostics {
                if !diagnostics.contains(diagnostic) {
                    diagnostics.push(diagnostic.clone());
                }
            }
        }
        let mut state = "absent";
        for leaf in producer.state.leaves() {
            let leaf_state = match leaf {
                ProducerState::Contribution(key, value) => {
                    if !graph.query_entries.iter().any(|entry| {
                        entry.producer == id
                            && entry.key == *key
                            && equal_values(&entry.value, value)
                    }) {
                        graph.query_entries.push(QueryEntry {
                            key: key.clone(),
                            value: value.clone(),
                            producer: id,
                            span: producer.owner.span.clone(),
                        });
                    }
                    "present"
                }
                ProducerState::Contributions(key, values) => {
                    for value in values {
                        if graph.query_entries.iter().any(|entry| {
                            entry.producer == id
                                && entry.key == *key
                                && equal_values(&entry.value, value)
                        }) {
                            continue;
                        }
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
                    if !diagnostics.contains(diagnostic) {
                        diagnostics.push(diagnostic.clone());
                    }
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
                ProducerState::Conflict(ids) => {
                    let mut diagnostic = Diagnostic::error(
                        "query.conflict",
                        "A query has multiple contributions and cannot select one value",
                        producer.owner.span.clone(),
                    );
                    diagnostic.secondary = ids
                        .iter()
                        .filter_map(|id| engine.producers.get(*id))
                        .map(|producer| producer.owner.span.clone())
                        .collect();
                    diagnostics.push(diagnostic);
                    "conflict"
                }
                ProducerState::Many(_) => unreachable!("producer states are flattened"),
            };
            if leaf_state == "pending"
                || (leaf_state == "conflict" && state != "pending")
                || (leaf_state == "present" && state == "absent")
            {
                state = leaf_state;
            }
        }
        graph.query_dependencies.push(QueryDependency {
            producer: id,
            state: state.into(),
            reads: producer.reads.iter().cloned().collect(),
            waits_for: producer.waits.iter().copied().collect(),
        });
    }
}

pub(crate) fn validate_groups(
    groups: &[QueryGroup],
    captures: &BTreeSet<String>,
    version: u32,
) -> Result<(), Diagnostic> {
    if version < 4 {
        fn new_name(name: &str) -> bool {
            matches!(
                primitive(name),
                "getEarlier"
                    | "filter"
                    | "choose"
                    | "values"
                    | "alternatives"
                    | "flatten"
                    | "last"
                    | "tail"
                    | "isList"
                    | "hypotheticalAll"
                    | "hypotheticalChecked"
            )
        }
        let mut actions: Vec<_> = groups.iter().flat_map(|group| &group.actions).collect();
        let mut expressions = vec![];
        while let Some(action) = actions.pop() {
            match action {
                QueryAction::Call { name, args, .. } => {
                    if new_name(name) {
                        return Err(Diagnostic::error(
                            "yl.artifact",
                            "These query primitives require artifact version 4",
                            Span::default(),
                        ));
                    }
                    expressions.extend(args);
                }
                QueryAction::If {
                    condition,
                    then_actions,
                    else_actions,
                    ..
                } => {
                    expressions.push(condition);
                    actions.extend(then_actions);
                    actions.extend(else_actions);
                }
                QueryAction::Match { inputs, cases, .. } => {
                    expressions.extend(inputs);
                    for case in cases {
                        expressions.extend(case.alternatives.iter().flatten().flatten());
                        actions.extend(&case.actions);
                    }
                }
                QueryAction::Emit {
                    message,
                    target,
                    related,
                    ..
                } => {
                    if !related.is_empty() {
                        return Err(Diagnostic::error(
                            "yl.artifact",
                            "Related diagnostic sources require artifact version 4",
                            Span::default(),
                        ));
                    }
                    expressions.extend(target);
                    expressions.extend(message.iter().filter_map(|part| match part {
                        QueryMessagePart::Value(value) => Some(value),
                        _ => None,
                    }));
                }
            }
        }
        while let Some(expression) = expressions.pop() {
            match expression {
                QueryExpr::Call { name, args } => {
                    if new_name(name) {
                        return Err(Diagnostic::error(
                            "yl.artifact",
                            "These query primitives require artifact version 4",
                            Span::default(),
                        ));
                    }
                    expressions.extend(args);
                }
                QueryExpr::Tuple(values) | QueryExpr::List(values) => expressions.extend(values),
                QueryExpr::Not(value) | QueryExpr::Lambda { body: value, .. } => {
                    expressions.push(value)
                }
                _ => {}
            }
        }
    }
    fn arity(name: &str) -> Option<usize> {
        if !name.starts_with("std/query#") {
            return None;
        }
        match primitive(name) {
            "get" | "getEarlier" | "getAll" | "ancestors" | "children" | "text" | "isPresent"
            | "hasMultiple" | "first" | "last" | "tail" | "facts" | "concat" | "unique"
            | "keys" | "values" | "alternatives" | "flatten" | "isList" => Some(1),
            "contribute"
            | "contributeAll"
            | "find"
            | "map"
            | "every"
            | "equal"
            | "hypothetical"
            | "hypotheticalAll"
            | "hypotheticalChecked"
            | "filter"
            | "contains"
            | "isSubset"
            | "lookup"
            | "zip" => Some(2),
            "choose" => Some(3),
            _ => None,
        }
    }
    fn expr(
        value: &QueryExpr,
        captures: &BTreeSet<String>,
        locals: &BTreeSet<String>,
        span: &Span,
        depth: usize,
    ) -> Result<(), Diagnostic> {
        let bad = |message: String| Diagnostic::error("yl.artifact", message, span.clone());
        if depth > 128 {
            return Err(bad("Query nesting exceeds the resource limit".into()));
        }
        match value {
            QueryExpr::Capture(name)
                if !captures.contains(name.split('.').next().unwrap_or(name)) =>
            {
                Err(bad(format!("Query references unavailable capture {name}")))
            }
            QueryExpr::Local(name) if !locals.contains(name) => {
                Err(bad(format!("Unbound query callback parameter {name}")))
            }
            QueryExpr::Call { name, args } => {
                if arity(name) != Some(args.len())
                    || matches!(primitive(name), "contribute" | "contributeAll")
                {
                    return Err(bad("Invalid query primitive or argument count".into()));
                }
                for (index, arg) in args.iter().enumerate() {
                    if matches!(primitive(name), "find" | "map" | "every" | "filter") && index == 1
                    {
                        let QueryExpr::Lambda { parameter, body } = arg else {
                            return Err(bad("find needs an expression callback".into()));
                        };
                        let mut locals = locals.clone();
                        locals.insert(parameter.clone());
                        expr(body, captures, &locals, span, depth + 1)?;
                    } else {
                        expr(arg, captures, locals, span, depth + 1)?;
                    }
                }
                Ok(())
            }
            QueryExpr::Lambda { .. } => Err(bad(
                "A callback is only valid as find's second argument".into(),
            )),
            QueryExpr::Tuple(values) | QueryExpr::List(values) => {
                for value in values {
                    expr(value, captures, locals, span, depth + 1)?;
                }
                Ok(())
            }
            QueryExpr::Not(value) => expr(value, captures, locals, span, depth + 1),
            _ => Ok(()),
        }
    }
    fn actions(
        values: &[QueryAction],
        captures: &BTreeSet<String>,
        depth: usize,
    ) -> Result<(), Diagnostic> {
        if depth > 128 {
            return Err(Diagnostic::error(
                "yl.artifact",
                "Query action nesting exceeds the resource limit",
                Span::default(),
            ));
        }
        for action in values {
            match action {
                QueryAction::Call { name, args, span } => {
                    if arity(name) != Some(args.len()) {
                        return Err(Diagnostic::error(
                            "yl.artifact",
                            "Invalid query primitive or argument count",
                            span.clone(),
                        ));
                    }
                    for arg in args {
                        expr(arg, captures, &BTreeSet::new(), span, 0)?;
                    }
                }
                QueryAction::If {
                    condition,
                    then_actions,
                    else_actions,
                    span,
                } => {
                    expr(condition, captures, &BTreeSet::new(), span, 0)?;
                    actions(then_actions, captures, depth + 1)?;
                    actions(else_actions, captures, depth + 1)?;
                }
                QueryAction::Match {
                    inputs,
                    cases,
                    span,
                } => {
                    for input in inputs {
                        expr(input, captures, &BTreeSet::new(), span, 0)?;
                    }
                    if inputs.is_empty()
                        || !cases.last().is_some_and(|case| {
                            case.alternatives
                                .iter()
                                .any(|alternative| alternative.iter().all(Option::is_none))
                        })
                    {
                        return Err(Diagnostic::error(
                            "yl.artifact",
                            "A query match needs a final wildcard case",
                            span.clone(),
                        ));
                    }
                    for (index, case) in cases.iter().enumerate() {
                        for alternative in &case.alternatives {
                            if alternative.len() != inputs.len()
                                || (index + 1 != cases.len()
                                    && alternative.iter().all(Option::is_none))
                            {
                                return Err(Diagnostic::error(
                                    "yl.artifact",
                                    "Invalid query match pattern arity or wildcard order",
                                    span.clone(),
                                ));
                            }
                            for pattern in alternative.iter().flatten() {
                                expr(pattern, captures, &BTreeSet::new(), span, 0)?;
                            }
                        }
                        actions(&case.actions, captures, depth + 1)?;
                    }
                }
                QueryAction::Emit {
                    message,
                    target,
                    related,
                    span,
                    ..
                } => {
                    for part in message {
                        if let QueryMessagePart::Value(value) = part {
                            expr(value, captures, &BTreeSet::new(), span, 0)?;
                        }
                    }
                    if let Some(target) = target {
                        expr(target, captures, &BTreeSet::new(), span, 0)?;
                    }
                    for target in related {
                        expr(target, captures, &BTreeSet::new(), span, 0)?;
                    }
                }
            }
        }
        Ok(())
    }
    for group in groups {
        actions(&group.actions, captures, 0)?;
    }
    Ok(())
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
pub enum QueryMessagePart {
    Text(String),
    Value(QueryExpr),
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
        message: Vec<QueryMessagePart>,
        span: Span,
        target: Option<QueryExpr>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        related: Vec<QueryExpr>,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct QueryCase {
    pub alternatives: Vec<Vec<Option<QueryExpr>>>,
    pub actions: Vec<QueryAction>,
    pub span: Span,
}
