//! Explicit host-owned reusable type constraints. Automatic ownership is parked.
use super::*;
use std::collections::BTreeSet;

#[derive(Clone, PartialEq, Eq)]
struct Template {
    locals: BTreeSet<SemanticIdentity>,
}
struct Instance {
    template: SemanticIdentity,
    substitutions: BTreeMap<SemanticIdentity, SemanticIdentity>,
}
#[derive(Default)]
pub(super) struct Instances {
    templates: BTreeMap<SemanticIdentity, Template>,
    instances: BTreeMap<(SemanticIdentity, SemanticIdentity), Instance>,
    next: usize,
    copied_constraints: usize,
}

/// Register the identities owned by a reusable type template.
///
/// This experimental host interface deliberately requires explicit ownership.
/// All slots of each local identity are freshened together. Reads of identities
/// outside this set remain captured. Only directed type contributions and type
/// restrictions are cloned; native effects and meaning matches are not replayed.
/// Repeating an identical registration is idempotent. Changing ownership fails.
pub fn register_template(
    context: &mut OperationContext<'_>,
    root: SemanticIdentity,
    locals: &[SemanticIdentity],
) -> Result<(), Diagnostic> {
    let mut locals: BTreeSet<_> = locals.iter().cloned().collect();
    locals.insert(root.clone());
    if locals.iter().any(|identity| {
        matches!(identity,
        SemanticIdentity::Occurrence(id) if context.state.match_owners.contains(id))
    }) {
        return Err(Diagnostic::error(
            "types.template_match",
            "PARKED: Type templates cannot yet retain meaning match relationships",
            context.operation_span.clone(),
        ));
    }
    context
        .with_service::<Possibilities, _>(MODULE, |state, context| {
            let template = Template { locals };
            if let Some(existing) = state.instances.templates.get(&root) {
                if existing != &template {
                    return Err(Diagnostic::error(
                        "types.template_ownership",
                        "A registered type template cannot change its owned identities",
                        context.operation_span.clone(),
                    ));
                }
            } else {
                state.instances.templates.insert(root, template);
            }
            Ok(())
        })
        .unwrap_or_else(|| {
            Err(Diagnostic::error(
                "types.service",
                "Type service is unavailable",
                context.operation_span.clone(),
            ))
        })
}

impl Instances {
    fn fresh(&mut self, span: &Span) -> Result<SemanticIdentity, Diagnostic> {
        if self.next >= LIMIT {
            return Err(Diagnostic::error(
                "types.complexity",
                "Type instance identities exceeded the analysis limit",
                span.clone(),
            ));
        }
        let identity = SemanticIdentity::Service {
            service: MODULE.into(),
            id: self.next,
        };
        self.next += 1;
        Ok(identity)
    }
}

fn source_template(
    state: &Possibilities,
    identity: &SemanticIdentity,
    span: &Span,
) -> Result<Option<SemanticIdentity>, Diagnostic> {
    let mut seen = BTreeSet::new();
    let mut sources = BTreeSet::new();
    let mut queue = std::collections::VecDeque::from([identity.clone()]);
    while let Some(identity) = queue.pop_front() {
        if !seen.insert(identity.clone()) {
            continue;
        }
        if state.instances.templates.contains_key(&identity) {
            sources.insert(identity.clone());
            continue;
        }
        for transfer in &state.transfers {
            if let (Destination::One(target), Expr::Read(source, _)) =
                (&transfer.destination, &transfer.value)
            {
                if target.identity == identity && target.slot.is_none() && source.slot.is_none() {
                    queue.push_back(source.identity.clone());
                }
            }
        }
    }
    if sources.len() > 1 {
        return Err(Diagnostic::error(
            "types.template_ownership",
            "PARKED: Instantiation of multiple alternative templates is not defined",
            span.clone(),
        ));
    }
    Ok(sources.into_iter().next())
}

pub(super) struct Instantiate;
impl NativeOperation for Instantiate {
    fn evaluate(&mut self, context: &mut OperationContext<'_>) -> Result<Evaluation, Diagnostic> {
        let parse = |value: Option<&SemanticValue>| match value {
            Some(SemanticValue::Identity(identity)) => Ok(identity.clone()),
            _ => Err(Diagnostic::error(
                "types.key",
                "Instantiation requires template and use identities",
                context.operation_span.clone(),
            )),
        };
        let template = parse(context.argument("template"))?;
        let usage = parse(context.argument("use"))?;
        context
            .with_service::<Possibilities, _>(MODULE, |state, context| {
                let pair = (template.clone(), usage);
                if !state.instances.instances.contains_key(&pair) {
                    let Some(source) = source_template(state, &template, context.operation_span)?
                    else {
                        if !state.settled {
                            return Ok(Evaluation::Pending);
                        }
                        return Err(Diagnostic::error(
                            "types.template_ownership",
                            "No explicitly owned type template is registered for this value",
                            context.identity_span(&SemanticValue::Identity(template)),
                        ));
                    };
                    let owned = state.instances.templates[&source].locals.clone();
                    let mut substitutions = BTreeMap::new();
                    for local in owned {
                        substitutions.insert(local, state.instances.fresh(context.operation_span)?);
                    }
                    state.instances.instances.insert(
                        pair.clone(),
                        Instance {
                            template: source,
                            substitutions,
                        },
                    );
                }
                let instance = &state.instances.instances[&pair];
                Ok(Evaluation::Ready(SemanticValue::Identity(
                    instance.substitutions[&instance.template].clone(),
                )))
            })
            .unwrap_or_else(|| {
                Err(Diagnostic::error(
                    "types.service",
                    "Type service is unavailable",
                    context.operation_span.clone(),
                ))
            })
    }
}

