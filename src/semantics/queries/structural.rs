//! Structural discrimination over partial tuple keys, without prefix assumptions.
//!
//! Each tuple path has postings for its known scalar or tuple-arity values.
//! Missing constraints are wildcards. Union branches keep only constraints
//! common to every possible branch; correlations are checked by Pattern later.
use super::{Pattern, QueryKey};
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

type Path = Vec<usize>;
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Facet {
    Scalar(QueryKey),
    Tuple(usize),
}
type Constraints = BTreeMap<Path, BTreeSet<Facet>>;

#[derive(Clone, Default)]
struct Position {
    values: BTreeMap<Facet, Vec<usize>>,
    wildcard: Vec<usize>,
}

#[derive(Clone, Default)]
struct PositionIndex {
    positions: BTreeMap<Path, Position>,
    all: Vec<usize>,
    /// Accounted indexing metadata, excluding the required candidate ID roster.
    pub(super) bytes: usize,
}
impl PositionIndex {
    fn with_budget<'a>(
        patterns: impl IntoIterator<Item = (usize, &'a Pattern)>,
        budget: usize,
    ) -> Self {
        let mut index = Self::default();
        let mut constrained: BTreeMap<Path, BTreeSet<usize>> = BTreeMap::new();
        let mut exhausted = false;
        for (id, pattern) in patterns {
            if matches!(pattern, Pattern::Empty) {
                continue;
            }
            index.all.push(id);
            if exhausted {
                continue;
            }
            for (path, facets) in constraints(pattern) {
                let cost = 128usize
                    .saturating_add(path.len().saturating_mul(16))
                    .saturating_add(
                        facets
                            .iter()
                            .map(|facet| match facet {
                                Facet::Scalar(QueryKey::Text(value) | QueryKey::Symbol(value)) => {
                                    128usize.saturating_add(value.len().saturating_mul(2))
                                }
                                _ => 256,
                            })
                            .sum::<usize>(),
                    );
                index.bytes = index.bytes.saturating_add(cost);
                if index.bytes > budget {
                    index.positions.clear();
                    constrained.clear();
                    index.bytes = 0;
                    exhausted = true;
                    break;
                }
                constrained.entry(path.clone()).or_default().insert(id);
                let position = index.positions.entry(path).or_default();
                for facet in facets {
                    position.values.entry(facet).or_default().push(id);
                }
            }
        }
        let wildcard_bytes = constrained
            .values()
            .map(|known| {
                index
                    .all
                    .len()
                    .saturating_sub(known.len())
                    .saturating_mul(8)
            })
            .sum::<usize>();
        if index.bytes.saturating_add(wildcard_bytes) > budget {
            index.positions.clear();
            index.bytes = 0;
            return index;
        }
        index.bytes += wildcard_bytes;
        for (path, position) in &mut index.positions {
            let known = &constrained[path];
            position.wildcard = index
                .all
                .iter()
                .copied()
                .filter(|id| !known.contains(id))
                .collect();
        }
        index
    }

    /// Choose the most selective known position, then let the caller check the
    /// complete structural overlap. Unknown positions never exclude candidates.
    pub fn candidates(&self, pattern: &Pattern) -> Vec<usize> {
        if matches!(pattern, Pattern::Empty) {
            return vec![];
        }
        let known = constraints(pattern);
        let selected = known
            .iter()
            .filter_map(|(path, allowed)| {
                let position = self.positions.get(path)?;
                let size = position.wildcard.len()
                    + allowed
                        .iter()
                        .filter_map(|facet| position.values.get(facet))
                        .map(Vec::len)
                        .sum::<usize>();
                Some((size, position, allowed))
            })
            .min_by_key(|(size, _, _)| *size);
        let Some((_, position, allowed)) = selected else {
            return self.all.clone();
        };
        let mut ids = position.wildcard.clone();
        for facet in allowed {
            if let Some(matches) = position.values.get(facet) {
                ids.extend(matches);
            }
        }
        ids.sort_unstable();
        ids.dedup();
        ids
    }
}

