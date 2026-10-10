//! Dependency scheduling and native lifecycle activation. No library policy lives here.
use super::*;

struct Instance {
    call: MeaningCall,
    arguments: BTreeMap<String, SemanticValue>,
    native: Box<dyn NativeOperation>,
    started: bool,
    ready: bool,
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
    let result = match hook {
        Hook::Before => instance.native.before(&mut context),
        Hook::Enter => instance.native.enter(&mut context).map(|()| None),
        Hook::Leave => instance.native.leave(&mut context).map(|()| None),
        Hook::After => instance.native.after(&mut context).map(|()| None),
    };
    if result.is_err() {
        context.state.hook_failed = true;
    }
    result
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
                if instance.ready && instance.call.contract.provides_context {
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
            tasks[task].instances[instance].ready = false;
            if let Err(cleanup) = invoke(
                &mut tasks[task].instances[instance],
                owner,
                state,
                Hook::Leave,
            ) {
                state.diagnostics.push(cleanup);
            }
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
pub(super) fn execute(
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
                    ready: false,
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

    let mut dependencies: Vec<_> = tasks.iter().map(|task| task.dependencies.clone()).collect();
    let mut chain_spans = BTreeMap::new();
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
                if pair[0].is_empty() || pair[1].is_empty() {
                    continue;
                }
                // A barrier expresses all-before-all without a quadratic Cartesian edge set.
                let barrier = dependencies.len();
                dependencies.push(pair[0].iter().copied().collect());
                chain_spans.insert(barrier, chain.span.clone());
                for &after in &pair[1] {
                    dependencies[after].insert(barrier);
                }
            }
        }
    }
    // Kahn's algorithm validates the complete graph before invoking native hooks.
    let mut remaining: Vec<_> = dependencies
        .iter()
        .map(std::collections::BTreeSet::len)
        .collect();
    let mut dependents = vec![vec![]; dependencies.len()];
    for (after, before) in dependencies.iter().enumerate() {
        for &before in before {
            dependents[before].push(after);
        }
    }
    let priority = |id: usize| {
        if id < tasks.len() {
            (tasks[id].owner.span.start, tasks[id].owner.id, id)
        } else {
            (0, 0, id)
        }
    };
    let mut ready: std::collections::BTreeSet<_> = remaining
        .iter()
        .enumerate()
        .filter(|(_, count)| **count == 0)
        .map(|(id, _)| priority(id))
        .collect();
    let mut sorted = vec![];
    let mut visited = 0;
    while let Some((_, _, id)) = ready.pop_first() {
        visited += 1;
        if id < tasks.len() {
            sorted.push(id);
        }
        for &after in &dependents[id] {
            remaining[after] -= 1;
            if remaining[after] == 0 {
                ready.insert(priority(after));
            }
        }
    }
    if visited != dependencies.len() {
        let first = (0..tasks.len()).find(|id| remaining[*id] > 0).unwrap_or(0);
        let mut diagnostic = Diagnostic::error(
            "semantic.precedence_cycle",
            "Meaning precedence contains a dependency cycle",
            tasks[first].group.span.clone(),
        );
        diagnostic.secondary = chain_spans
            .into_iter()
            .filter(|(id, _)| remaining[*id] > 0)
            .map(|(_, span)| span)
            .collect();
        diagnostics.push(diagnostic);
        return;
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
            tasks[task_id].instances[operation_id].ready = true;
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
                    tasks[task_id].instances[operation_id].ready = false;
                    if let Err(cleanup) = invoke(
                        &mut tasks[task_id].instances[operation_id],
                        owner,
                        &mut state,
                        Hook::Leave,
                    ) {
                        state.diagnostics.push(cleanup);
                    }
                    state.diagnostics.push(diagnostic);
                    failed = true;
                    break;
                }
                active.push((task_id, operation_id));
            }
        }
        leave(&active, &mut tasks, &mut state);
        tasks[task_id].done = true;
        failed |= state.hook_failed;
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
        if state.hook_failed {
            failed = true;
            break;
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
    if !tasks.iter().any(|task| {
        task.owner.id == owner.id && task.instances.iter().any(|instance| instance.started)
    }) {
        return;
    }
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
        for instance in task.instances.iter_mut().rev().filter(|instance| {
            instance.started && (!instance.call.contract.provides_context || !instance.ready)
        }) {
            if let Err(diagnostic) = invoke(instance, owner, state, Hook::After) {
                state.diagnostics.push(diagnostic);
            }
        }
    }
    leave(&active, tasks, state);
    // Context-provider cleanup runs after its final leave, under enclosing providers.
    // An implementation may release resources here without being re-entered afterward.
    for (position, &(task, operation)) in active.iter().enumerate().rev() {
        if tasks[task].owner.id != owner.id {
            continue;
        }
        let enclosing = &active[..position];
        if let Err(diagnostic) = enter(enclosing, tasks, state) {
            state.diagnostics.push(diagnostic);
            continue;
        }
        if let Err(diagnostic) = invoke(
            &mut tasks[task].instances[operation],
            owner,
            state,
            Hook::After,
        ) {
            state.diagnostics.push(diagnostic);
        }
        leave(enclosing, tasks, state);
    }
}