impl Instance {
    fn key(&self, key: &Key) -> Key {
        Key {
            identity: self
                .substitutions
                .get(&key.identity)
                .unwrap_or(&key.identity)
                .clone(),
            slot: key.slot.clone(),
        }
    }
    fn destination(
        &self,
        target: &Destination,
        span: &Span,
    ) -> Result<Option<Destination>, Diagnostic> {
        let keys = Possibilities::destination_keys(target);
        let local_count = keys
            .iter()
            .filter(|key| self.substitutions.contains_key(&key.identity))
            .count();
        if local_count == 0 {
            return Ok(None);
        }
        if local_count != keys.len() {
            return Err(Diagnostic::error(
                "types.template_ownership",
                "PARKED: One positional constraint cannot write both local and captured identities",
                span.clone(),
            ));
        }
        Ok(Some(match target {
            Destination::One(key) => Destination::One(self.key(key)),
            Destination::Positions(keys) => {
                Destination::Positions(keys.iter().map(|key| self.key(key)).collect())
            }
        }))
    }
    fn expr(&self, expr: &Expr) -> Expr {
        match expr {
            Expr::Read(key, span) => Expr::Read(self.key(key), span.clone()),
            Expr::List(values, span) => Expr::List(
                values.iter().map(|value| self.expr(value)).collect(),
                span.clone(),
            ),
            Expr::Hypothetical(result, assumptions, span) => Expr::Hypothetical(
                Box::new(self.expr(result)),
                assumptions
                    .iter()
                    .map(|(destination, value)| {
                        let destination = match destination {
                            Destination::One(key) => Destination::One(self.key(key)),
                            Destination::Positions(keys) => Destination::Positions(
                                keys.iter().map(|key| self.key(key)).collect(),
                            ),
                        };
                        (destination, self.expr(value))
                    })
                    .collect(),
                span.clone(),
            ),
            other => other.clone(),
        }
    }
}

pub(super) fn refresh(state: &mut Possibilities, span: &Span) -> Result<bool, Diagnostic> {
    let mut transfers = vec![];
    let mut restrictions = vec![];
    for ((alias, _), instance) in &state.instances.instances {
        if source_template(state, alias, span)?.as_ref() != Some(&instance.template) {
            return Err(Diagnostic::error(
                "types.template_ownership",
                "A type template's source ownership cannot change after instantiation",
                span.clone(),
            ));
        }
        for transfer in &state.transfers {
            if let Some(destination) =
                instance.destination(&transfer.destination, &transfer.rule)?
            {
                let copy = Transfer {
                    destination,
                    value: instance.expr(&transfer.value),
                    span: transfer.span.clone(),
                    rule: transfer.rule.clone(),
                };
                if !state.transfers.contains(&copy) && !transfers.contains(&copy) {
                    transfers.push(copy);
                }
            }
        }
        for restriction in &state.restrictions {
            if let Some(destination) =
                instance.destination(&restriction.destination, &restriction.rule)?
            {
                let copy = Restriction {
                    destination,
                    value: instance.expr(&restriction.value),
                    span: restriction.span.clone(),
                    target_span: restriction.target_span.clone(),
                    rule: restriction.rule.clone(),
                };
                if !state.restrictions.contains(&copy) && !restrictions.contains(&copy) {
                    restrictions.push(copy);
                }
            }
        }
    }
    if state.instances.copied_constraints + transfers.len() + restrictions.len() > LIMIT {
        return Err(Diagnostic::error(
            "types.complexity",
            "Instantiated type constraints exceeded the analysis limit",
            span.clone(),
        ));
    }
    state.instances.copied_constraints += transfers.len() + restrictions.len();
    let changed = !transfers.is_empty() || !restrictions.is_empty();
    for transfer in transfers {
        state.register_destination(&transfer.destination);
        state.register_expr(&transfer.value);
        state.connect(&transfer.destination, &transfer.value, &transfer.span)?;
        state.transfers.push(transfer);
    }
    for restriction in restrictions {
        state.register_destination(&restriction.destination);
        state.register_expr(&restriction.value);
        state.restrictions.push(restriction);
    }
    Ok(changed)
}
