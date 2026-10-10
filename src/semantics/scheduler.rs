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
) -> Result<(), Diagnostic> {
    let mut context = OperationContext {
        occurrence: owner,
        arguments: &instance.arguments,
        operation_span: &instance.call.span,
        state,
    };
    let result = match hook {
        Hook::Before => instance.native.before(&mut context),
        Hook::Enter => instance.native.enter(&mut context),
        Hook::Leave => instance.native.leave(&mut context),
        Hook::After => instance.native.after(&mut context),
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

    // Ordinary meanings follow source-order DFS completion. Explicit constraints
    // can suspend a descendant and allow its enclosing construct to continue.
    fn completed_order(
        occurrence: &Occurrence,
        order: &mut BTreeMap<usize, usize>,
        next: &mut usize,
    ) {
        for child in &occurrence.children {
            completed_order(child, order, next);
        }
        order.insert(occurrence.id, *next);
        *next += 1;
    }
    let mut order = BTreeMap::new();
    let mut next = 0;
    for occurrence in occurrences {
        completed_order(occurrence, &mut order, &mut next);
    }
    let mut dependencies: Vec<_> = tasks.iter().map(|task| task.dependencies.clone()).collect();
    let mut graph_owners: Vec<_> = tasks.iter().map(|task| task.owner).collect();
    let bare_definitions: std::collections::BTreeSet<_> = language
        .meanings
        .values()
        .flat_map(|definition| &definition.precedence)
        .flat_map(|chain| &chain.selectors)
        .filter(|selector| selector.group.is_none())
        .map(|selector| selector.definition.as_str())
        .collect();
    let mut empty_owners = BTreeMap::new();
    for owner in &all {
        if bare_definitions.contains(owner.definition.as_str())
            && !tasks.iter().any(|task| task.owner.id == owner.id)
        {
            let id = dependencies.len();
            empty_owners.insert(owner.id, id);
            let prerequisites = tasks
                .iter()
                .enumerate()
                .filter(|(_, task)| {
                    task.instances
                        .iter()
                        .any(|instance| instance.call.contract.provides_context)
                        && within(owner.id, task.owner.id, &parents)
                })
                .map(|(id, _)| id)
                .collect();
            dependencies.push(prerequisites);
            graph_owners.push(owner);
        }
    }
    #[derive(Clone)]
    struct Selection {
        nodes: Vec<usize>,
        whole: Vec<usize>,
    }
    struct Fence {
        before: Selection,
        after: Selection,
        barrier: usize,
    }
    let mut fences = vec![];
    let mut whole_predecessors: BTreeMap<usize, std::collections::BTreeSet<usize>> =
        BTreeMap::new();
    let mut chain_spans = BTreeMap::new();
    for owner in &all {
        let Some(definition) = language.meanings.get(&owner.definition) else {
            continue;
        };
        for chain in &definition.precedence {
            let selections: Vec<_> = chain
                .selectors
                .iter()
                .map(|selector| {
                    let matching: Vec<_> = all
                        .iter()
                        .filter(|candidate| {
                            candidate.definition == selector.definition
                                && within(candidate.id, owner.id, &parents)
                        })
                        .collect();
                    let mut nodes = vec![];
                    for candidate in &matching {
                        nodes.extend(
                            tasks
                                .iter()
                                .enumerate()
                                .filter(|(_, task)| {
                                    task.owner.id == candidate.id
                                        && selector.group.as_ref().is_none_or(|name| {
                                            task.group.name.as_ref() == Some(name)
                                        })
                                })
                                .map(|(id, _)| id),
                        );
                        if selector.group.is_none() {
                            if let Some(id) = empty_owners.get(&candidate.id) {
                                nodes.push(*id);
                            }
                        }
                    }
                    Selection {
                        nodes,
                        whole: if selector.group.is_none() {
                            matching.iter().map(|candidate| candidate.id).collect()
                        } else {
                            vec![]
                        },
                    }
                })
                .filter(|selection| !selection.nodes.is_empty())
                .collect();
            for pair in selections.windows(2) {
                for &node in &pair[0].nodes {
                    for &before in &pair[0].whole {
                        if graph_owners[node].id == before {
                            whole_predecessors.entry(node).or_default().insert(before);
                        }
                    }
                }
                let barrier = dependencies.len();
                dependencies.push(pair[0].nodes.iter().copied().collect());
                chain_spans.insert(barrier, chain.span.clone());
                for &after in &pair[1].nodes {
                    dependencies[after].insert(barrier);
                }
                fences.push(Fence {
                    before: pair[0].clone(),
                    after: pair[1].clone(),
                    barrier,
                });
            }
        }
    }
    // Ancestor/local constraints accumulate. Bare selectors postpone traversal,
    // while named selectors order only that owner's chosen operation group.
    let strict = dependencies.clone();

    for fence in fences {
        for region in fence.after.whole {
            // Explicit local predecessors of this construct are exceptions to
            // enclosing traversal fences as well as their own local fence.
            let mut required = std::collections::BTreeSet::new();
            let mut pending = fence.before.nodes.clone();
            for &node in &fence.after.nodes {
                if graph_owners[node].id == region {
                    pending.extend(strict[node].iter().copied());
                }
            }
            while let Some(id) = pending.pop() {
                if required.insert(id) {
                    pending.extend(strict[id].iter().copied());
                }
            }
            let earlier_whole: std::collections::BTreeSet<_> = required
                .iter()
                .flat_map(|id| whole_predecessors.get(id).into_iter().flatten().copied())
                .collect();
            for (id, candidate) in graph_owners.iter().enumerate() {
                if candidate.id == region || !within(candidate.id, region, &parents) {
                    continue;
                }
                let explicitly_earlier_subtree = earlier_whole.iter().any(|before| {
                    within(*before, region, &parents) && within(candidate.id, *before, &parents)
                });
                if required.contains(&id) || explicitly_earlier_subtree {
                    continue;
                }
                dependencies[id].insert(fence.barrier);
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
        if id < graph_owners.len() {
            (1, order[&graph_owners[id].id], id)
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
        let primary = chain_spans
            .iter()
            .find(|(id, _)| remaining[**id] > 0)
            .map(|(_, span)| span.clone())
            .or_else(|| {
                tasks
                    .iter()
                    .enumerate()
                    .find(|(id, _)| remaining[*id] > 0)
                    .map(|(_, task)| task.group.span.clone())
            })
            .or_else(|| {
                graph_owners
                    .iter()
                    .enumerate()
                    .find(|(id, _)| remaining[*id] > 0)
                    .map(|(_, owner)| owner.span.clone())
            })
            .unwrap_or_default();
        let mut diagnostic = Diagnostic::error(
            "semantic.precedence_cycle",
            "Meaning precedence contains a dependency cycle",
            primary,
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
                Ok(()) => {}
                Err(diagnostic) => {
                    state.diagnostics.push(diagnostic);
                    failed = true;
                    break;
                }
            }
            tasks[task_id].instances[operation_id].ready = true;
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
            if tasks[task_id].instances[operation_id]
                .call
                .contract
                .provides_context
            {
                active.push((task_id, operation_id));
            } else if let Err(diagnostic) = invoke(
                &mut tasks[task_id].instances[operation_id],
                owner,
                &mut state,
                Hook::Leave,
            ) {
                state.diagnostics.push(diagnostic);
                failed = true;
                break;
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
    let mut active = activation(owner.id, tasks, parents);
    if let Err(diagnostic) = enter(&active, tasks, state) {
        state.diagnostics.push(diagnostic);
        active.clear();
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
            .filter(|instance| instance.started && !instance.call.contract.provides_context)
        {
            if let Err(diagnostic) = invoke(instance, owner, state, Hook::After) {
                state.diagnostics.push(diagnostic);
            }
        }
    }
    leave(&active, tasks, state);
    let providers: Vec<_> = tasks
        .iter()
        .enumerate()
        .filter(|(_, task)| task.owner.id == owner.id)
        .flat_map(|(task, value)| {
            value
                .instances
                .iter()
                .enumerate()
                .filter(|(_, instance)| instance.started && instance.call.contract.provides_context)
                .map(move |(operation, _)| (task, operation))
        })
        .collect();
    // Always attempt cleanup, even when activation failed. Completed providers are
    // never re-entered; cleanup sees whatever enclosing contexts remain available.
    for (task, operation) in providers.into_iter().rev() {
        let mut enclosing = activation(owner.id, tasks, parents);
        if let Some(position) = enclosing
            .iter()
            .position(|target| *target == (task, operation))
        {
            enclosing.truncate(position);
        }
        if let Err(diagnostic) = enter(&enclosing, tasks, state) {
            state.diagnostics.push(diagnostic);
            enclosing.clear();
        }
        if let Err(diagnostic) = invoke(
            &mut tasks[task].instances[operation],
            owner,
            state,
            Hook::After,
        ) {
            state.diagnostics.push(diagnostic);
        }
        tasks[task].instances[operation].ready = false;
        leave(&enclosing, tasks, state);
    }
}
