//! Pratt execution for abstract grammar families and recursive operators.

use super::*;

impl Runtime<'_> {
    pub(super) fn abstract_rule(
        &mut self,
        id: u32,
        bases: &[u32],
        operators: &[ImageOperator],
        position: usize,
        raw: bool,
        min_power: usize,
    ) -> Option<Match> {
        let mut left = None;
        // Prefix alternatives compete with bases. Their recursive edge has the level's binding power.
        for op in operators
            .iter()
            .filter(|o| !o.left && o.right && o.binding_power >= min_power)
        {
            if let Some(m) = self.operator(id, op, position, raw, None) {
                left = Some(m);
                break;
            }
        }
        if left.is_none() {
            for base in bases {
                if let Some(m) = self.rule(*base, position, raw, 0) {
                    left = Some(m);
                    break;
                }
            }
        }
        let mut left = left?;
        let mut nonassoc = None;
        loop {
            let mut next = None;
            for op in operators
                .iter()
                .filter(|o| o.left && o.binding_power >= min_power)
            {
                if nonassoc == Some(op.binding_power) {
                    continue;
                }
                if let Some(m) = self.continuation(left.end, raw, |parser| {
                    parser.operator(id, op, position, raw, Some(&left))
                }) {
                    if m.end > left.end {
                        next = Some((op, m));
                        break;
                    }
                }
            }
            let Some((op, matched)) = next else {
                break;
            };
            if op.associativity == Associativity::Nonassoc {
                nonassoc = Some(op.binding_power);
            }
            left = matched;
        }
        Some(left)
    }
    pub(super) fn operator(
        &mut self,
        family: u32,
        operator: &ImageOperator,
        start: usize,
        raw: bool,
        left: Option<&Match>,
    ) -> Option<Match> {
        let image = self.image;
        let rule = image.rules.get(operator.rule as usize)?;
        let ImageBody::Concrete(term) = &rule.body else {
            return None;
        };
        let min_power = if operator.associativity == Associativity::Nonassoc
            || (operator.left && operator.associativity != Associativity::Right)
        {
            operator.binding_power.checked_add(1)?
        } else {
            operator.binding_power
        };
        let context = OperatorContext {
            left: if operator.left {
                Some((operator.left_edge?, left?))
            } else {
                None
            },
            right: if operator.right {
                Some(operator.right_edge?)
            } else {
                None
            },
            family,
            min_power,
        };
        let mut result = self.term_with(*term, start, raw, Some(&context))?;
        if let Some(left) = left {
            result.diagnostics.splice(0..0, left.diagnostics.clone());
        }
        self.construct(&rule.definition, rule, start, result)
    }
}
