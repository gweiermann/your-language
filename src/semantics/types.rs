//! Directed possible-type contributions and independent universal restrictions.
//! Identities and slot symbols are opaque; target-language policy lives in YL.
use super::*;
mod hypothetical;
mod instantiation;
pub use instantiation::register_template;
const MODULE: &str = "std/types";
const UNKNOWN: &str = "std/types#unknown";
const LIMIT: usize = 4096;
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Key {
    identity: SemanticIdentity,
    slot: Option<String>,
}
/// Convert original syntax values to opaque type addresses at the library seam.
fn syntax_key(value: &SemanticValue) -> SemanticValue {
    match value {
        SemanticValue::Source(source) if source.value == AstValue::None => SemanticValue::Absent,
        SemanticValue::Source(source) if source.repeated => SemanticValue::List(
            source
                .occurrences
                .iter()
                .map(|id| SemanticValue::Identity(SemanticIdentity::Occurrence(*id)))
                .collect(),
        ),
        SemanticValue::Source(source) if source.occurrences.len() == 1 => {
            SemanticValue::Identity(SemanticIdentity::Occurrence(source.occurrences[0]))
        }
        SemanticValue::Tuple(values) => {
            SemanticValue::Tuple(values.iter().map(syntax_key).collect())
        }
        SemanticValue::List(values) => SemanticValue::List(values.iter().map(syntax_key).collect()),
        value => value.clone(),
    }
}
fn key(value: &SemanticValue, span: &Span) -> Result<Key, Diagnostic> {
    let value = syntax_key(value);
    match &value {
        SemanticValue::Identity(identity) => Ok(Key {
            identity: identity.clone(),
            slot: None,
        }),
        SemanticValue::Tuple(values) if values.len() == 2 => match (&values[0], &values[1]) {
            (SemanticValue::Identity(identity), SemanticValue::Symbol(slot)) => Ok(Key {
                identity: identity.clone(),
                slot: Some(slot.clone()),
            }),
            _ => Err(Diagnostic::error(
                "types.key",
                "A type slot is (identity, symbol)",
                span.clone(),
            )),
        },
        _ => Err(Diagnostic::error(
            "types.key",
            "Expected an identity or (identity, symbol) type key",
            span.clone(),
        )),
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
enum Destination {
    One(Key),
    Positions(Vec<Key>),
}
fn destination(value: &SemanticValue, span: &Span) -> Result<Destination, Diagnostic> {
    let value = syntax_key(value);
    if let SemanticValue::List(values) = &value {
        Ok(Destination::Positions(
            values
                .iter()
                .map(|v| key(v, span))
                .collect::<Result<_, _>>()?,
        ))
    } else {
        Ok(Destination::One(key(&value, span)?))
    }
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Member {
    Atom(String),
    Unknown,
    List(Vec<Member>),
}
impl Member {
    fn value(&self) -> SemanticValue {
        match self {
            Self::Atom(s) => SemanticValue::Symbol(s.clone()),
            Self::Unknown => SemanticValue::Symbol(UNKNOWN.into()),
            Self::List(xs) => SemanticValue::List(xs.iter().map(Self::value).collect()),
        }
    }
    fn display(&self) -> String {
        match self {
            Self::Atom(s) => s.rsplit('#').next().unwrap_or(s).into(),
            Self::Unknown => "unknown".into(),
            Self::List(xs) => format!(
                "[{}]",
                xs.iter().map(Self::display).collect::<Vec<_>>().join(", ")
            ),
        }
    }
    fn compatible(&self, allowed: &Self) -> bool {
        match (self, allowed) {
            (Self::Unknown, _) | (_, Self::Unknown) => true,
            (Self::List(a), Self::List(b)) if a.len() == b.len() => {
                a.iter().zip(b).all(|(a, b)| a.compatible(b))
            }
            _ => self == allowed,
        }
    }
}
#[derive(Clone, Debug)]
struct Fact {
    implicit_unknown: bool,
    member: Member,
    origin: Span,
    anchors: Vec<Key>,
    children: Vec<Fact>,
}
impl Fact {
    fn scalar(member: Member, origin: Span) -> Self {
        Self {
            implicit_unknown: false,
            member,
            origin,
            anchors: vec![],
            children: vec![],
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
enum Expr {
    Constant(Member, Span),
    Read(Key, Span),
    List(Vec<Expr>, Span),
    Empty,
    Hypothetical(Box<Expr>, Vec<(Destination, Expr)>, Span),
}
impl Expr {
    fn parse(value: &SemanticValue, origin: &Span) -> Result<Self, Diagnostic> {
        match value {
            SemanticValue::Symbol(s) => Ok(Self::Constant(
                if s == UNKNOWN {
                    Member::Unknown
                } else {
                    Member::Atom(s.clone())
                },
                origin.clone(),
            )),
            SemanticValue::Absent => Ok(Self::Empty),
            SemanticValue::Deferred {
                service,
                key: value,
                span,
            } if service == MODULE => {
                if let Some(query) = hypothetical::parse(value, span)? {
                    Ok(query)
                } else {
                    Self::read(value, span)
                }
            }
            SemanticValue::List(xs) => Ok(Self::List(
                xs.iter()
                    .map(|v| Self::parse(v, origin))
                    .collect::<Result<_, _>>()?,
                origin.clone(),
            )),
            _ => Err(Diagnostic::error(
                "types.value",
                "Expected a type symbol, deferred type, or list of types",
                origin.clone(),
            )),
        }
    }
    fn read(value: &SemanticValue, span: &Span) -> Result<Self, Diagnostic> {
        let value = syntax_key(value);
        match &value {
            SemanticValue::Absent => Ok(Self::Empty),
            SemanticValue::List(xs) => Ok(Self::List(
                xs.iter()
                    .map(|v| Self::read(v, span))
                    .collect::<Result<_, _>>()?,
                span.clone(),
            )),
            _ => Ok(Self::Read(key(&value, span)?, span.clone())),
        }
    }
    fn dependencies(&self, depth: i32, output: &mut Vec<(Key, i32)>) {
        match self {
            Self::Read(k, _) => output.push((k.clone(), depth)),
            Self::List(xs, _) => {
                for x in xs {
                    x.dependencies(depth + 1, output)
                }
            }
            Self::Hypothetical(result, assumptions, _) => {
                result.dependencies(depth, output);
                for (_, value) in assumptions {
                    value.dependencies(depth, output);
                }
            }
            _ => {}
        }
    }
}
#[derive(Clone, PartialEq, Eq)]
struct Transfer {
    destination: Destination,
    value: Expr,
    span: Span,
    rule: Span,
}
#[derive(Clone, PartialEq, Eq)]
struct Restriction {
    destination: Destination,
    value: Expr,
    span: Span,
    target_span: Span,
    rule: Span,
}
type HypotheticalFrame = (Expr, Vec<(Destination, Vec<Member>)>);

#[derive(Default)]
struct Possibilities {
    facts: BTreeMap<Key, BTreeMap<Member, Fact>>,
    transfers: Vec<Transfer>,
    restrictions: Vec<Restriction>,
    forwarding: std::collections::BTreeSet<(Key, Key)>,
    reported: std::collections::BTreeSet<(usize, Key, Member)>,
    edges: std::collections::BTreeSet<(Key, Key, i32)>,
    settled: bool,
    instances: instantiation::Instances,
    hypothetical_depth: usize,
    hypothetical_stack: Vec<HypotheticalFrame>,
    residual_matches: Vec<hypothetical::TypeMatch>,
    query_errors: BTreeMap<usize, Diagnostic>,
}
impl Possibilities {
    fn connect(
        &mut self,
        target: &Destination,
        expr: &Expr,
        span: &Span,
    ) -> Result<(), Diagnostic> {
        if let (Destination::One(target), Expr::Read(source, _)) = (target, expr) {
            if source.slot.is_none() && target.slot.is_none() && source != target {
                self.forwarding.insert((source.clone(), target.clone()));
            }
        }
        let mut dependencies = vec![];
        expr.dependencies(0, &mut dependencies);
        let destinations = match target {
            Destination::One(key) => vec![(key.clone(), 0)],
            Destination::Positions(keys) => keys.iter().map(|key| (key.clone(), -1)).collect(),
        };
        for (source, depth) in dependencies {
            for (destination, projection) in &destinations {
                self.add_edge(&source, destination, depth + projection, span)?;
            }
        }
        Ok(())
    }
    fn add_edge(
        &mut self,
        source: &Key,
        destination: &Key,
        weight: i32,
        span: &Span,
    ) -> Result<(), Diagnostic> {
        if self
            .edges
            .contains(&(source.clone(), destination.clone(), weight))
        {
            return Ok(());
        }
        let mut distances = BTreeMap::from([(destination.clone(), 0)]);
        let keys = self
            .edges
            .len()
            .saturating_mul(2)
            .saturating_add(self.facts.len())
            .saturating_add(1);
        for _ in 0..keys {
            if distances
                .get(source)
                .is_some_and(|distance| distance + weight > 0)
            {
                return Err(Diagnostic::error(
                    "types.mismatch",
                    "Recursive contribution requires an infinite type",
                    span.clone(),
                ));
            }
            let mut changed = false;
            for (from, to, step) in &self.edges {
                if let Some(distance) = distances.get(from).copied() {
                    let candidate = distance + step;
                    if distances.get(to).is_none_or(|old| candidate > *old) {
                        distances.insert(to.clone(), candidate);
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }
        self.edges
            .insert((source.clone(), destination.clone(), weight));
        Ok(())
    }
    // Combine allowed alternatives by structural intersection, with unknown
    // serving as the permissive wildcard rather than an extra union member.
    fn meet_fact(left: &Fact, right: &Fact) -> Option<Fact> {
        match (&left.member, &right.member) {
            (Member::Unknown, _) => Some(right.clone()),
            (_, Member::Unknown) => Some(left.clone()),
            (Member::List(a), Member::List(b)) if a.len() == b.len() => {
                let children: Option<Vec<_>> = left
                    .children
                    .iter()
                    .zip(&right.children)
                    .map(|(a, b)| Self::meet_fact(a, b))
                    .collect();
                let children = children?;
                Some(Fact {
                    implicit_unknown: false,
                    member: Member::List(children.iter().map(|c| c.member.clone()).collect()),
                    origin: right.origin.clone(),
                    anchors: vec![],
                    children,
                })
            }
            _ if left.member == right.member => Some(left.clone()),
            _ => None,
        }
    }
    fn destination_keys(destination: &Destination) -> Vec<Key> {
        match destination {
            Destination::One(key) => vec![key.clone()],
            Destination::Positions(keys) => keys.clone(),
        }
    }
    // Join connected restrictions on canonical keys. Keeping whole rows retains
    // correlations even when positional key lists overlap or change order.
    fn contract_rows(
        &self,
        keys: &[Key],
        seen: &std::collections::BTreeSet<Key>,
    ) -> Result<Option<Vec<BTreeMap<Key, Fact>>>, Diagnostic> {
        let mut component: std::collections::BTreeSet<_> = keys.iter().cloned().collect();
        let mut selected = vec![];
        loop {
            let mut added = false;
            for (index, restriction) in self.restrictions.iter().enumerate() {
                if selected.contains(&index) {
                    continue;
                }
                let targets = Self::destination_keys(&restriction.destination);
                if targets.iter().any(|key| component.contains(key)) {
                    component.extend(targets);
                    selected.push(index);
                    added = true;
                }
            }
            if !added {
                break;
            }
        }
        if selected.is_empty() {
            return Ok(None);
        }
        let mut nested = seen.clone();
        nested.extend(component);
        let mut rows = vec![BTreeMap::<Key, Fact>::new()];
        let mut has_contract = false;
        for index in selected {
            let bound = &self.restrictions[index];
            let values = self.eval(&bound.value, &mut nested)?;
            if values.is_empty() {
                continue;
            }
            has_contract = true;
            let mut choices = vec![];
            for fact in values {
                let assignments = match &bound.destination {
                    Destination::One(key) => vec![(key.clone(), fact)],
                    Destination::Positions(keys) if fact.member == Member::Unknown => {
                        keys.iter().map(|key| (key.clone(), fact.clone())).collect()
                    }
                    Destination::Positions(keys) if keys.len() == fact.children.len() => {
                        keys.iter().cloned().zip(fact.children).collect()
                    }
                    _ => continue,
                };
                let mut row = BTreeMap::<Key, Fact>::new();
                let mut valid = true;
                for (key, fact) in assignments {
                    if let Some(old) = row.get(&key) {
                        if let Some(meet) = Self::meet_fact(old, &fact) {
                            row.insert(key, meet);
                        } else {
                            valid = false;
                            break;
                        }
                    } else {
                        row.insert(key, fact);
                    }
                }
                if valid {
                    choices.push(row);
                }
            }
            let mut joined = BTreeMap::new();
            for row in &rows {
                for choice in &choices {
                    let mut next = row.clone();
                    let mut valid = true;
                    for (key, fact) in choice {
                        if let Some(old) = next.get(key) {
                            if let Some(meet) = Self::meet_fact(old, fact) {
                                next.insert(key.clone(), meet);
                            } else {
                                valid = false;
                                break;
                            }
                        } else {
                            next.insert(key.clone(), fact.clone());
                        }
                    }
                    if valid {
                        let identity: Vec<_> = next
                            .iter()
                            .map(|(key, fact)| (key.clone(), fact.member.clone()))
                            .collect();
                        joined.entry(identity).or_insert(next);
                        if joined.len() > LIMIT {
                            return Err(Diagnostic::error(
                                "types.complexity",
                                "Restriction combinations exceeded the analysis limit",
                                bound.span.clone(),
                            ));
                        }
                    }
                }
            }
            rows = joined.into_values().collect();
            if rows.is_empty() {
                break;
            }
        }
        Ok(has_contract.then_some(rows))
    }
    fn register_expr(&mut self, value: &Expr) {
        let mut deps = vec![];
        value.dependencies(0, &mut deps);
        for (key, _) in deps {
            self.facts.entry(key).or_default();
        }
    }
    fn register_destination(&mut self, target: &Destination) {
        match target {
            Destination::One(k) => {
                self.facts.entry(k.clone()).or_default();
            }
            Destination::Positions(ks) => {
                for k in ks {
                    self.facts.entry(k.clone()).or_default();
                }
            }
        }
    }
    fn eval(
        &self,
        expr: &Expr,
        seen: &mut std::collections::BTreeSet<Key>,
    ) -> Result<Vec<Fact>, Diagnostic> {
        match expr {
            Expr::Hypothetical(result, assumptions, span) => {
                hypothetical::evaluate(self, result, assumptions, span)
            }
            Expr::Empty => Ok(vec![]),
            Expr::Constant(m, s) => Ok(vec![Fact::scalar(m.clone(), s.clone())]),
            Expr::Read(k, s) => {
                if !seen.insert(k.clone()) {
                    // A recursive bound may read finite contributed evidence;
                    // only an unobserved fallback would reenter the contracts.
                    return Ok(self
                        .facts
                        .get(k)
                        .map(|facts| facts.values().cloned().collect())
                        .unwrap_or_default());
                }
                let mut result = self
                    .facts
                    .get(k)
                    .map(|fs| fs.values().cloned().collect::<Vec<_>>())
                    .unwrap_or_default();
                if result.is_empty() {
                    if let Some(rows) = self.contract_rows(std::slice::from_ref(k), seen)? {
                        result = rows
                            .into_iter()
                            .filter_map(|mut row| row.remove(k))
                            .collect();
                    }
                }
                if result.is_empty() && self.settled {
                    let mut unknown = Fact::scalar(Member::Unknown, s.clone());
                    unknown.implicit_unknown = true;
                    result.push(unknown);
                }
                // An annotation supplies a contract-derived value, not the
                // token that used that value. Rebase only such binding reads
                // onto their use; actual initializer origins remain intact.
                if matches!(k.identity, SemanticIdentity::Binding(_)) {
                    let mut contracts = vec![];
                    for bound in &self.restrictions {
                        if matches!(&bound.destination, Destination::One(target) if target == k) {
                            contracts.extend(self.eval(&bound.value, seen)?);
                        }
                    }
                    for fact in &mut result {
                        if contracts.iter().any(|contract| {
                            contract.member == fact.member && contract.origin == fact.origin
                        }) {
                            fact.origin = s.clone();
                        }
                    }
                }
                for fact in &mut result {
                    if !fact.anchors.contains(k) {
                        fact.anchors.push(k.clone());
                    }
                }
                seen.remove(k);
                Ok(result)
            }
            Expr::List(xs, s) => {
                let keys: Option<Vec<_>> = xs
                    .iter()
                    .map(|expr| match expr {
                        Expr::Read(key, _) => Some(key.clone()),
                        _ => None,
                    })
                    .collect();
                if let Some(keys) = keys.filter(|keys| {
                    keys.iter().all(|key| !seen.contains(key))
                        && keys
                            .iter()
                            .any(|key| self.facts.get(key).is_none_or(BTreeMap::is_empty))
                }) {
                    if let Some(rows) = self.contract_rows(&keys, seen)? {
                        let mut contracts = vec![];
                        for row in rows {
                            let mut product = vec![vec![]];
                            for key in &keys {
                                let choices =
                                    if self.facts.get(key).is_some_and(|facts| !facts.is_empty()) {
                                        self.eval(&Expr::Read(key.clone(), s.clone()), seen)?
                                            .into_iter()
                                            .filter(|fact| {
                                                row.get(key).is_none_or(|bound| {
                                                    fact.member.compatible(&bound.member)
                                                })
                                            })
                                            .collect()
                                    } else {
                                        match row.get(key) {
                                            Some(fact) => vec![fact.clone()],
                                            None => self
                                                .eval(&Expr::Read(key.clone(), s.clone()), seen)?,
                                        }
                                    };
                                let mut next = vec![];
                                for prefix in &product {
                                    for choice in &choices {
                                        if contracts.len() + next.len() >= LIMIT {
                                            return Err(Diagnostic::error("types.complexity", "Restriction combinations exceeded the analysis limit", s.clone()));
                                        }
                                        let mut children = prefix.clone();
                                        children.push(choice.clone());
                                        next.push(children);
                                    }
                                }
                                product = next;
                            }
                            contracts.extend(product.into_iter().map(|children: Vec<Fact>| Fact {
                                implicit_unknown: false,
                                member: Member::List(
                                    children.iter().map(|c| c.member.clone()).collect(),
                                ),
                                origin: s.clone(),
                                anchors: vec![],
                                children,
                            }));
                        }
                        return Ok(contracts);
                    }
                }
                let mut product: Vec<Vec<Fact>> = vec![vec![]];
                for x in xs {
                    let choices = self.eval(x, seen)?;
                    let mut next = vec![];
                    for prefix in product {
                        for choice in &choices {
                            if next.len() >= LIMIT {
                                return Err(Diagnostic::error(
                                    "types.complexity",
                                    "Type combinations exceeded the analysis limit",
                                    s.clone(),
                                ));
                            }
                            let mut row = prefix.clone();
                            row.push(choice.clone());
                            next.push(row);
                        }
                    }
                    product = next;
                }
                Ok(product
                    .into_iter()
                    .map(|children| Fact {
                        implicit_unknown: false,
                        member: Member::List(children.iter().map(|c| c.member.clone()).collect()),
                        origin: s.clone(),
                        anchors: vec![],
                        children,
                    })
                    .collect())
            }
        }
    }
    fn insert(&mut self, k: &Key, mut fact: Fact) -> Result<bool, Diagnostic> {
        if fact.implicit_unknown {
            return Ok(false);
        }
        if let Some(slot) = &k.slot {
            if fact.children.iter().any(|child| {
                child
                    .anchors
                    .iter()
                    .any(|a| a.identity == k.identity && a.slot.is_none())
            }) {
                return Err(Diagnostic::error(
                    "types.mismatch",
                    format!("Recursive slot {slot} requires an infinite type"),
                    fact.origin.clone(),
                ));
            }
        }
        let mut changed = false;
        for source in &fact.anchors {
            if source.slot.is_none() && k.slot.is_none() && source != k {
                changed |= self.forwarding.insert((source.clone(), k.clone()));
            }
        }
        let set = self.facts.entry(k.clone()).or_default();
        if let Some(existing) = set.get_mut(&fact.member) {
            for anchor in fact.anchors.drain(..) {
                if !existing.anchors.contains(&anchor) {
                    existing.anchors.push(anchor);
                    changed = true;
                }
            }
            for (left, right) in existing.children.iter_mut().zip(fact.children) {
                for anchor in right.anchors {
                    if !left.anchors.contains(&anchor) {
                        left.anchors.push(anchor);
                        changed = true;
                    }
                }
            }
        } else {
            if set.len() >= LIMIT {
                return Err(Diagnostic::error(
                    "types.complexity",
                    "Type possibilities exceeded the analysis limit",
                    fact.origin,
                ));
            }
            set.insert(fact.member.clone(), fact);
            changed = true;
        }
        Ok(changed)
    }
    fn contribute(&mut self, target: &Destination, fact: Fact) -> Result<bool, Diagnostic> {
        // Settlement defaults describe missing evidence, not reusable facts.
        if fact.implicit_unknown {
            return Ok(false);
        }
        match target {
            Destination::One(k) => self.insert(k, fact),
            Destination::Positions(ks) => {
                if fact.member == Member::Unknown {
                    return Ok(false);
                }
                if fact.children.len() != ks.len() || !matches!(fact.member, Member::List(_)) {
                    return Err(Diagnostic::error(
                        "types.mismatch",
                        "Type list does not match the target key list's arity",
                        fact.origin,
                    ));
                }
                let mut changed = false;
                for (k, child) in ks.iter().zip(fact.children) {
                    changed |= self.insert(k, child)?;
                }
                Ok(changed)
            }
        }
    }
    fn report_restriction(
        &mut self,
        index: usize,
        k: &Key,
        fact: &Fact,
        allowed: &[Fact],
        context: &mut OperationContext<'_>,
    ) {
        if fact.member == Member::Unknown
            || allowed.is_empty()
            || allowed.iter().any(|a| fact.member.compatible(&a.member))
        {
            return;
        }
        if !self
            .reported
            .insert((index, k.clone(), fact.member.clone()))
        {
            return;
        }
        let expected = allowed
            .iter()
            .map(|a| a.member.display())
            .collect::<Vec<_>>()
            .join(" | ");
        let origin = fact
            .children
            .iter()
            .enumerate()
            .find(|(index, child)| {
                !allowed.iter().any(|bound| {
                    bound.member == Member::Unknown
                        || bound.children.get(*index).is_some_and(|allowed_child| {
                            child.member.compatible(&allowed_child.member)
                        })
                })
            })
            .map(|(_, child)| child.origin.clone())
            .unwrap_or_else(|| fact.origin.clone());
        let mut diagnostic = Diagnostic::error(
            "types.mismatch",
            format!("Type {} does not match {expected}", fact.member.display()),
            origin.clone(),
        );
        if let Some(bound) = allowed.first() {
            if bound.origin != origin {
                diagnostic.secondary.push(bound.origin.clone());
                diagnostic.help =
                    Some("The related location requires the incompatible type".into());
            }
        }
        context.emit(diagnostic);
    }
    fn propagate_slots(&mut self) -> Result<bool, Diagnostic> {
        let mut changed = false;
        for (source, target) in self.forwarding.clone() {
            // A copied value retains its authored structural slot handles. Base
            // possibilities flow one way; slot contributions address the same
            // structure through either reference, without merging base facts.
            let slots: std::collections::BTreeSet<_> = self
                .facts
                .keys()
                .filter(|key| key.identity == source.identity || key.identity == target.identity)
                .filter_map(|key| key.slot.clone())
                .collect();
            for slot in slots {
                let source_slot = Key {
                    identity: source.identity.clone(),
                    slot: Some(slot.clone()),
                };
                let target_slot = Key {
                    identity: target.identity.clone(),
                    slot: Some(slot),
                };
                self.facts.entry(source_slot.clone()).or_default();
                self.facts.entry(target_slot.clone()).or_default();
                let origin = self
                    .facts
                    .get(&source_slot)
                    .and_then(|facts| facts.values().next())
                    .or_else(|| {
                        self.facts
                            .get(&target_slot)
                            .and_then(|facts| facts.values().next())
                    })
                    .map(|fact| fact.origin.clone())
                    .unwrap_or_default();
                self.add_edge(&source_slot, &target_slot, 0, &origin)?;
                self.add_edge(&target_slot, &source_slot, 0, &origin)?;
                for fact in self.snapshot(&source_slot)? {
                    changed |= self.insert(&target_slot, fact)?;
                }
                for fact in self.snapshot(&target_slot)? {
                    changed |= self.insert(&source_slot, fact)?;
                }
            }
        }
        Ok(changed)
    }
    fn snapshot(&self, k: &Key) -> Result<Vec<Fact>, Diagnostic> {
        self.eval(
            &Expr::Read(k.clone(), Span::default()),
            &mut Default::default(),
        )
    }
    fn value(facts: &[Fact]) -> SemanticValue {
        let values: std::collections::BTreeSet<_> =
            facts.iter().map(|f| f.member.clone()).collect();
        if values.len() == 1 {
            return values.iter().next().unwrap().value();
        }
        SemanticValue::Alternatives {
            service: MODULE.into(),
            values: values.iter().map(Member::value).collect(),
        }
    }
    fn display(facts: &[Fact]) -> String {
        let values: std::collections::BTreeSet<_> =
            facts.iter().map(|f| f.member.display()).collect();
        values.into_iter().collect::<Vec<_>>().join(" | ")
    }
}
impl AnalysisService for Possibilities {
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn retain_match(&mut self, matching: &ResidualMatch) -> Result<(), Diagnostic> {
        hypothetical::retain(self, matching)
    }
    fn offers_alternatives(&self) -> bool {
        true
    }
    fn resolve_value(
        &mut self,
        value: &SemanticValue,
        context: &mut OperationContext<'_>,
    ) -> Result<Evaluation, Diagnostic> {
        let expr = if let Some(query) = hypothetical::parse(value, context.operation_span)? {
            query
        } else {
            Expr::read(value, context.operation_span)?
        };
        self.register_expr(&expr);
        let facts = self.eval(&expr, &mut Default::default())?;
        if facts.is_empty() {
            return Ok(Evaluation::Pending);
        }
        let values: std::collections::BTreeSet<_> =
            facts.iter().map(|f| f.member.clone()).collect();
        Ok(Evaluation::Ready(SemanticValue::Alternatives {
            service: MODULE.into(),
            values: values.iter().map(Member::value).collect(),
        }))
    }
    fn progress(&mut self, context: &mut OperationContext<'_>) -> Result<bool, Diagnostic> {
        let mut changed = instantiation::refresh(self, context.operation_span)?;
        for (index, transfer) in self.transfers.clone().into_iter().enumerate() {
            let facts = match self.eval(&transfer.value, &mut Default::default()) {
                Ok(facts) => {
                    self.query_errors.remove(&index);
                    facts
                }
                Err(diagnostic)
                    if matches!(
                        diagnostic.code.as_str(),
                        "types.mismatch"
                            | "types.hypothetical_effect"
                            | "types.hypothetical_cycle"
                            | "semantic.meaning_case"
                    ) =>
                {
                    self.query_errors.insert(index, diagnostic);
                    continue;
                }
                Err(mut diagnostic) => {
                    if diagnostic.primary.file.is_empty() {
                        diagnostic.primary = Box::new(transfer.span.clone());
                    }
                    return Err(diagnostic);
                }
            };
            for fact in facts {
                changed |= self.contribute(&transfer.destination, fact)?;
            }
        }
        changed |= self.propagate_slots()?;
        // Restrictions are checked only at finalization: future alternatives can
        // legitimately enlarge an allowed set. Facts are never deleted.
        let _ = context;
        Ok(changed)
    }
    fn settle(&mut self, _context: &mut OperationContext<'_>) -> Result<(), Diagnostic> {
        self.settled = true;
        Ok(())
    }
    fn finish(&mut self, context: &mut OperationContext<'_>) -> Result<(), Diagnostic> {
        self.settled = true;
        let mut query_errors = vec![];
        for error in self.query_errors.values() {
            if !query_errors.contains(error) {
                query_errors.push(error.clone());
            }
        }
        for error in query_errors {
            context.emit(error);
        }
        let mut checked_domains = std::collections::BTreeSet::new();
        for (index, bound) in self.restrictions.clone().into_iter().enumerate() {
            let mut allowed = self.eval(&bound.value, &mut Default::default())?;
            for fact in &mut allowed {
                if fact.origin.file.is_empty() {
                    fact.origin = bound.span.clone();
                }
            }
            let keys = Self::destination_keys(&bound.destination);
            if keys.iter().any(|key| !checked_domains.contains(key))
                && self
                    .contract_rows(&keys, &Default::default())?
                    .is_some_and(|rows| rows.is_empty())
            {
                checked_domains.extend(keys.clone());
                let primary = allowed
                    .first()
                    .map(|fact| fact.origin.clone())
                    .unwrap_or_else(|| bound.span.clone());
                let mut diagnostic = Diagnostic::error(
                    "types.mismatch",
                    "Type restrictions have no compatible possibility",
                    primary.clone(),
                );
                for other in &self.restrictions {
                    if other != &bound
                        && Self::destination_keys(&other.destination)
                            .iter()
                            .any(|key| keys.contains(key))
                    {
                        if let Some(fact) =
                            self.eval(&other.value, &mut Default::default())?.first()
                        {
                            if fact.origin != primary
                                && !diagnostic.secondary.contains(&fact.origin)
                            {
                                diagnostic.secondary.push(fact.origin.clone());
                            }
                        }
                    }
                }
                diagnostic.help =
                    Some("The related restrictions require incompatible types".into());
                context.emit(diagnostic);
            }
            match bound.destination {
                Destination::One(k) => {
                    for fact in self.facts.get(&k).cloned().unwrap_or_default().values() {
                        self.report_restriction(index, &k, fact, &allowed, context)
                    }
                }
                Destination::Positions(keys) => {
                    let value = Expr::List(
                        keys.iter()
                            .map(|key| {
                                if self.facts.get(key).is_some_and(|facts| !facts.is_empty()) {
                                    Expr::Read(key.clone(), bound.target_span.clone())
                                } else {
                                    Expr::Constant(Member::Unknown, bound.target_span.clone())
                                }
                            })
                            .collect(),
                        bound.target_span.clone(),
                    );
                    let identity = keys.first().cloned().unwrap_or(Key {
                        identity: SemanticIdentity::Occurrence(context.occurrence.id),
                        slot: None,
                    });
                    let mut observed_only = keys.iter().cloned().collect();
                    for fact in self.eval(&value, &mut observed_only)? {
                        self.report_restriction(index, &identity, &fact, &allowed, context);
                    }
                }
            }
        }
        if self.facts.is_empty() {
            return Ok(());
        }
        let typed_report = context.record();
        let report = context.record();
        for k in self.facts.keys() {
            let facts = self.snapshot(k)?;
            let row = context.record();
            let identity = SemanticValue::Identity(k.identity.clone());
            let identity = if let Some(slot) = &k.slot {
                SemanticValue::Tuple(vec![identity, SemanticValue::Symbol(slot.clone())])
            } else {
                identity
            };
            context.set(row, SemanticKey::Symbol("std/types#key".into()), identity)?;
            context.set(
                row,
                SemanticKey::Symbol("std/types#type".into()),
                Self::value(&facts),
            )?;
            let label = format!("{:?}/{:?}", k.identity, k.slot);
            context.set(
                typed_report,
                SemanticKey::Text(label.clone()),
                SemanticValue::Record(row),
            )?;
            let display = SemanticValue::Text(Self::display(&facts));
            context.set(report, SemanticKey::Text(label), display.clone())?;
            // Record identities are generic handles; no variable-library fields,
            // declaration names or lookup policy are consulted by this module.
            if let SemanticIdentity::Binding(record) = k.identity {
                context.set(
                    record,
                    SemanticKey::Symbol(
                        k.slot
                            .clone()
                            .unwrap_or_else(|| "std/types#inferredType".into()),
                    ),
                    display,
                )?;
            }
        }
        context.attach("std/types#typeValues", SemanticValue::Record(typed_report));
        context.attach("std/types#inferredTypes", SemanticValue::Record(report));
        Ok(())
    }
}
struct Collect {
    restriction: bool,
}
impl NativeOperation for Collect {
    fn before(&mut self, context: &mut OperationContext<'_>) -> Result<(), Diagnostic> {
        let target = context
            .argument("key")
            .cloned()
            .unwrap_or(SemanticValue::Absent);
        let value = context
            .argument("type")
            .cloned()
            .unwrap_or(SemanticValue::Absent);
        let destination = destination(&target, context.operation_span)?;
        if matches!(value, SemanticValue::Absent) {
            return Ok(());
        }
        let rule = context.operation_span.clone();
        let target_span = context.identity_span(&target);
        let span = context.identity_span(&value);
        let value = Expr::parse(&value, &span)?;
        context
            .with_service::<Possibilities, _>(MODULE, |state, _| {
                state.register_destination(&destination);
                state.register_expr(&value);
                if self.restriction {
                    let bound = Restriction {
                        destination,
                        value,
                        span,
                        target_span,
                        rule,
                    };
                    if !state.restrictions.contains(&bound) {
                        state.restrictions.push(bound);
                    }
                } else {
                    state.connect(&destination, &value, &span)?;
                    let transfer = Transfer {
                        destination,
                        value,
                        span,
                        rule,
                    };
                    if !state.transfers.contains(&transfer) {
                        state.transfers.push(transfer);
                    }
                }
                Ok(())
            })
            .unwrap_or_else(|| {
                Err(Diagnostic::error(
                    "types.service",
                    "Type service is unavailable",
                    context.occurrence.span.clone(),
                ))
            })
    }
}
struct TypeOf;
impl NativeOperation for TypeOf {
    fn evaluate(&mut self, context: &mut OperationContext<'_>) -> Result<Evaluation, Diagnostic> {
        let key = syntax_key(
            &context
                .argument("key")
                .cloned()
                .unwrap_or(SemanticValue::Absent),
        );
        if key == SemanticValue::Absent {
            return Ok(Evaluation::Ready(SemanticValue::Absent));
        }
        let span = context.identity_span(&key);
        let expr = Expr::read(&key, &span)?;
        context
            .with_service::<Possibilities, _>(MODULE, |state, _| state.register_expr(&expr))
            .ok_or_else(|| {
                Diagnostic::error(
                    "types.service",
                    "Type service is unavailable",
                    context.occurrence.span.clone(),
                )
            })?;
        Ok(Evaluation::Ready(SemanticValue::Deferred {
            service: MODULE.into(),
            key: Box::new(key),
            span,
        }))
    }
}
pub fn register(registry: &mut OperationRegistry) -> Result<(), String> {
    registry.export_symbol(MODULE, "unknown")?;
    registry.register_service(MODULE, || Box::<Possibilities>::default())?;
    for name in [
        "annotate",
        "restrictTo",
        "typeof",
        "instantiate",
        "resolveWith",
    ] {
        let parameters = if name == "resolveWith" {
            vec!["result", "assumptions"]
        } else if name == "instantiate" {
            vec!["template", "use"]
        } else if name == "typeof" {
            vec!["key"]
        } else {
            vec!["key", "type"]
        };
        registry.register(
            MODULE,
            name,
            OperationSignature {
                id: format!("{MODULE}#{name}"),
                provides_context: false,
                finalization_only: true,
                contribution: matches!(name, "annotate" | "restrictTo"),
                parameters: parameters
                    .into_iter()
                    .map(|name| OperationParameter {
                        name: name.into(),
                        kind: ParameterKind::Value,
                        default: None,
                    })
                    .collect(),
            },
            move || match name {
                "typeof" => Box::new(TypeOf) as Box<dyn NativeOperation>,
                "instantiate" => Box::new(instantiation::Instantiate),
                "resolveWith" => Box::new(hypothetical::ResolveWith),
                name => Box::new(Collect {
                    restriction: name == "restrictTo",
                }),
            },
        )?;
    }
    Ok(())
}
