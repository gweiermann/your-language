//! Experimental native semantic operations over successful syntax occurrences.
use crate::{
    diagnostic::{Diagnostic, Span},
    AstNode, AstValue, CompiledLanguage,
};
use serde::{Deserialize, Serialize};
use std::{any::Any, collections::BTreeMap, sync::Arc};
pub mod lexical;
mod scheduler;
pub mod syntax;
pub mod types;
use scheduler::execute;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MeaningDefinition {
    /// Marks a syntax occurrence as the edge of its owner's selection region.
    #[serde(default)]
    pub boundary: bool,
    pub groups: Vec<MeaningGroup>,
    pub precedence: Vec<MeaningChain>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MeaningGroup {
    pub name: Option<String>,
    pub calls: Vec<MeaningCall>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matching: Option<MeaningMatch>,
    pub span: Span,
}
impl MeaningGroup {
    pub(crate) fn all_calls(&self) -> Vec<&MeaningCall> {
        fn add<'a>(call: &'a MeaningCall, calls: &mut Vec<&'a MeaningCall>) {
            calls.push(call);
            for value in call.arguments.values() {
                add_value(value, calls);
            }
        }
        fn add_value<'a>(value: &'a MeaningValue, calls: &mut Vec<&'a MeaningCall>) {
            match value {
                MeaningValue::Call(call) => add(call, calls),
                MeaningValue::Tuple(values) | MeaningValue::List(values) => {
                    for value in values {
                        add_value(value, calls);
                    }
                }
                _ => {}
            }
        }
        let mut calls = vec![];
        for call in &self.calls {
            add(call, &mut calls);
        }
        if let Some(matching) = &self.matching {
            for value in &matching.inputs {
                add_value(value, &mut calls);
            }
            for case in &matching.cases {
                for action in &case.actions {
                    if let MeaningAction::Call(call) = action {
                        add(call, &mut calls);
                    }
                }
            }
        }
        calls
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MeaningCall {
    pub operation: String,
    pub contract: OperationSignature,
    pub arguments: BTreeMap<String, MeaningValue>,
    pub span: Span,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum MeaningValue {
    /// Current caller occurrence, independent of imported libraries.
    SelfValue,
    Absent,
    Tuple(Vec<MeaningValue>),
    List(Vec<MeaningValue>),
    Call(Box<MeaningCall>),
    Capture(String),
    Text(String),
    Bool(bool),
    Symbol(String),
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MeaningMatch {
    pub inputs: Vec<MeaningValue>,
    pub cases: Vec<MeaningCase>,
    pub span: Span,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MeaningCase {
    pub alternatives: Vec<Vec<Option<MeaningValue>>>,
    pub actions: Vec<MeaningAction>,
    pub span: Span,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum MeaningAction {
    Call(MeaningCall),
    Emit {
        severity: crate::diagnostic::Severity,
        message: Vec<MeaningMessagePart>,
        span: Span,
    },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum MeaningMessagePart {
    Text(String),
    Capture(String),
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MeaningChain {
    pub selectors: Vec<MeaningSelector>,
    pub span: Span,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MeaningSelector {
    pub definition: String,
    pub group: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
/// Preserved semantic value and its original source extent. Pipe wrappers do not
/// widen this extent; syntax-local captures separately retain grammar match spans.
pub struct SourceCapture {
    /// Nested occurrence selection for projections through repeated parents.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection: Option<OccurrenceSelection>,
    /// Repetition of syntax occurrences, independent of a pattern's value shape.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub repeated: bool,
    pub value: AstValue,
    pub span: Span,
    pub text: String,
    /// Original retained occurrence roots selected by this capture, in order.
    /// Empty for terminals and absent optional captures; survives projections.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub occurrences: Vec<usize>,
}
/// Shape of an occurrence-valued capture projection, independent of its AST.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum OccurrenceSelection {
    One(usize),
    List(Vec<OccurrenceSelection>),
    Absent,
}
impl SourceCapture {
    pub fn occurrence_selection(&self) -> Option<OccurrenceSelection> {
        if let Some(selection) = &self.selection {
            return Some(selection.clone());
        }
        if self.value == AstValue::None {
            Some(OccurrenceSelection::Absent)
        } else if self.repeated {
            Some(OccurrenceSelection::List(
                self.occurrences
                    .iter()
                    .map(|id| OccurrenceSelection::One(*id))
                    .collect(),
            ))
        } else if self.occurrences.len() == 1 {
            Some(OccurrenceSelection::One(self.occurrences[0]))
        } else {
            None
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Occurrence {
    pub id: usize,
    pub definition: String,
    pub span: Span,
    pub captures: BTreeMap<String, SourceCapture>,
    pub children: Vec<Occurrence>,
}
impl Occurrence {
    fn find(&self, id: usize) -> Option<&Occurrence> {
        if self.id == id {
            Some(self)
        } else {
            self.children.iter().find_map(|child| child.find(id))
        }
    }
    fn project_selection(
        &self,
        selection: &OccurrenceSelection,
        field: &str,
        span: &Span,
    ) -> Result<(AstValue, Option<OccurrenceSelection>), Diagnostic> {
        match selection {
            OccurrenceSelection::One(id) => {
                let capture = self
                    .find(*id)
                    .and_then(|owner| owner.captures.get(field))
                    .ok_or_else(|| {
                        Diagnostic::error(
                            "semantic.missing_capture",
                            format!("Capture projection .{field} is unavailable"),
                            span.clone(),
                        )
                    })?;
                Ok((capture.value.clone(), capture.occurrence_selection()))
            }
            OccurrenceSelection::List(items) => {
                let projected = items
                    .iter()
                    .map(|item| self.project_selection(item, field, span))
                    .collect::<Result<Vec<_>, _>>()?;
                let selection = projected
                    .iter()
                    .map(|(_, selection)| selection.clone())
                    .collect::<Option<Vec<_>>>()
                    .map(OccurrenceSelection::List);
                Ok((
                    AstValue::List(projected.into_iter().map(|(value, _)| value).collect()),
                    selection,
                ))
            }
            OccurrenceSelection::Absent => Ok((AstValue::None, Some(OccurrenceSelection::Absent))),
        }
    }
    /// Follow captured occurrence roots rather than reconstructed AST wrappers.
    fn capture_path(&self, path: &str) -> Result<SourceCapture, Diagnostic> {
        let mut parts = path.split('.');
        let root = parts.next().unwrap_or(path);
        let mut capture = self.captures.get(root).cloned().ok_or_else(|| {
            Diagnostic::error(
                "semantic.missing_capture",
                format!("Capture {root} is unavailable"),
                self.span.clone(),
            )
        })?;
        for field in parts {
            if capture.value == AstValue::None {
                return Ok(capture);
            }
            let repeated = capture.repeated;
            let selected = capture
                .occurrences
                .iter()
                .map(|id| {
                    self.find(*id)
                        .and_then(|owner| owner.captures.get(field))
                        .cloned()
                        .ok_or_else(|| {
                            Diagnostic::error(
                                "semantic.missing_capture",
                                format!("Capture projection {path} is unavailable"),
                                capture.span.clone(),
                            )
                        })
                })
                .collect::<Result<Vec<_>, _>>()?;
            if !repeated && selected.len() == 1 {
                capture = selected[0].clone();
                continue;
            }
            let selection = capture.occurrence_selection().ok_or_else(|| {
                Diagnostic::error(
                    "semantic.missing_capture",
                    format!("Capture projection {path} has no occurrence to select"),
                    capture.span.clone(),
                )
            })?;
            let (value, selection) = self.project_selection(&selection, field, &capture.span)?;
            let span = selected
                .first()
                .zip(selected.last())
                .map(|(a, b)| Span::new(&a.span.file, a.span.start, b.span.end))
                .unwrap_or_else(|| capture.span.clone());
            capture = SourceCapture {
                selection,
                repeated: true,
                value,
                span,
                text: selected
                    .iter()
                    .map(|item| item.text.as_str())
                    .collect::<Vec<_>>()
                    .join(" "),
                occurrences: selected
                    .iter()
                    .flat_map(|item| item.occurrences.clone())
                    .collect(),
            };
        }
        Ok(capture)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum SemanticValue {
    /// An observational query's growing set of possible values. Unlike List,
    /// this opts a match into Cartesian, dependency-driven distribution.
    Alternatives {
        service: String,
        values: Vec<SemanticValue>,
    },
    Identity(SemanticIdentity),
    Tuple(Vec<SemanticValue>),
    List(Vec<SemanticValue>),
    Deferred {
        service: String,
        key: Box<SemanticValue>,
        span: Span,
    },
    Absent,
    Text(String),
    Bool(bool),
    Symbol(String),
    Record(usize),
    Source(SourceCapture),
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum SemanticIdentity {
    Occurrence(usize),
    Binding(usize),
    /// Opaque analysis-local handle allocated by a native library.
    Service {
        service: String,
        id: usize,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum SemanticKey {
    Text(String),
    Symbol(String),
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SemanticRecord {
    pub id: usize,
    pub entries: Vec<(SemanticKey, SemanticValue)>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Attachment {
    pub occurrence: usize,
    pub key: String,
    pub value: SemanticValue,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SemanticGraph {
    pub records: Vec<SemanticRecord>,
    pub attachments: Vec<Attachment>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AnalysisResult {
    pub ast: Option<AstNode>,
    pub occurrences: Vec<Occurrence>,
    pub graph: SemanticGraph,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ParameterKind {
    Value,
    Capture,
    Text,
    Bool,
    Symbol,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OperationParameter {
    pub name: String,
    pub kind: ParameterKind,
    pub default: Option<MeaningValue>,
}
fn is_false(value: &bool) -> bool {
    !value
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OperationSignature {
    pub id: String,
    pub parameters: Vec<OperationParameter>,
    /// Context providers enclose the owning occurrence's semantic descendants.
    pub provides_context: bool,
    /// A collector whose effects belong to finalization, never live traversal.
    /// Relative collector ordering is retained, while lifecycle hooks run first.
    #[serde(default, skip_serializing_if = "is_false")]
    pub finalization_only: bool,
    /// Idempotent fact registration, safe once per distributed match tuple.
    /// This capability is part of the compiled native operation contract.
    #[serde(default, skip_serializing_if = "is_false")]
    pub contribution: bool,
}
type Factory = Arc<dyn Fn() -> Box<dyn NativeOperation> + Send + Sync>;
type ServiceFactory = Arc<dyn Fn() -> Box<dyn AnalysisService> + Send + Sync>;
#[derive(Clone, Default)]
pub struct OperationRegistry {
    pub(crate) operations: BTreeMap<String, (OperationSignature, Factory)>,
    pub(crate) exports: BTreeMap<(String, String), String>,
    pub(crate) symbols: BTreeMap<(String, String), String>,
    pub(crate) services: BTreeMap<String, ServiceFactory>,
}
impl OperationRegistry {
    /// Register fresh per-analysis state with automatic fixed-point completion.
    pub fn register_service<F>(&mut self, id: &str, factory: F) -> Result<(), String>
    where
        F: Fn() -> Box<dyn AnalysisService> + Send + Sync + 'static,
    {
        if self.services.contains_key(id) {
            return Err(format!("Duplicate analysis service {id}"));
        }
        self.services.insert(id.into(), Arc::new(factory));
        Ok(())
    }
    pub fn new() -> Self {
        Self::default()
    }
    pub fn register<F>(
        &mut self,
        module: &str,
        name: &str,
        signature: OperationSignature,
        factory: F,
    ) -> Result<(), String>
    where
        F: Fn() -> Box<dyn NativeOperation> + Send + Sync + 'static,
    {
        if self.exports.contains_key(&(module.into(), name.into()))
            || self.symbols.contains_key(&(module.into(), name.into()))
            || self.operations.contains_key(&signature.id)
        {
            return Err(format!("Duplicate operation {module}::{name}"));
        }
        if signature.provides_context && signature.finalization_only {
            return Err("A context provider cannot be finalization-only".into());
        }
        if signature.provides_context && signature.contribution {
            return Err("A contribution cannot provide a lifecycle context".into());
        }
        let mut parameters = std::collections::BTreeSet::new();
        for parameter in &signature.parameters {
            if !parameters.insert(&parameter.name) {
                return Err(format!("Duplicate parameter {}", parameter.name));
            }
            let compatible = matches!(
                (&parameter.kind, &parameter.default),
                (_, None)
                    | (ParameterKind::Value, Some(_))
                    | (ParameterKind::Text, Some(MeaningValue::Text(_)))
                    | (ParameterKind::Bool, Some(MeaningValue::Bool(_)))
                    | (ParameterKind::Symbol, Some(MeaningValue::Symbol(_)))
            );
            if !compatible {
                return Err(format!("Invalid default for {}", parameter.name));
            }
        }
        self.exports
            .insert((module.into(), name.into()), signature.id.clone());
        self.operations
            .insert(signature.id.clone(), (signature, Arc::new(factory)));
        Ok(())
    }
    pub fn export_symbol(&mut self, module: &str, name: &str) -> Result<String, String> {
        let key = (module.into(), name.into());
        if self.symbols.contains_key(&key) || self.exports.contains_key(&key) {
            return Err(format!("Duplicate export {module}::{name}"));
        }
        let id = format!("{module}#{name}");
        self.symbols.insert(key, id.clone());
        Ok(id)
    }
    pub fn signature(&self, id: &str) -> Option<&OperationSignature> {
        self.operations.get(id).map(|x| &x.0)
    }
    pub fn operation_id(&self, module: &str, name: &str) -> Option<&str> {
        self.exports
            .get(&(module.into(), name.into()))
            .map(String::as_str)
    }
    pub fn symbol_id(&self, module: &str, name: &str) -> Option<&str> {
        self.symbols
            .get(&(module.into(), name.into()))
            .map(String::as_str)
    }
    pub fn contains_module(&self, module: &str) -> bool {
        self.exports
            .keys()
            .chain(self.symbols.keys())
            .any(|(m, _)| m == module)
    }
}

/// Instances are created per operation occurrence, never reused between analyses.
/// `before`/`after` run once; `enter`/`leave` run for each scheduler activation.
pub trait NativeOperation {
    /// Evaluate a value expression. Pending is distinct from a ready unknown symbol.
    /// Readers must be observational: no diagnostics or externally visible effects.
    /// A fresh reader instance may be evaluated on every propagation retry; store
    /// persistent query state in an analysis service rather than the reader instance.
    fn evaluate(&mut self, _context: &mut OperationContext<'_>) -> Result<Evaluation, Diagnostic> {
        Ok(Evaluation::Ready(SemanticValue::Absent))
    }
    fn before(&mut self, _context: &mut OperationContext<'_>) -> Result<(), Diagnostic> {
        Ok(())
    }
    fn enter(&mut self, _context: &mut OperationContext<'_>) -> Result<(), Diagnostic> {
        Ok(())
    }
    fn leave(&mut self, _context: &mut OperationContext<'_>) -> Result<(), Diagnostic> {
        Ok(())
    }
    fn after(&mut self, _context: &mut OperationContext<'_>) -> Result<(), Diagnostic> {
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq)]
pub enum Evaluation {
    Pending,
    Ready(SemanticValue),
}
/// Observational match relationship offered to interested analysis services.
/// Native effects are not executed while retaining a relationship.
#[derive(Clone, Debug, PartialEq)]
pub struct ResidualMatch {
    pub inputs: Vec<SemanticValue>,
    pub cases: Vec<ResidualCase>,
    pub span: Span,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ResidualCase {
    pub alternatives: Vec<Vec<Option<SemanticValue>>>,
    pub actions: Vec<ResidualAction>,
}
#[derive(Clone, Debug, PartialEq)]
pub enum ResidualAction {
    Call {
        operation: String,
        arguments: BTreeMap<String, SemanticValue>,
        span: Span,
    },
    Diagnostic(Diagnostic),
}
/// An analysis-local native library service. The engine drives progress to a
/// fixed point, calls settle once, resumes propagation, then calls finish once.
/// No target-language policy is built into this lifecycle.
pub trait AnalysisService: Any {
    fn as_any_mut(&mut self) -> &mut dyn Any;
    /// Its handles resolve to growing alternative sets. This lets pending
    /// readers release writers ordered after them without choosing a case early.
    fn retain_match(&mut self, _matching: &ResidualMatch) -> Result<(), Diagnostic> {
        Ok(())
    }
    fn offers_alternatives(&self) -> bool {
        false
    }
    /// Resolve an opaque analysis-local handle without effects. Matches await
    /// these values; generic value parameters can retain the handle itself.
    fn resolve_value(
        &mut self,
        _key: &SemanticValue,
        context: &mut OperationContext<'_>,
    ) -> Result<Evaluation, Diagnostic> {
        Err(Diagnostic::error(
            "semantic.unknown_handle",
            "This library does not resolve deferred values",
            context.occurrence.span.clone(),
        ))
    }
    fn progress(&mut self, _context: &mut OperationContext<'_>) -> Result<bool, Diagnostic> {
        Ok(false)
    }
    fn settle(&mut self, _context: &mut OperationContext<'_>) -> Result<(), Diagnostic> {
        Ok(())
    }
    fn finish(&mut self, _context: &mut OperationContext<'_>) -> Result<(), Diagnostic> {
        Ok(())
    }
}
pub(crate) enum ServicePhase {
    Progress,
    Settle,
    Finish,
}
pub(crate) fn run_services(
    occurrence: &Occurrence,
    state: &mut AnalysisState,
    phase: ServicePhase,
) -> bool {
    let arguments = BTreeMap::new();
    let mut context = OperationContext {
        occurrence,
        arguments: &arguments,
        operation_span: &occurrence.span,
        state,
    };
    let ids: Vec<_> = context.state.services.keys().cloned().collect();
    let mut changed = false;
    for id in ids {
        if context.state.failed_services.contains(&id) && !matches!(phase, ServicePhase::Finish) {
            continue;
        }
        let mut service = context
            .state
            .services
            .remove(&id)
            .expect("registered service");
        let result = match phase {
            ServicePhase::Progress => service.progress(&mut context),
            ServicePhase::Settle => service.settle(&mut context).map(|()| false),
            ServicePhase::Finish => service.finish(&mut context).map(|()| false),
        };
        match result {
            Ok(progress) => changed |= progress,
            Err(error) => {
                context.emit(error);
                context.state.failed_services.insert(id.clone());
            }
        }
        context.state.services.insert(id, service);
    }
    changed
}
#[derive(Default)]
pub(crate) struct AnalysisState {
    pub globals: BTreeMap<String, SemanticValue>,
    pub graph: SemanticGraph,
    pub record_origins: BTreeMap<usize, usize>,
    pub match_owners: std::collections::BTreeSet<usize>,
    pub diagnostics: Vec<Diagnostic>,
    pub hook_failed: bool,
    pub services: BTreeMap<String, Box<dyn AnalysisService>>,
    pub failed_services: std::collections::BTreeSet<String>,
}
pub struct OperationContext<'a> {
    pub occurrence: &'a Occurrence,
    pub arguments: &'a BTreeMap<String, SemanticValue>,
    pub operation_span: &'a Span,
    pub(crate) state: &'a mut AnalysisState,
}
impl OperationContext<'_> {
    /// Locate this occurrence's use of an identity, preserving capture origins.
    pub fn identity_span(&self, value: &SemanticValue) -> Span {
        let identity = match value {
            SemanticValue::Identity(identity) => identity,
            SemanticValue::Tuple(values) | SemanticValue::List(values) => {
                return values
                    .first()
                    .map(|value| self.identity_span(value))
                    .unwrap_or_else(|| self.occurrence.span.clone())
            }
            SemanticValue::Deferred { span, .. } => return span.clone(),
            SemanticValue::Source(source) => return source.span.clone(),
            _ => return self.occurrence.span.clone(),
        };
        for capture in self.occurrence.captures.values() {
            for occurrence in &capture.occurrences {
                let matches = match identity {
                    SemanticIdentity::Occurrence(id) => id == occurrence,
                    SemanticIdentity::Binding(binding) => {
                        self.state.graph.attachments.iter().any(|attachment| {
                            attachment.occurrence == *occurrence
                                && attachment.value == SemanticValue::Record(*binding)
                        })
                    }
                    SemanticIdentity::Service { .. } => false,
                };
                if matches {
                    return capture.span.clone();
                }
            }
        }
        self.occurrence.span.clone()
    }
    pub fn resolve(&mut self, value: &SemanticValue) -> Result<Evaluation, Diagnostic> {
        let SemanticValue::Deferred {
            service: id, key, ..
        } = value
        else {
            return Ok(Evaluation::Ready(value.clone()));
        };
        if self.state.failed_services.contains(id) {
            return Ok(Evaluation::Pending);
        }
        let Some(mut service) = self.state.services.remove(id) else {
            return Err(Diagnostic::error(
                "semantic.unknown_handle",
                "Deferred value library is unavailable",
                self.occurrence.span.clone(),
            ));
        };
        let result = service.resolve_value(key, self);
        self.state.services.insert(id.clone(), service);
        result
    }

    /// Borrow one library's analysis-local service while retaining graph access.
    pub fn with_service<T: AnalysisService, R>(
        &mut self,
        id: &str,
        action: impl FnOnce(&mut T, &mut Self) -> R,
    ) -> Option<R> {
        if self.state.failed_services.contains(id) {
            return None;
        }
        let mut service = self.state.services.remove(id)?;
        let result = service
            .as_any_mut()
            .downcast_mut::<T>()
            .map(|service| action(service, self));
        self.state.services.insert(id.into(), service);
        result
    }
    /// Read the latest attachment published by an operation on an occurrence.
    /// Handles are local to this analysis; absent attachments remain absent.
    pub fn attachment(&self, occurrence: usize, key: &str) -> Option<&SemanticValue> {
        self.state
            .graph
            .attachments
            .iter()
            .rev()
            .find(|attachment| attachment.occurrence == occurrence && attachment.key == key)
            .map(|attachment| &attachment.value)
    }
    pub fn argument(&self, name: &str) -> Option<&SemanticValue> {
        self.arguments.get(name)
    }
    pub fn global(&self, key: &str) -> SemanticValue {
        self.state
            .globals
            .get(key)
            .cloned()
            .unwrap_or(SemanticValue::Absent)
    }
    pub fn set_global(&mut self, key: &str, value: SemanticValue) {
        self.state.globals.insert(key.into(), value);
    }
    pub fn record(&mut self) -> usize {
        let id = self.state.graph.records.len();
        self.state.record_origins.insert(id, self.occurrence.id);
        self.state.graph.records.push(SemanticRecord {
            id,
            entries: vec![],
        });
        id
    }
    /// Original syntax owner of a record, independent of its fields or library.
    pub fn record_origin(&self, record: usize) -> Option<usize> {
        self.state.record_origins.get(&record).copied()
    }
    pub fn get(&self, record: usize, key: &SemanticKey) -> Option<&SemanticValue> {
        self.state
            .graph
            .records
            .get(record)?
            .entries
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v)
    }
    pub fn set(
        &mut self,
        record: usize,
        key: SemanticKey,
        value: SemanticValue,
    ) -> Result<(), Diagnostic> {
        let Some(record) = self.state.graph.records.get_mut(record) else {
            return Err(Diagnostic::error(
                "semantic.invalid_handle",
                "Invalid semantic record",
                self.occurrence.span.clone(),
            ));
        };
        if let Some((_, old)) = record.entries.iter_mut().find(|(k, _)| *k == key) {
            *old = value;
        } else {
            record.entries.push((key, value));
        }
        Ok(())
    }
    pub fn attach(&mut self, key: &str, value: SemanticValue) {
        self.state.graph.attachments.push(Attachment {
            occurrence: self.occurrence.id,
            key: key.into(),
            value,
        });
    }
    pub fn emit(&mut self, diagnostic: Diagnostic) {
        self.state.diagnostics.push(diagnostic);
    }
}

pub struct SemanticEngine {
    registry: OperationRegistry,
}
impl Default for SemanticEngine {
    fn default() -> Self {
        let mut registry = lexical::registry();
        types::register(&mut registry).expect("standard modules have disjoint exports");
        syntax::register(&mut registry).expect("standard modules have disjoint exports");
        Self::new(registry)
    }
}
impl SemanticEngine {
    pub fn new(registry: OperationRegistry) -> Self {
        Self { registry }
    }
    pub fn registry(&self) -> &OperationRegistry {
        &self.registry
    }
    pub fn registry_mut(&mut self) -> &mut OperationRegistry {
        &mut self.registry
    }
    pub fn compile(
        &self,
        entry: &str,
        sources: &BTreeMap<String, String>,
    ) -> crate::diagnostic::CompileResult<CompiledLanguage> {
        crate::compiler::compile_sources_with_registry(entry, sources, &self.registry)
    }
    pub fn analyze(&self, language: &CompiledLanguage, file: &str, source: &str) -> AnalysisResult {
        analyze(language, file, source, &self.registry)
    }
    pub fn load(&self, bytes: &[u8]) -> crate::diagnostic::CompileResult<CompiledLanguage> {
        let language = crate::load_compiled_language(bytes)?;
        validate_registry(&language, &self.registry)?;
        Ok(language)
    }
    pub fn compile_and_analyze(
        &self,
        entry: &str,
        sources: &BTreeMap<String, String>,
        file: &str,
        source: &str,
    ) -> crate::diagnostic::CompileResult<AnalysisResult> {
        let language = self.compile(entry, sources)?;
        Ok(self.analyze(&language, file, source))
    }
}

fn analyze(
    language: &CompiledLanguage,
    file: &str,
    source: &str,
    registry: &OperationRegistry,
) -> AnalysisResult {
    if let Err(diagnostics) = validate_registry(language, registry) {
        return AnalysisResult {
            ast: None,
            occurrences: vec![],
            graph: SemanticGraph::default(),
            diagnostics,
        };
    }
    let (parsed, occurrences) = crate::runtime::parse_occurrences(language, file, source);
    let mut result = AnalysisResult {
        ast: parsed.ast,
        occurrences,
        graph: SemanticGraph::default(),
        diagnostics: parsed.diagnostics,
    };
    if result.ast.is_some()
        && !result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == crate::diagnostic::Severity::Error)
    {
        execute(
            language,
            registry,
            &result.occurrences,
            &mut result.graph,
            &mut result.diagnostics,
        );
    }
    result
}
fn validate_registry(
    language: &CompiledLanguage,
    registry: &OperationRegistry,
) -> crate::diagnostic::CompileResult<()> {
    for definition in language.meanings.values() {
        for call in definition.groups.iter().flat_map(MeaningGroup::all_calls) {
            let Some(signature) = registry.signature(&call.operation) else {
                return Err(vec![Diagnostic::error(
                    "semantic.missing_operation",
                    format!("Native operation {} is not registered", call.operation),
                    call.span.clone(),
                )]);
            };
            if signature != &call.contract {
                return Err(vec![Diagnostic::error(
                    "semantic.contract_mismatch",
                    format!(
                        "Native operation {} has an incompatible interface",
                        call.operation
                    ),
                    call.span.clone(),
                )]);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    fn occurrence(
        id: usize,
        definition: &str,
        text: &str,
        children: Vec<Occurrence>,
    ) -> Occurrence {
        Occurrence {
            id,
            definition: definition.into(),
            span: Span::new("program", id, id + 1),
            captures: BTreeMap::from([(
                "name".into(),
                SourceCapture {
                    selection: None,
                    repeated: false,
                    occurrences: vec![],
                    value: AstValue::Text(text.into()),
                    span: Span::new("program", id, id + 1),
                    text: text.into(),
                },
            )]),
            children,
        }
    }
    fn definition(
        registry: &OperationRegistry,
        operation: &str,
        args: BTreeMap<String, MeaningValue>,
    ) -> MeaningDefinition {
        let id = format!("std/variables#{operation}");
        MeaningDefinition {
            boundary: false,
            groups: vec![MeaningGroup {
                matching: None,
                name: None,
                calls: vec![MeaningCall {
                    operation: id.clone(),
                    contract: registry.signature(&id).unwrap().clone(),
                    arguments: args,
                    span: Span::new("language", 0, 1),
                }],
                span: Span::new("language", 0, 1),
            }],
            precedence: vec![],
        }
    }
    fn language() -> CompiledLanguage {
        crate::compile_sources(
            "language.yl",
            &BTreeMap::from([(
                "language.yl".into(),
                "node Program = \".\" entry Program".into(),
            )]),
        )
        .unwrap()
    }
    #[test]
    fn native_lexical_state_is_restored_for_siblings_and_deferred_bodies() {
        let registry = lexical::registry();
        let mut language = language();
        for owner in ["Program", "Block", "FunctionBody"] {
            language.meanings.insert(
                owner.into(),
                definition(&registry, "lexicalScope", BTreeMap::new()),
            );
        }
        language.meanings.insert(
            "Declare".into(),
            definition(
                &registry,
                "declare",
                BTreeMap::from([
                    ("name".into(), MeaningValue::Capture("name".into())),
                    ("reassignable".into(), MeaningValue::Bool(true)),
                ]),
            ),
        );
        language.meanings.insert(
            "Use".into(),
            definition(
                &registry,
                "use",
                BTreeMap::from([("name".into(), MeaningValue::Capture("name".into()))]),
            ),
        );
        language
            .meanings
            .get_mut("Program")
            .unwrap()
            .precedence
            .push(MeaningChain {
                selectors: vec![
                    MeaningSelector {
                        definition: "Declare".into(),
                        group: None,
                    },
                    MeaningSelector {
                        definition: "FunctionBody".into(),
                        group: None,
                    },
                ],
                span: Span::default(),
            });
        let tree = vec![occurrence(
            0,
            "Program",
            "",
            vec![
                occurrence(
                    1,
                    "Block",
                    "",
                    vec![
                        occurrence(2, "Declare", "x", vec![]),
                        occurrence(
                            3,
                            "FunctionBody",
                            "",
                            vec![occurrence(4, "Use", "x", vec![])],
                        ),
                    ],
                ),
                occurrence(5, "Declare", "y", vec![]),
                occurrence(6, "Use", "y", vec![]),
                occurrence(7, "Use", "x", vec![]),
            ],
        )];
        let mut graph = SemanticGraph::default();
        let mut diagnostics = vec![];
        execute(&language, &registry, &tree, &mut graph, &mut diagnostics);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, "semantic.unresolved_reference");
        assert_eq!(diagnostics[0].primary.start, 7);
        let attachment = |occurrence, key: &str| {
            graph
                .attachments
                .iter()
                .find(|a| a.occurrence == occurrence && a.key == format!("std/variables#{key}"))
                .unwrap()
                .value
                .clone()
        };
        assert_eq!(attachment(2, "declaration"), attachment(4, "reference"));
        assert_eq!(attachment(5, "declaration"), attachment(6, "reference"));
        assert_ne!(attachment(0, "scope"), attachment(1, "scope"));
    }

    #[test]
    fn parser_traces_only_committed_original_occurrences_through_pipes_and_pratt() {
        let grammar = r#"
            import { separatedBy } from "std/parser"
            node Name = value: /[a-z]+/
            node Expression {
                node Atom = name: Name
                node Sum = left: Expression "+" right: Expression
                precedence { Sum }
            }
            node Comma = ","
            node Program = expressions: Expression* |> separatedBy(Comma)
            entry Program
        "#;
        let mut language = crate::compile_sources(
            "trace.yl",
            &BTreeMap::from([("trace.yl".into(), grammar.into())]),
        )
        .unwrap();
        language
            .meanings
            .insert("trace.yl#Program".into(), MeaningDefinition::default());
        let engine = SemanticEngine::default();
        let result = engine.analyze(&language, "source", "a+b,c");
        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
        let mut definitions = vec![];
        fn visit(occurrence: &Occurrence, definitions: &mut Vec<String>) {
            definitions.push(occurrence.definition.clone());
            for child in &occurrence.children {
                visit(child, definitions);
            }
        }
        for occurrence in &result.occurrences {
            visit(occurrence, &mut definitions);
        }
        assert_eq!(
            definitions
                .iter()
                .filter(|name| name.ends_with("#Name"))
                .count(),
            3
        );
        assert_eq!(
            definitions
                .iter()
                .filter(|name| name.ends_with("#Expression::Sum"))
                .count(),
            1
        );
        assert!(!definitions.iter().any(|name| name.ends_with("#Comma")));
        let failure = engine.analyze(&language, "source", "a+");
        assert!(failure.ast.is_none());
        assert!(failure.occurrences.is_empty());
        assert!(failure.graph.attachments.is_empty());
    }
    struct Recording {
        events: Arc<Mutex<Vec<String>>>,
    }
    impl NativeOperation for Recording {
        fn before(&mut self, context: &mut OperationContext<'_>) -> Result<(), Diagnostic> {
            self.events
                .lock()
                .unwrap()
                .push(format!("before:{}", context.occurrence.id));
            Ok(())
        }
        fn enter(&mut self, context: &mut OperationContext<'_>) -> Result<(), Diagnostic> {
            self.events
                .lock()
                .unwrap()
                .push(format!("enter:{}", context.occurrence.id));
            Ok(())
        }
        fn leave(&mut self, context: &mut OperationContext<'_>) -> Result<(), Diagnostic> {
            self.events
                .lock()
                .unwrap()
                .push(format!("leave:{}", context.occurrence.id));
            Ok(())
        }
        fn after(&mut self, context: &mut OperationContext<'_>) -> Result<(), Diagnostic> {
            self.events
                .lock()
                .unwrap()
                .push(format!("after:{}", context.occurrence.id));
            Ok(())
        }
    }
    #[test]
    fn custom_context_operation_is_once_per_occurrence_and_balanced_per_activation() {
        let events = Arc::new(Mutex::new(vec![]));
        let capture = events.clone();
        let mut registry = OperationRegistry::new();
        registry
            .register(
                "test/library",
                "record",
                OperationSignature {
                    id: "test/library#record".into(),
                    parameters: vec![],
                    finalization_only: false,
                    contribution: false,
                    provides_context: true,
                },
                move || {
                    Box::new(Recording {
                        events: capture.clone(),
                    })
                },
            )
            .unwrap();
        let call = MeaningCall {
            operation: "test/library#record".into(),
            contract: registry.signature("test/library#record").unwrap().clone(),
            arguments: BTreeMap::new(),
            span: Span::default(),
        };
        let mut language = language();
        for id in ["Outer", "Inner"] {
            language.meanings.insert(
                id.into(),
                MeaningDefinition {
                    boundary: false,
                    groups: vec![MeaningGroup {
                        matching: None,
                        name: Some("record".into()),
                        calls: vec![call.clone()],
                        span: Span::default(),
                    }],
                    precedence: vec![],
                },
            );
        }
        let tree = vec![occurrence(
            0,
            "Outer",
            "",
            vec![occurrence(1, "Inner", "", vec![])],
        )];
        let mut graph = SemanticGraph::default();
        let mut diagnostics = vec![];
        execute(&language, &registry, &tree, &mut graph, &mut diagnostics);
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        let events = events.lock().unwrap();
        for id in [0, 1] {
            assert_eq!(
                events
                    .iter()
                    .filter(|event| **event == format!("before:{id}"))
                    .count(),
                1
            );
            assert_eq!(
                events
                    .iter()
                    .filter(|event| **event == format!("after:{id}"))
                    .count(),
                1
            );
            assert_eq!(
                events
                    .iter()
                    .filter(|event| **event == format!("enter:{id}"))
                    .count(),
                events
                    .iter()
                    .filter(|event| **event == format!("leave:{id}"))
                    .count()
            );
        }
        assert!(events.iter().filter(|event| **event == "enter:0").count() > 1);
    }

    struct FailingSetup {
        events: Arc<Mutex<Vec<String>>>,
    }
    impl NativeOperation for FailingSetup {
        fn before(&mut self, context: &mut OperationContext<'_>) -> Result<(), Diagnostic> {
            self.events.lock().unwrap().push("before".into());
            Err(Diagnostic::error(
                "test.setup",
                "Setup failed",
                context.occurrence.span.clone(),
            ))
        }
        fn enter(&mut self, _context: &mut OperationContext<'_>) -> Result<(), Diagnostic> {
            panic!("A failed provider must not activate");
        }
        fn after(&mut self, _context: &mut OperationContext<'_>) -> Result<(), Diagnostic> {
            self.events.lock().unwrap().push("after".into());
            Ok(())
        }
    }
    #[test]
    fn failed_provider_setup_is_cleaned_without_activating_partial_state() {
        let events = Arc::new(Mutex::new(vec![]));
        let capture = events.clone();
        let mut registry = OperationRegistry::new();
        registry
            .register(
                "test/library",
                "fail",
                OperationSignature {
                    id: "test/library#fail".into(),
                    parameters: vec![],
                    finalization_only: false,
                    contribution: false,
                    provides_context: true,
                },
                move || {
                    Box::new(FailingSetup {
                        events: capture.clone(),
                    })
                },
            )
            .unwrap();
        let mut language = language();
        language.meanings.insert(
            "Root".into(),
            MeaningDefinition {
                boundary: false,
                groups: vec![MeaningGroup {
                    matching: None,
                    name: None,
                    calls: vec![MeaningCall {
                        operation: "test/library#fail".into(),
                        contract: registry.signature("test/library#fail").unwrap().clone(),
                        arguments: BTreeMap::new(),
                        span: Span::default(),
                    }],
                    span: Span::default(),
                }],
                precedence: vec![],
            },
        );
        let mut graph = SemanticGraph::default();
        let mut diagnostics = vec![];
        execute(
            &language,
            &registry,
            &[occurrence(0, "Root", "", vec![])],
            &mut graph,
            &mut diagnostics,
        );
        assert_eq!(diagnostics[0].code, "test.setup");
        assert_eq!(*events.lock().unwrap(), ["before", "after"]);
    }
    #[test]
    fn precedence_cycle_prevents_every_native_hook() {
        let events = Arc::new(Mutex::new(vec![]));
        let capture = events.clone();
        let mut registry = OperationRegistry::new();
        registry
            .register(
                "test/library",
                "record",
                OperationSignature {
                    id: "test/library#record".into(),
                    parameters: vec![],
                    finalization_only: false,
                    contribution: false,
                    provides_context: false,
                },
                move || {
                    Box::new(Recording {
                        events: capture.clone(),
                    })
                },
            )
            .unwrap();
        let mut language = language();
        let mut definition = MeaningDefinition {
            boundary: false,
            groups: vec![MeaningGroup {
                matching: None,
                name: None,
                calls: vec![MeaningCall {
                    operation: "test/library#record".into(),
                    contract: registry.signature("test/library#record").unwrap().clone(),
                    arguments: BTreeMap::new(),
                    span: Span::default(),
                }],
                span: Span::default(),
            }],
            precedence: vec![],
        };
        definition.precedence.push(MeaningChain {
            selectors: vec![
                MeaningSelector {
                    definition: "Root".into(),
                    group: None,
                },
                MeaningSelector {
                    definition: "Root".into(),
                    group: None,
                },
            ],
            span: Span::default(),
        });
        language.meanings.insert("Root".into(), definition);
        let tree = vec![occurrence(0, "Root", "", vec![])];
        let mut graph = SemanticGraph::default();
        let mut diagnostics = vec![];
        execute(&language, &registry, &tree, &mut graph, &mut diagnostics);
        assert_eq!(diagnostics[0].code, "semantic.precedence_cycle");
        assert!(events.lock().unwrap().is_empty());
    }
}
