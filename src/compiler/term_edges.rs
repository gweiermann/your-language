//! Grammar edge inspection shared with runtime precedence parsing.

use super::*;

pub(crate) fn edge_term(term: &Term, right: bool) -> Option<&Term> {
    match term {
        Term::Ref(_) => Some(term),
        Term::Capture(_, term)
        | Term::Meaning { term, .. }
        | Term::Example { term, .. }
        | Term::Mark { term, .. }
        | Term::Project { term, .. } => edge_term(term, right),
        Term::Sequence(items) => {
            let mut candidates = items.iter().filter(|t| !zero_width(t));
            let edge = if right {
                candidates.next_back()
            } else {
                candidates.next()
            }?;
            edge_term(edge, right)
        }
        _ => None,
    }
}
pub(super) fn zero_width(term: &Term) -> bool {
    match term {
        Term::NotAhead(_) | Term::NotBehind(_) => true,
        Term::Literal(text) => text.is_empty(),
        Term::Capture(_, term)
        | Term::Meaning { term, .. }
        | Term::Example { term, .. }
        | Term::Mark { term, .. }
        | Term::Project { term, .. } => zero_width(term),
        Term::Sequence(items) => items.iter().all(zero_width),
        _ => false,
    }
}
pub(super) fn edge_ref(term: &Term, right: bool) -> Option<&str> {
    match edge_term(term, right)? {
        Term::Ref(id) => Some(id),
        _ => None,
    }
}
