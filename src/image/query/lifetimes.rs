//! Conservative access summaries derived from indexed executable query code.
//!
//! No source syntax or target-program occurrence count is needed. A query-free
//! function can survive propagation rounds; a store reader must be invalidated
//! when the producer snapshot changes. Unknown/dynamic keys remain runtime work.
use super::{Expr, ExprId, Opcode, QueryImage};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueryPlan {
    /// Key expressions of all store reads, including reads in guarded callbacks.
    pub reads: Vec<ExprId>,
    /// Key expressions of all contributions, including currently inactive ones.
    pub writes: Vec<ExprId>,
    /// True if a function transitively observes the mutable producer snapshot.
    pub store_dependent_functions: BTreeMap<String, bool>,
    /// Transitive read effects for each indexed expression. The runtime combines
    /// these flags for concrete producer guards without rewalking function code.
    pub store_dependent_expressions: Vec<bool>,
}

impl QueryPlan {
    pub fn compile(image: &QueryImage) -> Self {
        let mut plan = Self::default();
        for expression in &image.expressions {
            if let Expr::Call { opcode, args } = expression {
                if let Some(key) = args.first() {
                    match opcode {
                        Opcode::Get
                        | Opcode::GetAll
                        | Opcode::GetEarlier
                        | Opcode::Facts
                        | Opcode::Hypothetical
                        | Opcode::HypotheticalAll
                        | Opcode::HypotheticalChecked => plan.reads.push(*key),
                        Opcode::Contribute | Opcode::ContributeAll => plan.writes.push(*key),
                        _ => {}
                    }
                }
            }
        }
        let count = image.expressions.len() + image.actions.len();
        let mut reverse = vec![vec![]; count];
        let mut dependent = vec![false; count];
        let mut pending = vec![];
        for (node, is_dependent) in dependent.iter_mut().enumerate() {
            let mut edges = image.edges(node).unwrap_or_default();
            if let Some(Expr::Invoke { name, .. }) = image.expressions.get(node) {
                if let Some(function) = image.functions.get(name) {
                    edges.push(function.body as usize);
                }
            }
            for edge in edges {
                if let Some(readers) = reverse.get_mut(edge) {
                    readers.push(node);
                }
            }
            if let Some(Expr::Call {
                opcode:
                    Opcode::Get
                    | Opcode::GetAll
                    | Opcode::GetEarlier
                    | Opcode::Facts
                    | Opcode::Hypothetical
                    | Opcode::HypotheticalAll
                    | Opcode::HypotheticalChecked,
                ..
            }) = image.expressions.get(node)
            {
                *is_dependent = true;
                pending.push(node);
            }
        }
        while let Some(node) = pending.pop() {
            for reader in &reverse[node] {
                if !dependent[*reader] {
                    dependent[*reader] = true;
                    pending.push(*reader);
                }
            }
        }
        for (name, function) in &image.functions {
            plan.store_dependent_functions.insert(
                name.clone(),
                dependent
                    .get(function.body as usize)
                    .copied()
                    .unwrap_or(true),
            );
        }
        dependent.truncate(image.expressions.len());
        plan.store_dependent_expressions = dependent;
        plan
    }

    pub(crate) fn reads_store(&self, roots: &[ExprId]) -> bool {
        roots.iter().any(|id| {
            self.store_dependent_expressions
                .get(*id as usize)
                .copied()
                .unwrap_or(true)
        })
    }
}

/// Include every branch and callback, and follow recursive calls only once.
/// The result bounds possible future readers independently of evaluation order.
pub(crate) fn functions_used(image: &QueryImage, roots: &[ExprId]) -> BTreeSet<String> {
    reachable(image, roots)
        .into_iter()
        .filter_map(|id| {
            if let Expr::Invoke { name, .. } = &image.expressions[id as usize] {
                Some(name.clone())
            } else {
                None
            }
        })
        .collect()
}

fn reachable(image: &QueryImage, roots: &[ExprId]) -> BTreeSet<ExprId> {
    let mut pending = roots.iter().map(|id| *id as usize).collect::<Vec<_>>();
    let mut visited = BTreeSet::new();
    while let Some(node) = pending.pop() {
        if !visited.insert(node) {
            continue;
        }
        if let Some(Expr::Invoke { name, .. }) = image.expressions.get(node) {
            if let Some(function) = image.functions.get(name) {
                pending.push(function.body as usize);
            }
        }
        // Validation runs before execution. Compilation can still encounter bad
        // indexes: leave structural diagnostics to the existing validator.
        if let Ok(edges) = image.edges(node) {
            pending.extend(edges);
        }
    }
    visited
        .into_iter()
        .filter(|id| *id < image.expressions.len())
        .map(|id| id as ExprId)
        .collect()
}