/// Root shape partitions prevent wildcard positions in incompatible tuple
/// arities from drowning a selective later-position lookup. Within each shape,
/// discrimination remains independent of tuple field order.
#[derive(Clone, Default)]
pub(super) struct StructuralIndex {
    groups: BTreeMap<Facet, PositionIndex>,
    unknown: Vec<usize>,
    all: Vec<usize>,
    pub(super) bytes: usize,
}
impl StructuralIndex {
    pub fn new(patterns: impl IntoIterator<Item = (usize, Pattern)>) -> Self {
        Self::with_budget(patterns, 4 * 1024 * 1024)
    }
    fn with_budget(patterns: impl IntoIterator<Item = (usize, Pattern)>, budget: usize) -> Self {
        let mut result = Self::default();
        let mut grouped: BTreeMap<Facet, Vec<(usize, Rc<Pattern>)>> = BTreeMap::new();
        let mut exhausted = false;
        for (id, pattern) in patterns {
            if matches!(pattern, Pattern::Empty) {
                continue;
            }
            result.all.push(id);
            if exhausted {
                continue;
            }
            let roots = constraints(&pattern).remove(&vec![]);
            let Some(roots) = roots else {
                result.unknown.push(id);
                continue;
            };
            let pattern = Rc::new(pattern);
            for root in roots {
                let key_bytes = if grouped.contains_key(&root) {
                    0
                } else {
                    match &root {
                        Facet::Scalar(QueryKey::Text(value) | QueryKey::Symbol(value)) => {
                            128usize.saturating_add(value.len().saturating_mul(2))
                        }
                        _ => 256,
                    }
                };
                result.bytes = result.bytes.saturating_add(key_bytes).saturating_add(16);
                if result.bytes > budget {
                    grouped.clear();
                    result.unknown.clear();
                    result.bytes = 0;
                    exhausted = true;
                    break;
                }
                grouped.entry(root).or_default().push((id, pattern.clone()));
            }
        }
        if exhausted {
            return result;
        }
        for (root, patterns) in grouped {
            let group = PositionIndex::with_budget(
                patterns.iter().map(|(id, pattern)| (*id, pattern.as_ref())),
                budget.saturating_sub(result.bytes),
            );
            result.bytes = result.bytes.saturating_add(group.bytes);
            result.groups.insert(root, group);
        }
        result
    }
    pub fn candidates(&self, pattern: &Pattern) -> Vec<usize> {
        if matches!(pattern, Pattern::Empty) {
            return vec![];
        }
        let Some(roots) = constraints(pattern).remove(&vec![]) else {
            return self.all.clone();
        };
        // Empty groups mean the metadata budget was exhausted, not absence.
        if self.groups.is_empty() {
            return self.all.clone();
        }
        let mut result = self.unknown.clone();
        for root in roots {
            if let Some(group) = self.groups.get(&root) {
                result.extend(group.candidates(pattern));
            }
        }
        result.sort_unstable();
        result.dedup();
        result
    }
}

fn constraints(pattern: &Pattern) -> Constraints {
    fn exact(value: &QueryKey) -> Constraints {
        match value {
            QueryKey::Tuple(parts) => tuple(parts.iter().map(exact).collect()),
            value => BTreeMap::from([(vec![], BTreeSet::from([Facet::Scalar(value.clone())]))]),
        }
    }
    fn tuple(parts: Vec<Constraints>) -> Constraints {
        let mut result = BTreeMap::from([(vec![], BTreeSet::from([Facet::Tuple(parts.len())]))]);
        for (index, child) in parts.into_iter().enumerate() {
            for (path, allowed) in child {
                let mut nested = vec![index];
                nested.extend(path);
                result.insert(nested, allowed);
            }
        }
        result
    }
    match pattern {
        Pattern::Any | Pattern::Empty => BTreeMap::new(),
        Pattern::Exact(value) => exact(value),
        Pattern::Tuple(parts) => tuple(parts.iter().map(constraints).collect()),
        Pattern::Union(branches) => {
            let mut branches = branches
                .iter()
                .filter(|branch| !matches!(branch, Pattern::Empty));
            let Some(first) = branches.next() else {
                return BTreeMap::new();
            };
            let mut common = constraints(first);
            for branch in branches {
                let other = constraints(branch);
                common.retain(|path, allowed| {
                    if let Some(values) = other.get(path) {
                        allowed.extend(values.clone());
                        true
                    } else {
                        false
                    }
                });
            }
            common
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn tag(value: &str) -> Pattern {
        Pattern::Exact(QueryKey::Symbol(value.into()))
    }
    fn matching(patterns: &[Pattern], query: &Pattern) -> Vec<usize> {
        StructuralIndex::new(patterns.iter().cloned().enumerate())
            .candidates(query)
            .into_iter()
            .filter(|id| patterns[*id].overlaps(query))
            .collect()
    }
    #[test]
    fn exhausted_index_budget_keeps_every_possible_candidate() {
        let patterns = vec![(0, tag("first")), (1, tag("second"))];
        let index = StructuralIndex::with_budget(patterns, 1);
        assert_eq!(index.candidates(&tag("first")), vec![0, 1]);
        assert_eq!(index.bytes, 0);
    }

    #[test]
    fn holes_late_positions_nested_shapes_and_unions_are_conservative() {
        let patterns = vec![
            Pattern::Tuple(vec![Pattern::Any, tag("a")]),
            Pattern::Tuple(vec![Pattern::Any, tag("b")]),
            Pattern::Any,
            Pattern::Tuple(vec![
                Pattern::Any,
                Pattern::Tuple(vec![Pattern::Any, tag("a")]),
            ]),
            Pattern::Union(vec![
                Pattern::Tuple(vec![tag("left"), tag("a")]),
                Pattern::Tuple(vec![tag("right"), tag("b")]),
            ]),
        ];
        for query in [
            Pattern::Any,
            Pattern::Tuple(vec![Pattern::Any, tag("a")]),
            Pattern::Tuple(vec![
                Pattern::Any,
                Pattern::Tuple(vec![Pattern::Any, tag("b")]),
            ]),
            Pattern::Tuple(vec![tag("left"), tag("b")]),
            Pattern::Empty,
        ] {
            let expected = patterns
                .iter()
                .enumerate()
                .filter_map(|(id, value)| value.overlaps(&query).then_some(id))
                .collect::<Vec<_>>();
            assert_eq!(matching(&patterns, &query), expected);
        }
        let index = StructuralIndex::new(patterns.iter().cloned().enumerate());
        assert!(!index
            .candidates(&Pattern::Tuple(vec![Pattern::Any, tag("a")]))
            .contains(&1));
    }
}
