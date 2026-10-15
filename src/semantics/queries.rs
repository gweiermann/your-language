//! Declarative contribution discovery and dependency evaluation over syntax identities.
use super::*;
use crate::image::query::{Action, ActionId, Case, Expr, ExprId, MessagePart, Opcode, QueryImage};
use std::collections::BTreeSet;
use std::{cell::RefCell, rc::Rc};

const MODULE: &str = "core/query";

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
    for name in ["error", "warn", "hint"] {
        registry.register(
            "core/diagnostics",
            name,
            OperationSignature {
                id: format!("core/diagnostics#{name}"),
                parameters: ["message", "target", "related"]
                    .into_iter()
                    .enumerate()
                    .map(|(index, name)| OperationParameter {
                        name: name.into(),
                        kind: ParameterKind::Value,
                        default: (index != 0).then_some(MeaningValue::Absent),
                    })
                    .collect(),
                provides_context: false,
                finalization_only: false,
                contribution: false,
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
    If(ExprId, bool),
    Case {
        inputs: Vec<ExprId>,
        cases: Vec<Case>,
        selected: usize,
    },
}

#[derive(Clone)]
enum Effect {
    Contribution(ExprId, ExprId),
    Contributions(ExprId, ExprId),
    Diagnostic {
        severity: crate::diagnostic::Severity,
        message: Vec<MessagePart>,
        target: Option<ExprId>,
        related: Vec<ExprId>,
    },
    Observe(ExprId),
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
    branch_values: Vec<(ExprId, SemanticValue)>,
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
    image: &'a QueryImage,
    producers: Vec<Producer<'a>>,
    discovered_blocks: BTreeSet<(usize, String)>,
    block_orders: BTreeMap<(usize, String), (usize, usize)>,
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
    name.strip_prefix("core/query#")
        .or_else(|| name.strip_prefix("std/query#"))
        .unwrap_or(name)
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
        image: &'a QueryImage,
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
            image,
            producers: vec![],
            discovered_blocks: BTreeSet::new(),
            block_orders: BTreeMap::new(),
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
            if let Some(groups) = image.groups.get(&owner.definition) {
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

    /// Discover invocation effects separately from evaluating their result. A binding
    /// may occur in many expressions, but its invocation contributes only once.
    fn discover_expression(
        &mut self,
        owner: &'a Occurrence,
        expression: &ExprId,
        guards: &[Guard],
        rank: usize,
    ) {
        match &self.image.expressions[*expression as usize] {
            Expr::Block {
                invocation,
                actions,
                result,
            } => {
                if self
                    .discovered_blocks
                    .insert((owner.id, invocation.clone()))
                {
                    self.discover(owner, actions, guards, rank);
                    self.discover_expression(owner, result, guards, rank);
                    self.block_orders
                        .insert((owner.id, invocation.clone()), (rank, self.producers.len()));
                }
            }
            Expr::Call {
                opcode: Opcode::Choose,
                args,
            } => {
                self.discover_expression(owner, &args[0], guards, rank);
                for (value, expected) in [(&args[1], true), (&args[2], false)] {
                    let mut nested = guards.to_vec();
                    nested.push(Guard::If(args[0], expected));
                    self.discover_expression(owner, value, &nested, rank);
                }
            }
            Expr::Call { args: values, .. } | Expr::Tuple(values) | Expr::List(values) => {
                for value in values {
                    self.discover_expression(owner, value, guards, rank);
                }
            }
            Expr::Not(value) => self.discover_expression(owner, value, guards, rank),
            // Effectful callback bodies are rejected during compilation and artifact validation.
            Expr::Lambda { .. } => {}
            _ => {}
        }
    }

    fn discover(
        &mut self,
        owner: &'a Occurrence,
        actions: &[ActionId],
        guards: &[Guard],
        rank: usize,
    ) {
        for action in actions {
            let action = &self.image.actions[*action as usize];
            match action {
                Action::Call { expression, .. } => {
                    self.discover_expression(owner, expression, guards, rank)
                }
                Action::If { condition, .. } => {
                    self.discover_expression(owner, condition, guards, rank)
                }
                Action::Match { inputs, cases, .. } => {
                    for input in inputs {
                        self.discover_expression(owner, input, guards, rank);
                    }
                    for case in cases {
                        for pattern in case.alternatives.iter().flatten().flatten() {
                            self.discover_expression(owner, pattern, guards, rank);
                        }
                    }
                }
                Action::Emit {
                    message,
                    target,
                    related,
                    ..
                } => {
                    for part in message {
                        if let MessagePart::Value(value) = part {
                            self.discover_expression(owner, value, guards, rank);
                        }
                    }
                    for value in target.iter().chain(related) {
                        self.discover_expression(owner, value, guards, rank);
                    }
                }
            }
            let (effect, span) = match action {
                Action::Call { expression, span } => {
                    let effect = match &self.image.expressions[*expression as usize] {
                        Expr::Call {
                            opcode: Opcode::Contribute,
                            args,
                        } => Effect::Contribution(args[0], args[1]),
                        Expr::Call {
                            opcode: Opcode::ContributeAll,
                            args,
                        } => Effect::Contributions(args[0], args[1]),
                        _ => Effect::Observe(*expression),
                    };
                    (effect, span)
                }
                Action::Emit {
                    severity,
                    message,
                    target,
                    related,
                    span,
                } => (
                    Effect::Diagnostic {
                        severity: severity.clone(),
                        message: message.clone(),
                        target: *target,
                        related: related.clone(),
                    },
                    span,
                ),
                Action::If {
                    condition,
                    then_actions,
                    else_actions,
                    ..
                } => {
                    let mut nested = guards.to_vec();
                    nested.push(Guard::If(*condition, true));
                    self.discover(owner, then_actions, &nested, rank);
                    *nested.last_mut().expect("new guard") = Guard::If(*condition, false);
                    self.discover(owner, else_actions, &nested, rank);
                    continue;
                }
                Action::Match { inputs, cases, .. } => {
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
        expression: &ExprId,
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
        expression: &ExprId,
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
        match &self.image.expressions[*expression as usize] {
            Expr::BindingValue { invocation, result } => {
                let order = self
                    .block_orders
                    .get(&(owner.id, invocation.clone()))
                    .copied()
                    .ok_or_else(|| {
                        Diagnostic::error(
                            "query.binding",
                            "A binding has no defining invocation",
                            owner.span.clone(),
                        )
                    })?;
                let previous = trace.order.replace(order);
                let value = self.eval(result, owner, locals, trace);
                trace.order = previous;
                value
            }
            Expr::Block { result, .. } => self.eval(result, owner, locals, trace),
            Expr::SelfValue => ready(SemanticValue::Identity(SemanticIdentity::Occurrence(
                owner.id,
            ))),
            Expr::Capture(name) => ready(self.capture_value(owner.capture_path(name)?)),
            Expr::Local(name) => locals
                .get(name)
                .cloned()
                .map(Outcome::Ready)
                .ok_or_else(|| {
                    Diagnostic::error(
                        "query.local",
                        format!("Unbound callback parameter {name}"),
                        owner.span.clone(),
                    )
                }),
            Expr::Absent => ready(SemanticValue::Absent),
            Expr::Bool(value) => ready(SemanticValue::Bool(*value)),
            Expr::Text(value) => ready(SemanticValue::Text(value.clone())),
            Expr::Symbol(value) => ready(SemanticValue::Symbol(value.clone())),
            Expr::Tuple(items) | Expr::List(items) => {
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
                ready(
                    if matches!(
                        &self.image.expressions[*expression as usize],
                        Expr::Tuple(_)
                    ) {
                        SemanticValue::Tuple(values)
                    } else {
                        SemanticValue::List(values)
                    },
                )
            }
            Expr::Not(value) => match self.eval(value, owner, locals, trace)? {
                Outcome::Ready(SemanticValue::Bool(value)) => ready(SemanticValue::Bool(!value)),
                Outcome::Ready(SemanticValue::Absent) => ready(SemanticValue::Absent),
                Outcome::Ready(_) => Err(Diagnostic::error(
                    "query.condition",
                    "Condition must be boolean",
                    owner.span.clone(),
                )),
                value => Ok(value),
            },
            Expr::Lambda { .. } => Err(Diagnostic::error(
                "query.callback",
                "A callback is only valid as find's second argument",
                owner.span.clone(),
            )),
            Expr::Call { opcode, args } => self.call(*opcode, args, owner, locals, trace),
        }
    }

    // Address evaluation can rule out a path even when another component is
    // pending. Ordinary data tuples still preserve absent fields and shapes.
    fn eval_key(
        &self,
        expression: &ExprId,
        owner: &Occurrence,
        locals: &Locals,
        trace: &mut Trace,
    ) -> Result<Outcome, Diagnostic> {
        let Expr::Tuple(parts) = &self.image.expressions[*expression as usize] else {
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
        name: Opcode,
        args: &[ExprId],
        owner: &Occurrence,
        locals: &Locals,
        trace: &mut Trace,
    ) -> Result<Outcome, Diagnostic> {
        let bad = || {
            Diagnostic::error(
                "query.argument",
                format!("Invalid arguments for {}", name.name()),
                owner.span.clone(),
            )
        };
        if matches!(
            name,
            Opcode::Get | Opcode::GetEarlier | Opcode::GetAll | Opcode::Facts
        ) {
            return match self.eval_key(&args[0], owner, locals, trace)? {
                Outcome::Ready(value) => match address(&value, &owner.span)? {
                    Some(key) => Ok(self.lookup_visible(
                        &key,
                        name == Opcode::GetAll,
                        name == Opcode::Facts,
                        name == Opcode::GetEarlier,
                        trace,
                    )),
                    None => Ok(Outcome::Ready(SemanticValue::Absent)),
                },
                outcome => Ok(outcome),
            };
        }
        if name == Opcode::Choose {
            return match self.eval(&args[0], owner, locals, trace)? {
                Outcome::Ready(SemanticValue::Bool(condition)) => {
                    self.eval(&args[if condition { 1 } else { 2 }], owner, locals, trace)
                }
                Outcome::Ready(SemanticValue::Absent) => Ok(Outcome::Ready(SemanticValue::Absent)),
                Outcome::Ready(_) => Err(bad()),
                value => Ok(value),
            };
        }
        if matches!(
            name,
            Opcode::Find | Opcode::Map | Opcode::Every | Opcode::Filter
        ) {
            let values = match self.eval(&args[0], owner, locals, trace)? {
                Outcome::Ready(SemanticValue::List(values)) => values,
                Outcome::Ready(SemanticValue::Absent) => {
                    return Ok(Outcome::Ready(SemanticValue::Absent))
                }
                Outcome::Ready(_) => return Err(bad()),
                pending => return Ok(pending),
            };
            let Expr::Lambda { parameter, body } = &self.image.expressions[args[1] as usize] else {
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
                    (Opcode::Find, Outcome::Ready(SemanticValue::Absent)) => {}
                    (Opcode::Find, result) => return Ok(result),
                    (Opcode::Map, Outcome::Ready(value)) => {
                        add_value_cost(&mut cost, &value, &owner.span)?;
                        mapped.push(value)
                    }
                    (Opcode::Filter, Outcome::Ready(SemanticValue::Bool(true))) => {
                        mapped.push(value)
                    }
                    (Opcode::Filter, Outcome::Ready(SemanticValue::Bool(false))) => {}
                    (Opcode::Filter, Outcome::Ready(SemanticValue::Absent)) => {}
                    (Opcode::Every, Outcome::Ready(SemanticValue::Absent)) => {
                        return Ok(Outcome::Ready(SemanticValue::Absent))
                    }
                    (Opcode::Every, Outcome::Ready(SemanticValue::Bool(false))) => {
                        return Ok(Outcome::Ready(SemanticValue::Bool(false)))
                    }
                    (Opcode::Every, Outcome::Ready(SemanticValue::Bool(true))) => {}
                    (_, Outcome::Pending) => pending = true,
                    (_, Outcome::Conflict(ids)) => return Ok(Outcome::Conflict(ids)),
                    _ => return Err(bad()),
                }
            }
            return Ok(if pending {
                Outcome::Pending
            } else {
                Outcome::Ready(match name {
                    Opcode::Map | Opcode::Filter => SemanticValue::List(mapped),
                    Opcode::Every => SemanticValue::Bool(true),
                    _ => SemanticValue::Absent,
                })
            });
        }
        // Presence inspectors and explicit value materialization observe absence.
        // Other pure primitives end only this dependent expression, not its caller.
        let strict = !matches!(
            name,
            Opcode::IsPresent | Opcode::Equal | Opcode::IsList | Opcode::Values
        );
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
            (Opcode::Ancestors, [value]) => {
                SemanticValue::List(self.ancestors(value, &owner.span)?)
            }
            (Opcode::Children, [value]) => {
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
            (Opcode::Text, [SemanticValue::Source(source)]) => {
                SemanticValue::Text(source.text.clone())
            }
            (Opcode::Text, [SemanticValue::Text(text)]) => SemanticValue::Text(text.clone()),
            (Opcode::Text, [SemanticValue::Identity(SemanticIdentity::Occurrence(id))]) => {
                let owner = self.occurrences.get(id).ok_or_else(bad)?;
                SemanticValue::Text(
                    self.source
                        .get(owner.span.start..owner.span.end)
                        .unwrap_or("")
                        .into(),
                )
            }
            (Opcode::IsPresent, [value]) => {
                SemanticValue::Bool(!matches!(value, SemanticValue::Absent))
            }
            (Opcode::Equal, [left, right]) => SemanticValue::Bool(equal_values(left, right)),
            (Opcode::HasMultiple, [SemanticValue::List(values)]) => {
                SemanticValue::Bool(values.len() > 1)
            }
            (Opcode::Values, [SemanticValue::Alternatives { values, .. }]) => {
                SemanticValue::List(values.clone())
            }
            (Opcode::Values, [SemanticValue::Absent]) => SemanticValue::List(vec![]),
            (Opcode::Values, [value]) => SemanticValue::List(vec![value.clone()]),
            (Opcode::Alternatives, [SemanticValue::List(values)]) => {
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
            (Opcode::Flatten, [SemanticValue::List(values)]) => {
                let mut items = vec![];
                for value in values {
                    match value {
                        SemanticValue::List(values) => items.extend(values.clone()),
                        _ => return Err(bad()),
                    }
                }
                SemanticValue::List(items)
            }
            (Opcode::IsList, [value]) => {
                SemanticValue::Bool(matches!(value, SemanticValue::List(_)))
            }
            (Opcode::Tail, [SemanticValue::List(values) | SemanticValue::Tuple(values)]) => {
                SemanticValue::List(values.iter().skip(1).cloned().collect())
            }
            (Opcode::Last, [SemanticValue::List(values) | SemanticValue::Tuple(values)]) => {
                values.last().cloned().unwrap_or(SemanticValue::Absent)
            }
            (Opcode::First, [SemanticValue::List(values) | SemanticValue::Tuple(values)]) => {
                values.first().cloned().unwrap_or(SemanticValue::Absent)
            }
            (Opcode::Concat, [SemanticValue::List(values)]) => {
                let texts = values
                    .iter()
                    .map(|value| match value {
                        SemanticValue::Text(text) => Ok(text.as_str()),
                        _ => Err(bad()),
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                SemanticValue::Text(texts.concat())
            }
            (Opcode::Contains, [SemanticValue::List(values), value]) => {
                SemanticValue::Bool(values.iter().any(|item| equal_values(item, value)))
            }
            (Opcode::Unique, [SemanticValue::List(values)]) => {
                let mut unique = vec![];
                for value in values {
                    if !unique.iter().any(|item| equal_values(item, value)) {
                        unique.push(value.clone());
                    }
                }
                SemanticValue::List(unique)
            }
            (Opcode::IsSubset, [SemanticValue::List(left), SemanticValue::List(right)]) => {
                SemanticValue::Bool(
                    left.iter()
                        .all(|value| right.iter().any(|item| equal_values(value, item))),
                )
            }
            (Opcode::Lookup, [SemanticValue::List(fields), key]) => {
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
            (Opcode::Keys, [SemanticValue::List(fields)]) => {
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
            (Opcode::Zip, [SemanticValue::List(left), SemanticValue::List(right)]) => {
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
                Opcode::Hypothetical | Opcode::HypotheticalAll | Opcode::HypotheticalChecked,
                [key, SemanticValue::List(assumptions)],
            ) => {
                return self.hypothetical(
                    key,
                    assumptions,
                    trace,
                    &owner.span,
                    name != Opcode::Hypothetical,
                    name == Opcode::HypotheticalChecked,
                );
            }
            _ => return Err(bad()),
        };
        Ok(Outcome::Ready(result))
    }

    fn abstract_expr(
        &self,
        expr: &ExprId,
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
        match &self.image.expressions[*expr as usize] {
            Expr::Block { result, .. } | Expr::BindingValue { result, .. } => {
                self.abstract_expr(result, owner, locals, depth + 1)
            }
            Expr::Tuple(values) => Pattern::Tuple(
                values
                    .iter()
                    .map(|value| self.abstract_expr(value, owner, locals, depth + 1))
                    .collect(),
            ),
            Expr::Call {
                opcode: Opcode::Get,
                args,
            } => {
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
            Expr::Call {
                opcode: Opcode::Find,
                args,
            } => {
                let Ok(Outcome::Ready(SemanticValue::List(values))) =
                    self.eval(&args[0], owner, locals, &mut Trace::default())
                else {
                    return Pattern::Any;
                };
                let Expr::Lambda { parameter, body } = &self.image.expressions[args[1] as usize]
                else {
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
                            context.branch_values.push((*input, value.clone()));
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
                        MessagePart::Text(value) => value.clone(),
                        MessagePart::Value(value) => {
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
            image: self.image,
            producers: self.producers.clone(),
            discovered_blocks: self.discovered_blocks.clone(),
            block_orders: self.block_orders.clone(),
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
    let fallback;
    let image = if let Some(image) = &language.image {
        &image.queries
    } else {
        fallback = match QueryImage::compile(language) {
            Ok(image) => image,
            Err(error) => {
                diagnostics.push(error);
                return;
            }
        };
        &fallback
    };
    if image.groups.is_empty() {
        return;
    }
    let ranks = match super::scheduler::query_group_ranks(language, occurrences) {
        Ok(ranks) => ranks,
        Err(errors) => {
            diagnostics.extend(errors);
            return;
        }
    };
    let mut engine = Engine::new(language, image, occurrences, source, &ranks);
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

/// Count normalized operation payloads before the compiler clones them. The
/// explicit stack includes nested action expressions and invocation blocks.
pub(crate) fn action_node_count(actions: &[QueryAction], limit: usize) -> Option<usize> {
    enum Node<'a> {
        Action(&'a QueryAction),
        Expression(&'a QueryExpr),
        Message(&'a QueryMessagePart),
    }
    let mut pending: Vec<_> = actions.iter().map(Node::Action).collect();
    let mut count = 0usize;
    while let Some(node) = pending.pop() {
        count = count.checked_add(1)?;
        if count > limit {
            return None;
        }
        match node {
            Node::Action(action) => match action {
                QueryAction::Call { args, .. } => pending.extend(args.iter().map(Node::Expression)),
                QueryAction::If {
                    condition,
                    then_actions,
                    else_actions,
                    ..
                } => {
                    pending.push(Node::Expression(condition));
                    pending.extend(then_actions.iter().chain(else_actions).map(Node::Action));
                }
                QueryAction::Match { inputs, cases, .. } => {
                    pending.extend(inputs.iter().map(Node::Expression));
                    for case in cases {
                        pending.extend(
                            case.alternatives
                                .iter()
                                .flatten()
                                .flatten()
                                .map(Node::Expression),
                        );
                        pending.extend(case.actions.iter().map(Node::Action));
                    }
                }
                QueryAction::Emit {
                    message,
                    target,
                    related,
                    ..
                } => {
                    pending.extend(message.iter().map(Node::Message));
                    pending.extend(target.iter().chain(related).map(Node::Expression));
                }
            },
            Node::Expression(expression) => match expression {
                QueryExpr::Block {
                    actions, result, ..
                } => {
                    pending.extend(actions.iter().map(Node::Action));
                    pending.push(Node::Expression(result));
                }
                QueryExpr::Call { args, .. } | QueryExpr::Tuple(args) | QueryExpr::List(args) => {
                    pending.extend(args.iter().map(Node::Expression));
                }
                QueryExpr::Not(value)
                | QueryExpr::Lambda { body: value, .. }
                | QueryExpr::BindingValue { result: value, .. } => {
                    pending.push(Node::Expression(value))
                }
                _ => {}
            },
            Node::Message(QueryMessagePart::Value(value)) => pending.push(Node::Expression(value)),
            Node::Message(QueryMessagePart::Text(_)) => {}
        }
    }
    Some(count)
}

pub(crate) fn validate_groups(
    groups: &[QueryGroup],
    captures: &BTreeSet<String>,
    version: u32,
) -> Result<(), Diagnostic> {
    struct Validator<'a> {
        captures: &'a BTreeSet<String>,
        version: u32,
        blocks: BTreeMap<String, (Vec<QueryAction>, QueryExpr)>,
        bindings: BTreeMap<String, Span>,
    }
    fn arity(name: &str) -> Option<usize> {
        if !name.starts_with("std/query#") && !name.starts_with("core/query#") {
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
    impl Validator<'_> {
        fn bad(span: &Span, message: impl Into<String>) -> Diagnostic {
            Diagnostic::error("yl.artifact", message, span.clone())
        }
        fn primitive(&self, name: &str, count: usize, span: &Span) -> Result<(), Diagnostic> {
            if name.starts_with("core/query#") && self.version < 5 {
                return Err(Self::bad(
                    span,
                    "Core query primitives require artifact version 5",
                ));
            }
            if self.version < 4
                && matches!(
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
            {
                return Err(Self::bad(
                    span,
                    "These query primitives require artifact version 4",
                ));
            }
            if arity(name) != Some(count) {
                return Err(Self::bad(span, "Invalid query primitive or argument count"));
            }
            Ok(())
        }
        fn expr(
            &mut self,
            value: &QueryExpr,
            locals: &BTreeSet<String>,
            span: &Span,
            depth: usize,
        ) -> Result<(), Diagnostic> {
            if depth > 128 {
                return Err(Self::bad(span, "Query nesting exceeds the resource limit"));
            }
            match value {
                QueryExpr::BindingValue { invocation, result } => {
                    if self.version < 5 || invocation.is_empty() {
                        return Err(Self::bad(
                            span,
                            "A binding value requires a version 5 invocation identity",
                        ));
                    }
                    self.bindings.insert(invocation.clone(), span.clone());
                    self.expr(result, locals, span, depth + 1)
                }
                QueryExpr::Block {
                    invocation,
                    actions,
                    result,
                } => {
                    if self.version < 5 {
                        return Err(Self::bad(
                            span,
                            "Operation result blocks require artifact version 5",
                        ));
                    }
                    if invocation.is_empty() {
                        return Err(Self::bad(
                            span,
                            "An operation result needs an invocation identity",
                        ));
                    }
                    if !locals.is_empty() {
                        return Err(Self::bad(
                            span,
                            "Effectful operations are not supported inside query callbacks",
                        ));
                    }
                    if let Some((previous_actions, previous_result)) = self.blocks.get(invocation) {
                        if previous_actions != actions || previous_result != result.as_ref() {
                            return Err(Self::bad(
                                span,
                                "An invocation identity has conflicting definitions",
                            ));
                        }
                    } else {
                        self.blocks.insert(
                            invocation.clone(),
                            (actions.clone(), result.as_ref().clone()),
                        );
                    }
                    self.actions(actions, depth + 1)?;
                    self.expr(result, locals, span, depth + 1)
                }
                QueryExpr::Capture(name)
                    if !self
                        .captures
                        .contains(name.split('.').next().unwrap_or(name)) =>
                {
                    Err(Self::bad(
                        span,
                        format!("Query references unavailable capture {name}"),
                    ))
                }
                QueryExpr::Local(name) if !locals.contains(name) => Err(Self::bad(
                    span,
                    format!("Unbound query callback parameter {name}"),
                )),
                QueryExpr::Call { name, args } => {
                    self.primitive(name, args.len(), span)?;
                    if matches!(primitive(name), "contribute" | "contributeAll") {
                        return Err(Self::bad(span, "Contributions must be query actions"));
                    }
                    for (index, arg) in args.iter().enumerate() {
                        if matches!(primitive(name), "find" | "map" | "every" | "filter")
                            && index == 1
                        {
                            let QueryExpr::Lambda { parameter, body } = arg else {
                                return Err(Self::bad(
                                    span,
                                    "Query iteration needs an expression callback",
                                ));
                            };
                            let mut locals = locals.clone();
                            locals.insert(parameter.clone());
                            self.expr(body, &locals, span, depth + 1)?;
                        } else {
                            self.expr(arg, locals, span, depth + 1)?;
                        }
                    }
                    Ok(())
                }
                QueryExpr::Lambda { .. } => Err(Self::bad(
                    span,
                    "A callback is only valid as an iteration argument",
                )),
                QueryExpr::Tuple(values) | QueryExpr::List(values) => {
                    for value in values {
                        self.expr(value, locals, span, depth + 1)?;
                    }
                    Ok(())
                }
                QueryExpr::Not(value) => self.expr(value, locals, span, depth + 1),
                _ => Ok(()),
            }
        }
        fn actions(&mut self, values: &[QueryAction], depth: usize) -> Result<(), Diagnostic> {
            if depth > 128 {
                return Err(Self::bad(
                    &Span::default(),
                    "Query action nesting exceeds the resource limit",
                ));
            }
            let locals = BTreeSet::new();
            for action in values {
                match action {
                    QueryAction::Call { name, args, span } => {
                        self.primitive(name, args.len(), span)?;
                        for arg in args {
                            self.expr(arg, &locals, span, depth + 1)?;
                        }
                    }
                    QueryAction::If {
                        condition,
                        then_actions,
                        else_actions,
                        span,
                    } => {
                        self.expr(condition, &locals, span, depth + 1)?;
                        self.actions(then_actions, depth + 1)?;
                        self.actions(else_actions, depth + 1)?;
                    }
                    QueryAction::Match {
                        inputs,
                        cases,
                        span,
                    } => {
                        for input in inputs {
                            self.expr(input, &locals, span, depth + 1)?;
                        }
                        if inputs.is_empty()
                            || !cases.last().is_some_and(|case| {
                                case.alternatives
                                    .iter()
                                    .any(|alternative| alternative.iter().all(Option::is_none))
                            })
                        {
                            return Err(Self::bad(
                                span,
                                "A query match needs a final wildcard case",
                            ));
                        }
                        for (index, case) in cases.iter().enumerate() {
                            for alternative in &case.alternatives {
                                if alternative.len() != inputs.len()
                                    || (index + 1 != cases.len()
                                        && alternative.iter().all(Option::is_none))
                                {
                                    return Err(Self::bad(
                                        span,
                                        "Invalid query match pattern arity or wildcard order",
                                    ));
                                }
                                for pattern in alternative.iter().flatten() {
                                    self.expr(pattern, &locals, span, depth + 1)?;
                                }
                            }
                            self.actions(&case.actions, depth + 1)?;
                        }
                    }
                    QueryAction::Emit {
                        message,
                        target,
                        related,
                        span,
                        ..
                    } => {
                        if self.version < 4 && !related.is_empty() {
                            return Err(Self::bad(
                                span,
                                "Related diagnostic sources require artifact version 4",
                            ));
                        }
                        for part in message {
                            if let QueryMessagePart::Value(value) = part {
                                self.expr(value, &locals, span, depth + 1)?;
                            }
                        }
                        for value in target.iter().chain(related) {
                            self.expr(value, &locals, span, depth + 1)?;
                        }
                    }
                }
            }
            Ok(())
        }
    }
    let mut validator = Validator {
        captures,
        version,
        blocks: BTreeMap::new(),
        bindings: BTreeMap::new(),
    };
    for group in groups {
        validator.actions(&group.actions, 0)?;
    }
    for (invocation, span) in &validator.bindings {
        if !validator.blocks.contains_key(invocation) {
            return Err(Validator::bad(
                span,
                "A binding value has no defining invocation",
            ));
        }
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
    BindingValue {
        invocation: String,
        result: Box<QueryExpr>,
    },
    Block {
        invocation: String,
        actions: Vec<QueryAction>,
        result: Box<QueryExpr>,
    },
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

#[cfg(test)]
mod validation_tests {
    use super::*;

    #[test]
    fn binding_reads_require_a_declared_invocation_context() {
        let read = QueryExpr::BindingValue {
            invocation: "binding".into(),
            result: Box::new(QueryExpr::Bool(true)),
        };
        assert!(validate(read.clone(), 5).is_err());
        assert!(validate(
            QueryExpr::Tuple(vec![read, block("binding", QueryExpr::Bool(true))]),
            5
        )
        .is_ok());
    }

    #[test]
    fn copy_budget_counts_payloads_inside_nested_action_blocks() {
        let actions = vec![QueryAction::Call {
            name: "core/query#contribute".into(),
            args: vec![
                QueryExpr::Text("key".into()),
                QueryExpr::Block {
                    invocation: "nested".into(),
                    actions: vec![QueryAction::Call {
                        name: "core/query#contribute".into(),
                        args: vec![QueryExpr::List(vec![QueryExpr::Bool(true); 32])],
                        span: Span::default(),
                    }],
                    result: Box::new(QueryExpr::Absent),
                },
            ],
            span: Span::default(),
        }];
        let count = action_node_count(&actions, usize::MAX).unwrap();
        assert_eq!(count, 38);
        assert_eq!(action_node_count(&actions, count), Some(count));
        assert_eq!(action_node_count(&actions, count - 1), None);
    }

    fn block(invocation: &str, result: QueryExpr) -> QueryExpr {
        QueryExpr::Block {
            invocation: invocation.into(),
            actions: vec![],
            result: Box::new(result),
        }
    }

    fn validate(expression: QueryExpr, version: u32) -> Result<(), Diagnostic> {
        validate_groups(
            &[QueryGroup {
                name: None,
                actions: vec![QueryAction::Call {
                    name: if version < 5 {
                        "std/query#first"
                    } else {
                        "core/query#first"
                    }
                    .into(),
                    args: vec![QueryExpr::List(vec![expression])],
                    span: Span::default(),
                }],
                span: Span::default(),
            }],
            &BTreeSet::new(),
            version,
        )
    }

    #[test]
    fn result_blocks_require_version_five() {
        assert!(validate(block("call", QueryExpr::Absent), 4).is_err());
        assert!(validate(block("call", QueryExpr::Absent), 5).is_ok());
    }

    #[test]
    fn repeated_bindings_must_describe_the_same_invocation() {
        let first = block("call", QueryExpr::Bool(true));
        assert!(validate(QueryExpr::Tuple(vec![first.clone(), first.clone()]), 5).is_ok());
        assert!(validate(
            QueryExpr::Tuple(vec![first, block("call", QueryExpr::Bool(false))]),
            5
        )
        .is_err());
    }

    #[test]
    fn result_block_actions_are_validated() {
        let expression = QueryExpr::Block {
            invocation: "call".into(),
            actions: vec![QueryAction::Call {
                name: "core/query#contribute".into(),
                args: vec![
                    QueryExpr::Capture("unavailable".into()),
                    QueryExpr::Bool(true),
                ],
                span: Span::default(),
            }],
            result: Box::new(QueryExpr::Absent),
        };
        assert!(validate(expression, 5).is_err());
    }

    #[test]
    fn callbacks_cannot_hide_operation_effects() {
        let expression = QueryExpr::Call {
            name: "core/query#map".into(),
            args: vec![
                QueryExpr::List(vec![]),
                QueryExpr::Lambda {
                    parameter: "item".into(),
                    body: Box::new(block("call", QueryExpr::Local("item".into()))),
                },
            ],
        };
        assert!(validate(expression, 5).is_err());
    }

    #[test]
    fn nested_blocks_cannot_reset_the_depth_budget() {
        let expression = (0..130).fold(QueryExpr::Absent, |result, index| {
            block(&index.to_string(), result)
        });
        assert!(validate(expression, 5).is_err());
    }
}
