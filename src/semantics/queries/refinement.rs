//! Conservative structural upper bounds and dependency-driven wakeups.
//!
//! Templates include every discovered guarded writer. Concrete observations may
//! sleep only while both candidate membership and producer versions are unchanged.
//! This retires evaluation work, not publicly exported fact roots.
use super::structural::StructuralIndex;
use super::{
    address, Effect, Engine, Expr, ExprId, Locals, Occurrence, Outcome, Pattern, QueryKey, Trace,
};
use std::collections::BTreeSet;
use std::rc::Rc;

pub(super) type ObservedInputs = Vec<Vec<(usize, u64)>>;

impl Engine<'_> {
    pub(super) fn prepare_structural_templates(&mut self) {
        #[cfg(test)]
        if self.cache_disabled || self.exact_only {
            self.structural_templates = None;
            return;
        }
        let patterns = self
            .producers
            .iter()
            .enumerate()
            .filter_map(|(id, producer)| match producer.effect {
                Effect::Contribution(key, _) | Effect::Contributions(key, _) => {
                    Some((id, self.structural_shape(key, producer.owner, 0)))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        self.structural_templates = Some(Rc::new(StructuralIndex::new(patterns)));
    }

    /// Only query-free values become exact template constraints. A pending query
    /// is a hole, while known later tuple positions remain usable discriminators.
    fn structural_shape(&self, expression: ExprId, owner: &Occurrence, depth: usize) -> Pattern {
        if depth > 24 {
            return Pattern::Any;
        }
        if !self.lifetime_plan.reads_store(&[expression]) {
            if let Ok(Outcome::Ready(value)) =
                self.eval(&expression, owner, &Locals::new(), &mut Trace::default())
            {
                if let Ok(key) = address(&value, &owner.span) {
                    return key.map(Pattern::Exact).unwrap_or(Pattern::Empty);
                }
            }
        }
        match &self.image.expressions[expression as usize] {
            Expr::Tuple(parts) => Pattern::Tuple(
                parts
                    .iter()
                    .map(|part| self.structural_shape(*part, owner, depth + 1))
                    .collect(),
            ),
            Expr::Block { result, .. } | Expr::BindingValue { result, .. } => {
                self.structural_shape(*result, owner, depth + 1)
            }
            _ => Pattern::Any,
        }
    }

    /// Record all compatible writers, including currently absent guarded ones.
    /// Membership is checked again after refinement, so a newly matching writer
    /// wakes a reader even if every previously observed writer stayed unchanged.
    pub(super) fn observe_inputs(&self, reads: &BTreeSet<QueryKey>) -> Option<ObservedInputs> {
        if reads.is_empty() {
            return None;
        }
        let mut total = 0usize;
        let mut observation = vec![];
        for key in reads {
            let candidates = self
                .candidate_index
                .as_ref()
                .map(|index| index.candidates(key))
                .unwrap_or_else(|| (0..self.producers.len()).collect());
            let versions = candidates
                .into_iter()
                .filter(|id| {
                    self.producers[*id].footprint.matches(key)
                        && matches!(
                            self.producers[*id].effect,
                            Effect::Contribution(..) | Effect::Contributions(..)
                        )
                })
                .map(|id| (id, self.producer_versions[id]))
                .collect::<Vec<_>>();
            total = total.saturating_add(versions.len());
            // A broad dynamic family must not create an unbounded wakeup cache.
            // Falling back to evaluation preserves the existing execution contract.
            if total > 16_384 {
                return None;
            }
            observation.push(versions);
        }
        Some(observation)
    }

    pub(super) fn observed_bytes(&self) -> usize {
        self.observations
            .iter()
            .flatten()
            .map(observation_bytes)
            .sum()
    }

    pub(super) fn retain_observation(&self, reads: &BTreeSet<QueryKey>) -> Option<ObservedInputs> {
        #[cfg(test)]
        if self.cache_disabled || self.exact_only {
            return None;
        }
        let observation = self.observe_inputs(reads)?;
        let retained = self
            .observation_budget
            .get()
            .saturating_add(observation_bytes(&observation));
        if retained > 4 * 1024 * 1024 {
            return None;
        }
        self.observation_budget.set(retained);
        Some(observation)
    }

    pub(super) fn inputs_unchanged(&self, id: usize) -> bool {
        #[cfg(test)]
        if self.cache_disabled || self.exact_only {
            return false;
        }
        let Some(previous) = &self.observations[id] else {
            return false;
        };
        self.observe_inputs(&self.producers[id].reads).as_ref() == Some(previous)
    }
}

fn observation_bytes(value: &ObservedInputs) -> usize {
    24usize.saturating_add(
        value
            .iter()
            .map(|row| 24usize.saturating_add(row.len().saturating_mul(16)))
            .sum::<usize>(),
    )
}
