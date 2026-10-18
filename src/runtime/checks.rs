//! Syntax-local checks and structured capture-based condition evaluation.

use super::*;

impl Runtime<'_> {
    pub(super) fn checks(&mut self, rule: &ImageRule, start: usize, mut matched: Match) -> Match {
        for check in &rule.constraints {
            let satisfied = self.evaluate_condition(check.condition, &matched.fields)
                == ConditionValue::Bool(true);
            if satisfied {
                for emission in &check.emissions {
                    matched.diagnostics.push(Diagnostic {
                        severity: emission.severity.clone(),
                        code: "parse.constraint".into(),
                        message: emission.message.clone(),
                        primary: Box::new(Span::new(self.file, start, matched.end)),
                        secondary: vec![emission.span.clone()],
                        annotations: Box::default(),
                        help: None,
                    });
                }
            }
        }
        matched
    }
    fn evaluate_condition(
        &mut self,
        condition: u32,
        fields: &BTreeMap<String, Captured>,
    ) -> ConditionValue {
        let image = self.image;
        match &image.checks[condition as usize] {
            CheckExpression::Always | CheckExpression::CaptureType { .. } => {
                ConditionValue::Bool(true)
            }
            CheckExpression::Bool(value) => ConditionValue::Bool(*value),
            CheckExpression::Text(value) => ConditionValue::Text(value.clone()),
            CheckExpression::Absent => ConditionValue::Absent,
            CheckExpression::EnumValue { ty, variant } => {
                ConditionValue::Enum(ty.clone(), variant.clone())
            }
            CheckExpression::CaptureValue(name) => {
                fields
                    .get(name)
                    .map_or(ConditionValue::Absent, |f| match &f.value {
                        AstValue::Text(value) => ConditionValue::Text(value.clone()),
                        AstValue::None => ConditionValue::Absent,
                        _ => ConditionValue::PresentCapture,
                    })
            }
            CheckExpression::Present(name) => {
                ConditionValue::Bool(fields.get(name).is_some_and(|f| f.value != AstValue::None))
            }
            CheckExpression::Matches { capture, regex }
            | CheckExpression::OptionalMatches { capture, regex } => {
                let Some(field) = fields.get(capture).filter(|f| f.value != AstValue::None) else {
                    return ConditionValue::Absent;
                };
                let text = if let AstValue::Text(value) = &field.value {
                    value.clone()
                } else {
                    self.source
                        .get(field.span.start..field.span.end)
                        .unwrap_or("")
                        .into()
                };
                ConditionValue::Bool(self.regex(regex).is_some_and(|r| r.is_match(&text)))
            }
            CheckExpression::Not(inner) => match self.evaluate_condition(*inner, fields) {
                ConditionValue::Bool(v) => ConditionValue::Bool(!v),
                _ => ConditionValue::Absent,
            },
            CheckExpression::And(a, b) => {
                let left = self.evaluate_condition(*a, fields);
                if left == ConditionValue::Bool(true) {
                    self.evaluate_condition(*b, fields)
                } else {
                    left
                }
            }
            CheckExpression::Or(a, b) => {
                let left = self.evaluate_condition(*a, fields);
                if left == ConditionValue::Bool(true) {
                    left
                } else {
                    self.evaluate_condition(*b, fields)
                }
            }
            CheckExpression::Equal(a, b) => {
                let a = self.evaluate_condition(*a, fields);
                let b = self.evaluate_condition(*b, fields);
                ConditionValue::Bool(a == b)
            }
            CheckExpression::Between {
                trivia,
                left,
                right,
            } => {
                let (Some(a), Some(b)) = (fields.get(left), fields.get(right)) else {
                    return ConditionValue::Absent;
                };
                let (start, end) = (a.span.end, b.span.start);
                let found = if start >= end {
                    false
                } else if let Some(id) = trivia {
                    (start..end)
                        .filter(|p| self.source.is_char_boundary(*p))
                        .any(|offset| {
                            self.rule(*id, offset, true, 0)
                                .is_some_and(|m| m.end <= end && m.end > offset)
                        })
                } else {
                    self.skip(start) > start
                };
                ConditionValue::Bool(found)
            }
        }
    }
}
