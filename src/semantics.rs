//! Experimental native semantic operations over successful syntax occurrences.
use crate::{
    diagnostic::{Diagnostic, Span},
    AstNode, AstValue, CompiledLanguage,
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::Arc};
pub mod lexical;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MeaningDefinition {
    pub groups: Vec<MeaningGroup>,
    pub precedence: Vec<MeaningChain>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MeaningGroup {
    pub name: Option<String>,
    pub calls: Vec<MeaningCall>,
    pub span: Span,
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
    Capture(String),
    Text(String),
    Bool(bool),
    Symbol(String),
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
pub struct SourceCapture {
    pub value: AstValue,
    pub span: Span,
    pub text: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Occurrence {
    pub id: usize,
    pub definition: String,
    pub span: Span,
    pub captures: BTreeMap<String, SourceCapture>,
    pub children: Vec<Occurrence>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum SemanticValue {
    Absent,
    Text(String),
    Bool(bool),
    Symbol(String),
    Record(usize),
    Source(SourceCapture),
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
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OperationSignature {
    pub id: String,
    pub parameters: Vec<OperationParameter>,
    /// Context providers enclose the owning occurrence's semantic descendants.
    pub provides_context: bool,
}
type Factory = Arc<dyn Fn() -> Box<dyn NativeOperation> + Send + Sync>;
#[derive(Clone, Default)]
pub struct OperationRegistry {
    pub(crate) operations: BTreeMap<String, (OperationSignature, Factory)>,
    pub(crate) exports: BTreeMap<(String, String), String>,
    pub(crate) symbols: BTreeMap<(String, String), String>,
}
impl OperationRegistry {
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
            || self.operations.contains_key(&signature.id)
        {
            return Err(format!("Duplicate operation {module}::{name}"));
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
    fn before(
        &mut self,
        _context: &mut OperationContext<'_>,
    ) -> Result<Option<SemanticValue>, Diagnostic> {
        Ok(None)
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
#[derive(Default)]
pub(crate) struct AnalysisState {
    pub globals: BTreeMap<String, SemanticValue>,
    pub graph: SemanticGraph,
    pub diagnostics: Vec<Diagnostic>,
}
pub struct OperationContext<'a> {
    pub occurrence: &'a Occurrence,
    pub arguments: &'a BTreeMap<String, SemanticValue>,
    pub operation_span: &'a Span,
    pub(crate) state: &'a mut AnalysisState,
}
impl OperationContext<'_> {
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
        self.state.graph.records.push(SemanticRecord {
            id,
            entries: vec![],
        });
        id
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
        Self::new(lexical::registry())
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
    if result.ast.is_some() {
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
        for call in definition.groups.iter().flat_map(|group| &group.calls) {
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

struct Instance {
    call: MeaningCall,
    arguments: BTreeMap<String, SemanticValue>,
    native: Box<dyn NativeOperation>,
    started: bool,
}
struct Task<'a> {
    owner: &'a Occurrence,
    group: &'a MeaningGroup,
    instances: Vec<Instance>,
    dependencies: std::collections::BTreeSet<usize>,
    done: bool,
}
#[derive(Clone, Copy)]
enum Hook {
    Before,
    Enter,
    Leave,
    After,
}
fn invoke(
    instance: &mut Instance,
    owner: &Occurrence,
    state: &mut AnalysisState,
    hook: Hook,
) -> Result<Option<SemanticValue>, Diagnostic> {
    let mut context = OperationContext {
        occurrence: owner,
        arguments: &instance.arguments,
        operation_span: &instance.call.span,
        state,
    };
    match hook {
        Hook::Before => instance.native.before(&mut context),
        Hook::Enter => instance.native.enter(&mut context).map(|()| None),
        Hook::Leave => instance.native.leave(&mut context).map(|()| None),
        Hook::After => instance.native.after(&mut context).map(|()| None),
    }
}
fn within(owner: usize, region: usize, parents: &BTreeMap<usize, Option<usize>>) -> bool {
    let mut current = Some(owner);
    while let Some(id) = current {
        if id == region {
            return true;
        }
        current = parents.get(&id).copied().flatten();
    }
    false
}
fn activation(
    owner: usize,
    tasks: &[Task<'_>],
    parents: &BTreeMap<usize, Option<usize>>,
) -> Vec<(usize, usize)> {
    let mut ancestry = vec![];
    let mut current = Some(owner);
    while let Some(id) = current {
        ancestry.push(id);
        current = parents.get(&id).copied().flatten();
    }
    ancestry.reverse();
    let mut result = vec![];
    for id in ancestry {
        for (task_id, task) in tasks
            .iter()
            .enumerate()
            .filter(|(_, task)| task.owner.id == id)
        {
            for (operation_id, instance) in task.instances.iter().enumerate() {
                if instance.started && instance.call.contract.provides_context {
                    result.push((task_id, operation_id));
                }
            }
        }
    }
    result
}
fn enter(
    active: &[(usize, usize)],
    tasks: &mut [Task<'_>],
    state: &mut AnalysisState,
) -> Result<(), Diagnostic> {
    for (entered, &(task, instance)) in active.iter().enumerate() {
        let owner = tasks[task].owner;
        if let Err(diagnostic) = invoke(
            &mut tasks[task].instances[instance],
            owner,
            state,
            Hook::Enter,
        ) {
            leave(&active[..entered], tasks, state);
            return Err(diagnostic);
        }
    }
    Ok(())
}
fn leave(active: &[(usize, usize)], tasks: &mut [Task<'_>], state: &mut AnalysisState) {
    for &(task, instance) in active.iter().rev() {
        let owner = tasks[task].owner;
        if let Err(diagnostic) = invoke(
            &mut tasks[task].instances[instance],
            owner,
            state,
            Hook::Leave,
        ) {
            state.diagnostics.push(diagnostic);
        }
    }
}
fn execute(
    language: &CompiledLanguage,
    registry: &OperationRegistry,
    occurrences: &[Occurrence],
    graph: &mut SemanticGraph,
    diagnostics: &mut Vec<Diagnostic>,
) {
    fn flatten<'a>(
        occurrence: &'a Occurrence,
        parent: Option<usize>,
        all: &mut Vec<&'a Occurrence>,
        parents: &mut BTreeMap<usize, Option<usize>>,
    ) {
        all.push(occurrence);
        parents.insert(occurrence.id, parent);
        for child in &occurrence.children {
            flatten(child, Some(occurrence.id), all, parents);
        }
    }
    let mut all = vec![];
    let mut parents = BTreeMap::new();
    for occurrence in occurrences {
        flatten(occurrence, None, &mut all, &mut parents);
    }
    let mut tasks = vec![];
    for owner in &all {
        let Some(definition) = language.meanings.get(&owner.definition) else {
            continue;
        };
        for group in &definition.groups {
            let mut instances = vec![];
            for call in &group.calls {
                let Some((signature, factory)) = registry.operations.get(&call.operation) else {
                    diagnostics.push(Diagnostic::error(
                        "semantic.missing_operation",
                        format!("Native operation {} is not registered", call.operation),
                        call.span.clone(),
                    ));
                    return;
                };
                if signature != &call.contract {
                    diagnostics.push(Diagnostic::error(
                        "semantic.contract_mismatch",
                        format!(
                            "Native operation {} has an incompatible interface",
                            call.operation
                        ),
                        call.span.clone(),
                    ));
                    return;
                }
                let mut arguments = BTreeMap::new();
                for (name, value) in &call.arguments {
                    let value = match value {
                        MeaningValue::Capture(capture) => {
                            let Some(capture) = owner.captures.get(capture) else {
                                diagnostics.push(Diagnostic::error(
                                    "semantic.missing_capture",
                                    format!("Capture {capture} is not present on this occurrence"),
                                    owner.span.clone(),
                                ));
                                return;
                            };
                            SemanticValue::Source(capture.clone())
                        }
                        MeaningValue::Text(text) => SemanticValue::Text(text.clone()),
                        MeaningValue::Bool(value) => SemanticValue::Bool(*value),
                        MeaningValue::Symbol(value) => SemanticValue::Symbol(value.clone()),
                    };
                    arguments.insert(name.clone(), value);
                }
                instances.push(Instance {
                    call: call.clone(),
                    arguments,
                    native: factory(),
                    started: false,
                });
            }
            tasks.push(Task {
                owner,
                group,
                instances,
                dependencies: Default::default(),
                done: false,
            });
        }
    }
    // A context provider encloses its owner independently of user scheduling rules.
    for provider in 0..tasks.len() {
        if !tasks[provider]
            .instances
            .iter()
            .any(|instance| instance.call.contract.provides_context)
        {
            continue;
        }
        for child in 0..tasks.len() {
            let another_provider = tasks[child].owner.id == tasks[provider].owner.id
                && tasks[child]
                    .instances
                    .iter()
                    .any(|instance| instance.call.contract.provides_context);
            if provider != child
                && !another_provider
                && within(tasks[child].owner.id, tasks[provider].owner.id, &parents)
            {
                tasks[child].dependencies.insert(provider);
            }
        }
    }
    for owner in &all {
        let Some(definition) = language.meanings.get(&owner.definition) else {
            continue;
        };
        for chain in &definition.precedence {
            let selections: Vec<Vec<usize>> = chain
                .selectors
                .iter()
                .map(|selector| {
                    tasks
                        .iter()
                        .enumerate()
                        .filter(|(_, task)| {
                            within(task.owner.id, owner.id, &parents)
                                && task.owner.definition == selector.definition
                                && selector
                                    .group
                                    .as_ref()
                                    .is_none_or(|name| task.group.name.as_ref() == Some(name))
                        })
                        .map(|(id, _)| id)
                        .collect()
                })
                .collect();
            for pair in selections.windows(2) {
                for &before in &pair[0] {
                    for &after in &pair[1] {
                        tasks[after].dependencies.insert(before);
                    }
                }
            }
        }
    }
    // Validate the whole graph before any native operation runs.
    let mut sorted = vec![];
    let mut selected = std::collections::BTreeSet::new();
    while selected.len() < tasks.len() {
        let next = (0..tasks.len())
            .filter(|id| !selected.contains(id) && tasks[*id].dependencies.is_subset(&selected))
            .min_by_key(|id| (tasks[*id].owner.span.start, tasks[*id].owner.id, *id));
        let Some(next) = next else {
            let first = (0..tasks.len())
                .find(|id| !selected.contains(id))
                .unwrap_or(0);
            let mut diagnostic = Diagnostic::error(
                "semantic.precedence_cycle",
                "Meaning precedence contains a dependency cycle",
                tasks[first].group.span.clone(),
            );
            diagnostic.secondary = tasks
                .iter()
                .enumerate()
                .filter(|(id, _)| !selected.contains(id))
                .skip(1)
                .map(|(_, task)| task.group.span.clone())
                .collect();
            diagnostics.push(diagnostic);
            return;
        };
        selected.insert(next);
        sorted.push(next);
    }
    let mut state = AnalysisState::default();
    let mut finalized = std::collections::BTreeSet::new();
    let mut failed = false;
    for task_id in sorted {
        let owner = tasks[task_id].owner;
        let mut active = activation(owner.id, &tasks, &parents);
        if let Err(diagnostic) = enter(&active, &mut tasks, &mut state) {
            state.diagnostics.push(diagnostic);
            failed = true;
            break;
        }
        for operation_id in 0..tasks[task_id].instances.len() {
            tasks[task_id].instances[operation_id].started = true;
            match invoke(
                &mut tasks[task_id].instances[operation_id],
                owner,
                &mut state,
                Hook::Before,
            ) {
                Ok(Some(value)) => state.graph.attachments.push(Attachment {
                    occurrence: owner.id,
                    key: tasks[task_id].instances[operation_id]
                        .call
                        .operation
                        .clone(),
                    value,
                }),
                Ok(None) => {}
                Err(diagnostic) => {
                    state.diagnostics.push(diagnostic);
                    failed = true;
                    break;
                }
            }
            if tasks[task_id].instances[operation_id]
                .call
                .contract
                .provides_context
            {
                if let Err(diagnostic) = invoke(
                    &mut tasks[task_id].instances[operation_id],
                    owner,
                    &mut state,
                    Hook::Enter,
                ) {
                    state.diagnostics.push(diagnostic);
                    failed = true;
                    break;
                }
                active.push((task_id, operation_id));
            }
        }
        leave(&active, &mut tasks, &mut state);
        tasks[task_id].done = true;
        if failed {
            break;
        }
        for occurrence in all.iter().rev() {
            if finalized.contains(&occurrence.id)
                || tasks
                    .iter()
                    .any(|task| !task.done && within(task.owner.id, occurrence.id, &parents))
            {
                continue;
            }
            finish(occurrence, &mut tasks, &parents, &mut state);
            finalized.insert(occurrence.id);
        }
    }
    if failed {
        for occurrence in all.iter().rev() {
            if !finalized.contains(&occurrence.id) {
                finish(occurrence, &mut tasks, &parents, &mut state);
            }
        }
    }
    *graph = state.graph;
    diagnostics.extend(state.diagnostics);
}
fn finish(
    owner: &Occurrence,
    tasks: &mut [Task<'_>],
    parents: &BTreeMap<usize, Option<usize>>,
    state: &mut AnalysisState,
) {
    let active = activation(owner.id, tasks, parents);
    if let Err(diagnostic) = enter(&active, tasks, state) {
        state.diagnostics.push(diagnostic);
        return;
    }
    for task in tasks
        .iter_mut()
        .rev()
        .filter(|task| task.owner.id == owner.id)
    {
        for instance in task
            .instances
            .iter_mut()
            .rev()
            .filter(|instance| instance.started)
        {
            if let Err(diagnostic) = invoke(instance, owner, state, Hook::After) {
                state.diagnostics.push(diagnostic);
            }
        }
    }
    leave(&active, tasks, state);
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
        let id = format!("std/semantics#{operation}");
        MeaningDefinition {
            groups: vec![MeaningGroup {
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
                "declareVariable",
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
                .find(|a| a.occurrence == occurrence && a.key == format!("std/semantics#{key}"))
                .unwrap()
                .value
                .clone()
        };
        assert_eq!(attachment(2, "declaration"), attachment(4, "reference"));
        assert_eq!(attachment(5, "declaration"), attachment(6, "reference"));
        assert_ne!(attachment(0, "scope"), attachment(1, "scope"));
    }
    struct Recording {
        events: Arc<Mutex<Vec<String>>>,
    }
    impl NativeOperation for Recording {
        fn before(
            &mut self,
            context: &mut OperationContext<'_>,
        ) -> Result<Option<SemanticValue>, Diagnostic> {
            self.events
                .lock()
                .unwrap()
                .push(format!("before:{}", context.occurrence.id));
            Ok(None)
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
                    groups: vec![MeaningGroup {
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
            groups: vec![MeaningGroup {
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
