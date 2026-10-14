//! Indexed parser program. Rule recursion uses rule IDs; instruction edges form a DAG.
use crate::{
    diagnostic::{Diagnostic, Span},
    ir::{CompiledLanguage, Condition, Emission, RuleBody, Term},
    syntax::{Associativity, Quantifier},
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

type CaptureContract = (BTreeSet<String>, BTreeSet<String>);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ParserImage {
    pub entry: u32,
    pub rules: Vec<ImageRule>,
    pub instructions: Vec<Instruction>,
    pub trivia: Vec<u32>,
    pub checks: Vec<CheckExpression>,
    pub preview_roots: BTreeMap<String, u32>,
    #[serde(skip)]
    pub(crate) regexes: BTreeMap<String, regex::Regex>,
}
impl PartialEq for ParserImage {
    fn eq(&self, other: &Self) -> bool {
        self.entry == other.entry
            && self.rules == other.rules
            && self.instructions == other.instructions
            && self.trivia == other.trivia
            && self.checks == other.checks
            && self.preview_roots == other.preview_roots
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ImageCheck {
    pub condition: u32,
    pub emissions: Vec<Emission>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum CheckExpression {
    Bool(bool),
    Text(String),
    Absent,
    CaptureValue(String),
    EnumValue {
        ty: String,
        variant: String,
    },
    Present(String),
    CaptureType {
        capture: String,
        rule: String,
    },
    OptionalMatches {
        capture: String,
        regex: String,
    },
    Matches {
        capture: String,
        regex: String,
    },
    Not(u32),
    And(u32, u32),
    Or(u32, u32),
    Equal(u32, u32),
    Between {
        trivia: Option<u32>,
        left: String,
        right: String,
    },
    Always,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ImageRule {
    pub definition: String,
    pub name: String,
    pub trivia: bool,
    pub body: ImageBody,
    pub constraints: Vec<ImageCheck>,
    pub span: Span,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ImageBody {
    Concrete(u32),
    Abstract {
        bases: Vec<u32>,
        operators: Vec<ImageOperator>,
    },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ImageOperator {
    pub rule: u32,
    pub binding_power: usize,
    pub associativity: Associativity,
    pub left: bool,
    pub right: bool,
    pub left_edge: Option<u32>,
    pub right_edge: Option<u32>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Instruction {
    Example {
        definition: String,
        term: u32,
    },
    Meaning {
        definition: String,
        term: u32,
    },
    Literal(String),
    Regex(String),
    Ref(u32),
    Sequence(Vec<u32>),
    Choice(Vec<u32>),
    Repeat(u32, Quantifier),
    Capture(String, u32),
    NotAhead(u32),
    NotBehind(u32),
    Mark {
        id: u32,
        term: u32,
    },
    Project {
        id: u32,
        term: u32,
        quantifier: Option<Quantifier>,
        collect_lists: bool,
    },
}
impl ParserImage {
    pub(crate) fn rule_id(&self, definition: &str) -> Option<u32> {
        self.rules
            .iter()
            .position(|rule| rule.definition == definition)
            .and_then(|i| u32::try_from(i).ok())
    }
    pub fn compile(language: &CompiledLanguage) -> Result<Self, Diagnostic> {
        let ids: BTreeMap<_, _> = language
            .rules
            .keys()
            .enumerate()
            .map(|(i, k)| {
                Ok((
                    k.clone(),
                    u32::try_from(i).map_err(|_| invalid("Too many parser rules"))?,
                ))
            })
            .collect::<Result<_, Diagnostic>>()?;
        fn lower(
            term: &Term,
            ids: &BTreeMap<String, u32>,
            output: &mut Vec<Instruction>,
        ) -> Result<u32, Diagnostic> {
            let item = match term {
                Term::Example { definition, term } => Instruction::Example {
                    definition: definition.clone(),
                    term: lower(term, ids, output)?,
                },
                Term::Meaning { definition, term } => Instruction::Meaning {
                    definition: definition.clone(),
                    term: lower(term, ids, output)?,
                },
                Term::Literal(v) => Instruction::Literal(v.clone()),
                Term::Regex(v) => Instruction::Regex(v.clone()),
                Term::Ref(v) => {
                    Instruction::Ref(*ids.get(v).ok_or_else(|| invalid("Unknown parser rule"))?)
                }
                Term::Sequence(v) => Instruction::Sequence(
                    v.iter()
                        .map(|t| lower(t, ids, output))
                        .collect::<Result<_, _>>()?,
                ),
                Term::Choice(v) => Instruction::Choice(
                    v.iter()
                        .map(|t| lower(t, ids, output))
                        .collect::<Result<_, _>>()?,
                ),
                Term::Repeat(t, q) => Instruction::Repeat(lower(t, ids, output)?, *q),
                Term::Capture(n, t) => Instruction::Capture(n.clone(), lower(t, ids, output)?),
                Term::NotAhead(t) => Instruction::NotAhead(lower(t, ids, output)?),
                Term::NotBehind(t) => Instruction::NotBehind(lower(t, ids, output)?),
                Term::Mark { id, term } => Instruction::Mark {
                    id: *id,
                    term: lower(term, ids, output)?,
                },
                Term::Project {
                    id,
                    term,
                    quantifier,
                    collect_lists,
                } => Instruction::Project {
                    id: *id,
                    term: lower(term, ids, output)?,
                    quantifier: *quantifier,
                    collect_lists: *collect_lists,
                },
            };
            let id =
                u32::try_from(output.len()).map_err(|_| invalid("Too many parser instructions"))?;
            output.push(item);
            Ok(id)
        }
        fn lower_check(
            condition: &Condition,
            ids: &BTreeMap<String, u32>,
            output: &mut Vec<CheckExpression>,
        ) -> Result<u32, Diagnostic> {
            let c = match condition {
                Condition::Bool(v) => CheckExpression::Bool(*v),
                Condition::Text(v) => CheckExpression::Text(v.clone()),
                Condition::Absent => CheckExpression::Absent,
                Condition::CaptureValue(v) => CheckExpression::CaptureValue(v.clone()),
                Condition::EnumValue { ty, variant } => CheckExpression::EnumValue {
                    ty: ty.clone(),
                    variant: variant.clone(),
                },
                Condition::Present(v) => CheckExpression::Present(v.clone()),
                Condition::CaptureType { capture, rule } => CheckExpression::CaptureType {
                    capture: capture.clone(),
                    rule: rule.clone(),
                },
                Condition::OptionalMatches { capture, regex } => CheckExpression::OptionalMatches {
                    capture: capture.clone(),
                    regex: regex.clone(),
                },
                Condition::Matches { capture, regex } => CheckExpression::Matches {
                    capture: capture.clone(),
                    regex: regex.clone(),
                },
                Condition::Not(a) => CheckExpression::Not(lower_check(a, ids, output)?),
                Condition::And(a, b) => {
                    CheckExpression::And(lower_check(a, ids, output)?, lower_check(b, ids, output)?)
                }
                Condition::Or(a, b) => {
                    CheckExpression::Or(lower_check(a, ids, output)?, lower_check(b, ids, output)?)
                }
                Condition::Equal(a, b) => CheckExpression::Equal(
                    lower_check(a, ids, output)?,
                    lower_check(b, ids, output)?,
                ),
                Condition::Between {
                    trivia,
                    left,
                    right,
                } => CheckExpression::Between {
                    trivia: trivia
                        .as_ref()
                        .map(|id| {
                            ids.get(id)
                                .copied()
                                .ok_or_else(|| invalid("Unknown check trivia rule"))
                        })
                        .transpose()?,
                    left: left.clone(),
                    right: right.clone(),
                },
                Condition::Always => CheckExpression::Always,
            };
            let id =
                u32::try_from(output.len()).map_err(|_| invalid("Too many check instructions"))?;
            output.push(c);
            Ok(id)
        }
        let mut checks = vec![];
        let mut instructions = vec![];
        let mut rules = vec![];
        for (id, rule) in &language.rules {
            let body = match &rule.body {
                RuleBody::Concrete(t) => ImageBody::Concrete(lower(t, &ids, &mut instructions)?),
                RuleBody::Abstract { bases, operators } => ImageBody::Abstract {
                    bases: bases
                        .iter()
                        .map(|id| {
                            ids.get(id)
                                .copied()
                                .ok_or_else(|| invalid("Unknown base rule"))
                        })
                        .collect::<Result<_, _>>()?,
                    operators: operators
                        .iter()
                        .map(|o| {
                            Ok(ImageOperator {
                                rule: *ids
                                    .get(&o.rule)
                                    .ok_or_else(|| invalid("Unknown operator rule"))?,
                                binding_power: o.binding_power,
                                associativity: o.associativity,
                                left: o.left,
                                right: o.right,
                                left_edge: None,
                                right_edge: None,
                            })
                        })
                        .collect::<Result<_, Diagnostic>>()?,
                },
            };
            rules.push(ImageRule {
                definition: id.clone(),
                name: rule.name.clone(),
                trivia: rule.trivia,
                body,
                constraints: rule
                    .constraints
                    .iter()
                    .map(|c| {
                        Ok(ImageCheck {
                            condition: lower_check(&c.condition, &ids, &mut checks)?,
                            emissions: c.emissions.clone(),
                        })
                    })
                    .collect::<Result<_, Diagnostic>>()?,
                span: rule.span.clone(),
            });
        }
        let mut preview_roots = BTreeMap::new();
        for (id, rule) in &language.rules {
            if !rule.trivia {
                let root = lower(&Term::Ref(id.clone()), &ids, &mut instructions)?;
                preview_roots.insert(id.clone(), root);
            }
        }
        for (id, example) in &language.examples {
            let root = lower(&example.term, &ids, &mut instructions)?;
            preview_roots.insert(id.clone(), root);
        }
        let mut image = Self {
            entry: *ids
                .get(&language.entry)
                .ok_or_else(|| invalid("Unknown entry rule"))?,
            rules,
            instructions,
            checks,
            preview_roots,
            regexes: BTreeMap::new(),
            trivia: language
                .trivia
                .iter()
                .map(|id| {
                    ids.get(id)
                        .copied()
                        .ok_or_else(|| invalid("Unknown trivia rule"))
                })
                .collect::<Result<_, _>>()?,
        };
        for i in 0..image.rules.len() {
            if let ImageBody::Abstract { operators, .. } = image.rules[i].body.clone() {
                let operators = operators
                    .into_iter()
                    .map(|mut o| {
                        let ImageBody::Concrete(root) = image.rules[o.rule as usize].body else {
                            return Err(invalid("Operator must be concrete"));
                        };
                        if o.left {
                            o.left_edge = image.edge(root, false);
                        }
                        if o.right {
                            o.right_edge = image.edge(root, true);
                        }
                        Ok(o)
                    })
                    .collect::<Result<_, _>>()?;
                if let ImageBody::Abstract {
                    operators: target, ..
                } = &mut image.rules[i].body
                {
                    *target = operators;
                }
            }
        }
        image.validate()?;
        image.prepare()?;
        Ok(image)
    }
    fn zero_width(&self, id: u32) -> bool {
        match &self.instructions[id as usize] {
            Instruction::NotAhead(_) | Instruction::NotBehind(_) => true,
            Instruction::Literal(s) => s.is_empty(),
            Instruction::Capture(_, t)
            | Instruction::Meaning { term: t, .. }
            | Instruction::Example { term: t, .. }
            | Instruction::Mark { term: t, .. }
            | Instruction::Project { term: t, .. } => self.zero_width(*t),
            Instruction::Sequence(items) => items.iter().all(|t| self.zero_width(*t)),
            _ => false,
        }
    }
    fn edge(&self, id: u32, right: bool) -> Option<u32> {
        match &self.instructions[id as usize] {
            Instruction::Ref(_) => Some(id),
            Instruction::Capture(_, t)
            | Instruction::Meaning { term: t, .. }
            | Instruction::Example { term: t, .. }
            | Instruction::Mark { term: t, .. }
            | Instruction::Project { term: t, .. } => self.edge(*t, right),
            Instruction::Sequence(items) => {
                let mut candidates = items.iter().filter(|t| !self.zero_width(**t));
                self.edge(
                    *if right {
                        candidates.next_back()?
                    } else {
                        candidates.next()?
                    },
                    right,
                )
            }
            _ => None,
        }
    }
    pub fn validate(&self) -> Result<(), Diagnostic> {
        if self
            .rules
            .len()
            .saturating_add(self.instructions.len())
            .saturating_add(self.checks.len())
            > 1_000_000
        {
            return Err(invalid("Parser image exceeds the instruction limit"));
        }
        let rule_ok = |id: u32| (id as usize) < self.rules.len();
        let instruction_ok = |id: u32| (id as usize) < self.instructions.len();
        if !rule_ok(self.entry) || self.trivia.iter().any(|id| !rule_ok(*id)) {
            return Err(invalid("Invalid parser entry or trivia index"));
        }
        let mut depths = vec![];
        for (id, item) in self.instructions.iter().enumerate() {
            let children: Vec<u32> = match item {
                Instruction::Sequence(v) | Instruction::Choice(v) => v.clone(),
                Instruction::Example { term, .. }
                | Instruction::Meaning { term, .. }
                | Instruction::Mark { term, .. }
                | Instruction::Project { term, .. }
                | Instruction::Capture(_, term)
                | Instruction::Repeat(term, _)
                | Instruction::NotAhead(term)
                | Instruction::NotBehind(term) => vec![*term],
                Instruction::Ref(rule) if !rule_ok(*rule) => {
                    return Err(invalid("Invalid parser rule index"))
                }
                Instruction::Regex(pattern) => {
                    regex::Regex::new(pattern)
                        .map_err(|_| invalid("Invalid parser regular expression"))?;
                    vec![]
                }
                _ => vec![],
            };
            if children.iter().any(|child| *child as usize >= id) {
                return Err(invalid(
                    "Parser instruction edges must point to preceding instructions",
                ));
            }
            let depth = children
                .iter()
                .map(|c| depths[*c as usize])
                .max()
                .unwrap_or(0usize)
                + 1;
            if depth > 128 {
                return Err(invalid("Parser instruction nesting exceeds resource limit"));
            }
            depths.push(depth);
        }
        let mut check_depths = vec![];
        for (id, item) in self.checks.iter().enumerate() {
            let children = match item {
                CheckExpression::Not(a) => vec![*a],
                CheckExpression::And(a, b)
                | CheckExpression::Or(a, b)
                | CheckExpression::Equal(a, b) => vec![*a, *b],
                CheckExpression::Between {
                    trivia: Some(t), ..
                } if !rule_ok(*t) => return Err(invalid("Invalid check trivia rule index")),
                CheckExpression::Matches { regex, .. }
                | CheckExpression::OptionalMatches { regex, .. } => {
                    regex::Regex::new(regex)
                        .map_err(|_| invalid("Invalid check regular expression"))?;
                    vec![]
                }
                _ => vec![],
            };
            if children.iter().any(|child| *child as usize >= id) {
                return Err(invalid("Check edges must point to preceding instructions"));
            }
            let depth = children
                .iter()
                .map(|c| check_depths[*c as usize])
                .max()
                .unwrap_or(0usize)
                + 1;
            if depth > 128 {
                return Err(invalid("Check nesting exceeds resource limit"));
            }
            check_depths.push(depth);
        }
        let contracts = self.capture_contracts()?;
        let mut budget = 0;
        let mut definitions = std::collections::BTreeSet::new();
        for (family_id, rule) in self.rules.iter().enumerate() {
            if !definitions.insert(&rule.definition) {
                return Err(invalid("Duplicate parser rule identity"));
            }
            if rule
                .constraints
                .iter()
                .any(|c| c.condition as usize >= self.checks.len())
            {
                return Err(invalid("Invalid check root index"));
            }
            if let ImageBody::Concrete(root) = rule.body {
                if instruction_ok(root) {
                    self.validate_projections(root, &mut BTreeSet::new(), &mut budget)?;
                    self.unique_captures(root, &mut budget)?;
                }
            }
            match &rule.body {
                ImageBody::Concrete(root) if !instruction_ok(*root) => {
                    return Err(invalid("Invalid parser root instruction"))
                }
                ImageBody::Abstract { bases, operators } => {
                    if bases.iter().any(|b| !rule_ok(*b)) {
                        return Err(invalid("Invalid parser base rule index"));
                    }
                    let mut members = BTreeSet::new();
                    for op in operators {
                        if op.binding_power == 0
                            || op.binding_power > self.rules.len()
                            || (!op.left && !op.right)
                            || !members.insert(op.rule)
                        {
                            return Err(invalid(
                                "Invalid precedence binding power, role or duplicate operator",
                            ));
                        }
                        if !rule_ok(op.rule) {
                            return Err(invalid("Invalid parser operator rule index"));
                        }
                        let ImageBody::Concrete(root) = self.rules[op.rule as usize].body else {
                            return Err(invalid("Operator must be concrete"));
                        };
                        if !instruction_ok(root) {
                            return Err(invalid("Invalid parser operator root"));
                        }
                        let left = self.edge(root, false).is_some_and(|edge| {
                            self.instructions[edge as usize] == Instruction::Ref(family_id as u32)
                        });
                        let right = self.edge(root, true).is_some_and(|edge| {
                            self.instructions[edge as usize] == Instruction::Ref(family_id as u32)
                        });
                        if op.left != left || op.right != right {
                            return Err(invalid("Precedence operator does not reference its family at the declared edges"));
                        }
                        if !instruction_ok(root)
                            || op.left_edge != op.left.then(|| self.edge(root, false)).flatten()
                            || op.right_edge != op.right.then(|| self.edge(root, true)).flatten()
                            || (op.left && op.left_edge.is_none())
                            || (op.right && op.right_edge.is_none())
                        {
                            return Err(invalid("Invalid parser operator edge"));
                        }
                    }
                }
                _ => {}
            }
        }
        for (definition, root) in &self.preview_roots {
            if !instruction_ok(*root) {
                return Err(invalid("Invalid preview root instruction"));
            }
            if let Some(rule) = self.rule_id(definition) {
                if self.instructions[*root as usize] != Instruction::Ref(rule) {
                    return Err(invalid("Node preview must reference its actual rule"));
                }
            }
            self.validate_projections(*root, &mut BTreeSet::new(), &mut budget)?;
            self.unique_captures(*root, &mut budget)?;
        }
        for (rule_id, rule) in self.rules.iter().enumerate() {
            if rule.constraints.is_empty() {
                continue;
            }
            let (captures, known) = self.rule_contract(
                rule_id as u32,
                &contracts,
                &mut BTreeSet::new(),
                &mut budget,
            )?;
            for check in &rule.constraints {
                if !matches!(
                    self.validate_check(
                        check.condition,
                        rule_id as u32,
                        &captures,
                        &known,
                        &mut budget
                    )?,
                    CheckType::Bool(_) | CheckType::Absent
                ) {
                    return Err(invalid("Check guard must be boolean"));
                }
            }
        }
        Ok(())
    }
    fn marker_root(&self, root: u32, marker: u32) -> Option<u32> {
        let mut pending = vec![root];
        let mut seen = BTreeSet::new();
        while let Some(id) = pending.pop() {
            if !seen.insert(id) {
                continue;
            }
            match &self.instructions[id as usize] {
                Instruction::Mark { id, term } if *id == marker => return Some(*term),
                Instruction::Sequence(items) | Instruction::Choice(items) => {
                    pending.extend(items.iter().rev())
                }
                Instruction::Repeat(t, _)
                | Instruction::Capture(_, t)
                | Instruction::Meaning { term: t, .. }
                | Instruction::Example { term: t, .. }
                | Instruction::Mark { term: t, .. }
                | Instruction::Project { term: t, .. } => pending.push(*t),
                _ => {}
            }
        }
        None
    }
    fn capture_contracts(&self) -> Result<Vec<CaptureContract>, Diagnostic> {
        let mut contracts: Vec<CaptureContract> = vec![];
        let mut may_none: Vec<bool> = vec![];
        let mut budget = 0usize;
        for item in &self.instructions {
            let contract = match item {
                Instruction::Capture(name, t) => {
                    let (mut p, mut g) = contracts[*t as usize].clone();
                    p.insert(name.clone());
                    if !may_none[*t as usize] {
                        g.insert(name.clone());
                    }
                    (p, g)
                }
                Instruction::Sequence(items) => {
                    let (mut p, mut g) = (BTreeSet::new(), BTreeSet::new());
                    for t in items {
                        let (a, b) = &contracts[*t as usize];
                        p.extend(a.iter().cloned());
                        g.extend(b.iter().cloned());
                    }
                    (p, g)
                }
                Instruction::Choice(items) => {
                    let mut p = BTreeSet::new();
                    let mut g: Option<BTreeSet<String>> = None;
                    for t in items {
                        let (a, b) = &contracts[*t as usize];
                        p.extend(a.iter().cloned());
                        g = Some(
                            g.map_or_else(|| b.clone(), |g| g.intersection(b).cloned().collect()),
                        );
                    }
                    (p, g.unwrap_or_default())
                }
                Instruction::Repeat(t, q) => {
                    let (p, g) = contracts[*t as usize].clone();
                    (
                        p,
                        if *q == Quantifier::Plus {
                            g
                        } else {
                            BTreeSet::new()
                        },
                    )
                }
                Instruction::Mark { term: t, .. }
                | Instruction::Meaning { term: t, .. }
                | Instruction::Example { term: t, .. } => contracts[*t as usize].clone(),
                Instruction::Project {
                    id,
                    term,
                    quantifier,
                    ..
                } => {
                    let (p, g) = self
                        .marker_root(*term, *id)
                        .map(|t| contracts[t as usize].clone())
                        .unwrap_or_default();
                    (
                        p,
                        if *quantifier == Some(Quantifier::Star) {
                            BTreeSet::new()
                        } else {
                            g
                        },
                    )
                }
                _ => Default::default(),
            };
            let absent = match item {
                Instruction::Repeat(_, Quantifier::Optional)
                | Instruction::NotAhead(_)
                | Instruction::NotBehind(_) => true,
                Instruction::Capture(_, t)
                | Instruction::Meaning { term: t, .. }
                | Instruction::Example { term: t, .. }
                | Instruction::Mark { term: t, .. } => may_none[*t as usize],
                Instruction::Project {
                    id,
                    term,
                    quantifier: None,
                    ..
                } => self
                    .marker_root(*term, *id)
                    .is_none_or(|t| may_none[t as usize]),
                Instruction::Choice(items) => items.iter().any(|t| may_none[*t as usize]),
                _ => false,
            };
            budget = budget
                .saturating_add(contract.0.len())
                .saturating_add(contract.1.len());
            if budget > 1_000_000 {
                return Err(invalid("Parser capture contracts exceed resource limit"));
            }
            contracts.push(contract);
            may_none.push(absent);
        }
        Ok(contracts)
    }
    /// Capture contracts derive from instruction edges, never untrusted metadata.
    pub(crate) fn capture_sets(&self) -> BTreeMap<String, BTreeSet<String>> {
        let contracts = self
            .capture_contracts()
            .expect("validated parser capture contracts");
        let mut rules: Vec<BTreeSet<String>> = self
            .rules
            .iter()
            .map(|rule| match rule.body {
                ImageBody::Concrete(t) => contracts[t as usize].0.clone(),
                _ => BTreeSet::new(),
            })
            .collect();
        loop {
            let mut changed = false;
            for (i, rule) in self.rules.iter().enumerate() {
                if let ImageBody::Abstract { bases, operators } = &rule.body {
                    let additions: BTreeSet<_> = bases
                        .iter()
                        .copied()
                        .chain(operators.iter().map(|o| o.rule))
                        .flat_map(|r| rules[r as usize].iter().cloned())
                        .collect();
                    let before = rules[i].len();
                    rules[i].extend(additions);
                    changed |= rules[i].len() != before;
                }
            }
            if !changed {
                break;
            }
        }
        let mut result: BTreeMap<String, BTreeSet<String>> = self
            .rules
            .iter()
            .zip(rules)
            .map(|(r, c)| (r.definition.clone(), c))
            .collect();
        for item in &self.instructions {
            if let Instruction::Meaning { definition, term } = item {
                result
                    .entry(definition.clone())
                    .and_modify(|names| {
                        names.retain(|name| contracts[*term as usize].0.contains(name))
                    })
                    .or_insert_with(|| contracts[*term as usize].0.clone());
            }
        }
        result
    }
    fn unique_captures(&self, id: u32, budget: &mut usize) -> Result<BTreeSet<String>, Diagnostic> {
        *budget += 1;
        if *budget > 1_000_000 {
            return Err(invalid("Parser validation exceeds resource limit"));
        }
        let mut result = BTreeSet::new();
        match &self.instructions[id as usize] {
            Instruction::Capture(name, t) => {
                result = self.unique_captures(*t, budget)?;
                if !result.insert(name.clone()) {
                    return Err(invalid("Duplicate parser capture"));
                }
            }
            Instruction::Sequence(items) => {
                for t in items {
                    let names = self.unique_captures(*t, budget)?;
                    if !result.is_disjoint(&names) {
                        return Err(invalid("Duplicate parser capture"));
                    }
                    result.extend(names);
                }
            }
            Instruction::Choice(items) => {
                for t in items {
                    result.extend(self.unique_captures(*t, budget)?);
                }
            }
            Instruction::Repeat(t, _)
            | Instruction::Mark { term: t, .. }
            | Instruction::Meaning { term: t, .. }
            | Instruction::Example { term: t, .. } => result = self.unique_captures(*t, budget)?,
            Instruction::Project { id, term, .. } => {
                if let Some(t) = self.marker_root(*term, *id) {
                    result = self.unique_captures(t, budget)?;
                }
            }
            _ => {}
        }
        Ok(result)
    }
    fn rule_contract(
        &self,
        id: u32,
        contracts: &[CaptureContract],
        visiting: &mut BTreeSet<u32>,
        budget: &mut usize,
    ) -> Result<CaptureContract, Diagnostic> {
        *budget += 1;
        if *budget > 1_000_000 || visiting.len() > 256 {
            return Err(invalid("Parser family validation exceeds resource limit"));
        }
        if !visiting.insert(id) {
            return Ok(Default::default());
        }
        let result = match &self.rules[id as usize].body {
            ImageBody::Concrete(root) => contracts[*root as usize].clone(),
            ImageBody::Abstract { bases, operators } => {
                let mut p = BTreeSet::new();
                let mut g: Option<BTreeSet<String>> = None;
                for id in bases
                    .iter()
                    .copied()
                    .chain(operators.iter().map(|o| o.rule))
                {
                    if visiting.contains(&id) {
                        continue;
                    }
                    let (a, b) = self.rule_contract(id, contracts, visiting, budget)?;
                    p.extend(a);
                    g = Some(
                        g.map_or_else(|| b.clone(), |g| g.intersection(&b).cloned().collect()),
                    );
                }
                (p, g.unwrap_or_default())
            }
        };
        visiting.remove(&id);
        Ok(result)
    }
    fn capture_inputs(&self, rule: u32, name: &str) -> Vec<u32> {
        let mut pending_rules = vec![rule];
        let mut seen_rules = BTreeSet::new();
        let mut pending = vec![];
        while let Some(id) = pending_rules.pop() {
            if !seen_rules.insert(id) {
                continue;
            }
            match &self.rules[id as usize].body {
                ImageBody::Concrete(root) => pending.push(*root),
                ImageBody::Abstract { bases, operators } => {
                    pending_rules.extend(bases);
                    pending_rules.extend(operators.iter().map(|o| o.rule));
                }
            }
        }
        let mut result = vec![];
        let mut seen = BTreeSet::new();
        while let Some(id) = pending.pop() {
            if !seen.insert(id) {
                continue;
            }
            match &self.instructions[id as usize] {
                Instruction::Capture(n, t) if n == name => result.push(*t),
                Instruction::Sequence(items) | Instruction::Choice(items) => pending.extend(items),
                Instruction::Capture(_, t)
                | Instruction::Repeat(t, _)
                | Instruction::Meaning { term: t, .. }
                | Instruction::Example { term: t, .. }
                | Instruction::Mark { term: t, .. } => pending.push(*t),
                Instruction::Project { id, term, .. } => {
                    pending.extend(self.marker_root(*term, *id))
                }
                _ => {}
            }
        }
        result
    }
    fn string_instruction(&self, id: u32) -> bool {
        match &self.instructions[id as usize] {
            Instruction::Literal(_) | Instruction::Regex(_) => true,
            Instruction::Example { term: t, .. }
            | Instruction::Repeat(t, Quantifier::Optional)
            | Instruction::Capture(_, t)
            | Instruction::Mark { term: t, .. } => self.string_instruction(*t),
            Instruction::Choice(items) => items.iter().all(|t| self.string_instruction(*t)),
            Instruction::Project {
                id,
                term,
                quantifier: None | Some(Quantifier::Optional),
                ..
            } => self
                .marker_root(*term, *id)
                .is_some_and(|t| self.string_instruction(t)),
            _ => false,
        }
    }
    fn node_instruction(&self, id: u32, expected: u32) -> bool {
        match &self.instructions[id as usize] {
            Instruction::Ref(rule) => {
                let mut pending = vec![expected];
                let mut seen = BTreeSet::new();
                while let Some(id) = pending.pop() {
                    if id == *rule {
                        return true;
                    }
                    if !seen.insert(id) {
                        continue;
                    }
                    if let ImageBody::Abstract { bases, operators } = &self.rules[id as usize].body
                    {
                        pending.extend(bases);
                        pending.extend(operators.iter().map(|o| o.rule));
                    }
                }
                false
            }
            Instruction::Capture(_, t)
            | Instruction::Meaning { term: t, .. }
            | Instruction::Example { term: t, .. }
            | Instruction::Mark { term: t, .. } => self.node_instruction(*t, expected),
            Instruction::Choice(items) => items.iter().all(|t| self.node_instruction(*t, expected)),
            Instruction::Project {
                id,
                term,
                quantifier: None,
                ..
            } => self
                .marker_root(*term, *id)
                .is_some_and(|t| self.node_instruction(t, expected)),
            _ => false,
        }
    }
    fn check_presence(&self, id: u32, truth: bool) -> BTreeSet<String> {
        match &self.checks[id as usize] {
            CheckExpression::Present(name) if truth => BTreeSet::from([name.clone()]),
            CheckExpression::Not(a) => self.check_presence(*a, !truth),
            CheckExpression::And(a, b) if truth => self
                .check_presence(*a, true)
                .union(&self.check_presence(*b, true))
                .cloned()
                .collect(),
            CheckExpression::Or(a, b) if !truth => self
                .check_presence(*a, false)
                .union(&self.check_presence(*b, false))
                .cloned()
                .collect(),
            CheckExpression::Equal(a, b) => {
                match (&self.checks[*a as usize], &self.checks[*b as usize]) {
                    (CheckExpression::CaptureValue(name), CheckExpression::Absent)
                    | (CheckExpression::Absent, CheckExpression::CaptureValue(name))
                        if !truth =>
                    {
                        BTreeSet::from([name.clone()])
                    }
                    _ => BTreeSet::new(),
                }
            }
            CheckExpression::OptionalMatches { capture, .. } if truth => {
                BTreeSet::from([capture.clone()])
            }
            _ => BTreeSet::new(),
        }
    }
    fn validate_check(
        &self,
        id: u32,
        rule: u32,
        captures: &BTreeSet<String>,
        known: &BTreeSet<String>,
        budget: &mut usize,
    ) -> Result<CheckType, Diagnostic> {
        *budget += 1;
        if *budget > 1_000_000 {
            return Err(invalid("Check validation exceeds resource limit"));
        }
        let capture = |name: &String, required: bool| -> Result<(), Diagnostic> {
            if !captures.contains(name) {
                return Err(invalid("Check references unknown capture"));
            }
            if required && !known.contains(name) {
                return Err(invalid(
                    "Check requires an optional capture without a presence guard",
                ));
            }
            Ok(())
        };
        let recurse =
            |id, budget: &mut usize| self.validate_check(id, rule, captures, known, budget);
        Ok(match &self.checks[id as usize] {
            CheckExpression::Always | CheckExpression::Bool(_) => CheckType::Bool(false),
            CheckExpression::Text(_) => CheckType::Text,
            CheckExpression::Absent => CheckType::Absent,
            CheckExpression::EnumValue { ty, variant } => {
                if ty.is_empty() || variant.is_empty() {
                    return Err(invalid("Invalid enum check value"));
                }
                CheckType::Enum(ty.clone())
            }
            CheckExpression::CaptureValue(name) => {
                capture(name, false)?;
                if self
                    .capture_inputs(rule, name)
                    .iter()
                    .any(|t| !self.string_instruction(*t))
                {
                    return Err(invalid("Check equality requires scalar capture values"));
                }
                CheckType::Text
            }
            CheckExpression::CaptureType {
                capture: name,
                rule: expected,
            } => {
                capture(name, true)?;
                let expected = self
                    .rule_id(expected)
                    .ok_or_else(|| invalid("Unknown check node type"))?;
                let inputs = self.capture_inputs(rule, name);
                if inputs.is_empty() || inputs.iter().any(|t| !self.node_instruction(*t, expected))
                {
                    return Err(invalid("Check capture has incompatible node type"));
                }
                CheckType::Bool(false)
            }
            CheckExpression::Present(name) => {
                capture(name, false)?;
                CheckType::Bool(false)
            }
            CheckExpression::Matches { capture: name, .. }
            | CheckExpression::OptionalMatches { capture: name, .. } => {
                let optional = matches!(
                    &self.checks[id as usize],
                    CheckExpression::OptionalMatches { .. }
                );
                capture(name, !optional)?;
                CheckType::Bool(optional && !known.contains(name))
            }
            CheckExpression::Between {
                trivia,
                left,
                right,
            } => {
                capture(left, true)?;
                capture(right, true)?;
                if trivia.is_some_and(|id| !self.rules[id as usize].trivia) {
                    return Err(invalid("Check between owner is not trivia"));
                }
                CheckType::Bool(false)
            }
            CheckExpression::Not(a) => {
                let t = recurse(*a, budget)?;
                if !matches!(t, CheckType::Bool(_) | CheckType::Absent) {
                    return Err(invalid("Check negation requires boolean operands"));
                }
                t
            }
            CheckExpression::And(a, b) | CheckExpression::Or(a, b) => {
                let left = recurse(*a, budget)?;
                let mut known = known.clone();
                known.extend(self.check_presence(
                    *a,
                    matches!(&self.checks[id as usize], CheckExpression::And(..)),
                ));
                let right = self.validate_check(*b, rule, captures, &known, budget)?;
                let bool_type = |t| match t {
                    CheckType::Bool(optional) => Some(optional),
                    CheckType::Absent => Some(true),
                    _ => None,
                };
                let (Some(a), Some(b)) = (bool_type(left), bool_type(right)) else {
                    return Err(invalid("Logical checks require boolean operands"));
                };
                CheckType::Bool(a || b)
            }
            CheckExpression::Equal(a, b) => {
                if let (CheckExpression::CaptureValue(name), CheckExpression::Absent)
                | (CheckExpression::Absent, CheckExpression::CaptureValue(name)) =
                    (&self.checks[*a as usize], &self.checks[*b as usize])
                {
                    capture(name, false)?;
                    return Ok(CheckType::Bool(false));
                }
                let (a, b) = (recurse(*a, budget)?, recurse(*b, budget)?);
                if !(matches!(
                    (&a, &b),
                    (CheckType::Absent, _)
                        | (_, CheckType::Absent)
                        | (CheckType::Bool(_), CheckType::Bool(_))
                ) || a == b)
                {
                    return Err(invalid("Check equality operands have incompatible types"));
                }
                CheckType::Bool(false)
            }
        })
    }
    fn value_shape(&self, id: u32) -> Option<Quantifier> {
        match &self.instructions[id as usize] {
            Instruction::Repeat(_, q) => Some(*q),
            Instruction::Capture(_, t)
            | Instruction::Meaning { term: t, .. }
            | Instruction::Example { term: t, .. }
            | Instruction::Mark { term: t, .. } => self.value_shape(*t),
            Instruction::Project { quantifier, .. } => *quantifier,
            _ => None,
        }
    }
    fn marker_cardinality(&self, root: u32, marker: u32, flatten: bool) -> (usize, Option<usize>) {
        let mut counts: Vec<(usize, Option<usize>)> = vec![];
        for item in self.instructions.iter().take(root as usize + 1) {
            let count = match item {
                Instruction::Mark { id, term } if *id == marker => match self.value_shape(*term) {
                    Some(Quantifier::Optional) => (0, Some(1)),
                    Some(Quantifier::Star) if flatten => (0, None),
                    Some(Quantifier::Plus) if flatten => (1, None),
                    _ => (1, Some(1)),
                },
                Instruction::Sequence(items) => items.iter().map(|t| counts[*t as usize]).fold(
                    (0usize, Some(0usize)),
                    |(min, max), (a, b)| {
                        (
                            min.saturating_add(a),
                            max.zip(b).and_then(|(a, b)| a.checked_add(b)),
                        )
                    },
                ),
                Instruction::Choice(items) => items
                    .iter()
                    .map(|t| counts[*t as usize])
                    .reduce(|(a, b), (c, d)| (a.min(c), b.zip(d).map(|(b, d)| b.max(d))))
                    .unwrap_or((0, Some(0))),
                Instruction::Repeat(t, q) => {
                    let (min, max) = counts[*t as usize];
                    match q {
                        Quantifier::Optional => (0, max),
                        Quantifier::Star => (0, if max == Some(0) { Some(0) } else { None }),
                        Quantifier::Plus => (min, if max == Some(0) { Some(0) } else { None }),
                    }
                }
                Instruction::Capture(_, t)
                | Instruction::Meaning { term: t, .. }
                | Instruction::Example { term: t, .. }
                | Instruction::Mark { term: t, .. }
                | Instruction::Project { term: t, .. } => counts[*t as usize],
                _ => (0, Some(0)),
            };
            counts.push(count);
        }
        counts[root as usize]
    }
    fn validate_projections(
        &self,
        id: u32,
        active: &mut BTreeSet<u32>,
        budget: &mut usize,
    ) -> Result<(), Diagnostic> {
        *budget += 1;
        if *budget > 1_000_000 {
            return Err(invalid("Parser validation exceeds resource limit"));
        }
        match &self.instructions[id as usize] {
            Instruction::Project {
                id,
                term,
                quantifier,
                collect_lists,
            } => {
                if !active.insert(*id) {
                    return Err(invalid("Nested projection reuses its enclosing marker"));
                }
                let (min, max) = self.marker_cardinality(*term, *id, *collect_lists);
                let valid = match quantifier {
                    None => min == 1 && max == Some(1),
                    Some(Quantifier::Optional) => max.is_some_and(|n| n <= 1),
                    Some(Quantifier::Star) => true,
                    Some(Quantifier::Plus) => min >= 1,
                };
                if !valid {
                    return Err(invalid("Projection output violates its cardinality"));
                }
                self.validate_projections(*term, active, budget)?;
                active.remove(id);
            }
            Instruction::Mark { id, term } => {
                if !active.contains(id) {
                    return Err(invalid("Value marker is outside its projection"));
                }
                self.validate_projections(*term, active, budget)?;
            }
            Instruction::Sequence(items) | Instruction::Choice(items) => {
                for t in items {
                    self.validate_projections(*t, active, budget)?;
                }
            }
            Instruction::Repeat(t, _)
            | Instruction::Meaning { term: t, .. }
            | Instruction::Example { term: t, .. }
            | Instruction::Capture(_, t)
            | Instruction::NotAhead(t)
            | Instruction::NotBehind(t) => self.validate_projections(*t, active, budget)?,
            _ => {}
        }
        Ok(())
    }
    /// Prepare immutable regex programs once after loading the indexed image.
    pub fn prepare(&mut self) -> Result<(), Diagnostic> {
        let patterns = self
            .instructions
            .iter()
            .filter_map(|item| match item {
                Instruction::Regex(pattern) => Some(pattern),
                _ => None,
            })
            .chain(self.checks.iter().filter_map(|item| match item {
                CheckExpression::Matches { regex, .. }
                | CheckExpression::OptionalMatches { regex, .. } => Some(regex),
                _ => None,
            }));
        for pattern in patterns {
            if !self.regexes.contains_key(pattern) {
                let compiled = regex::Regex::new(pattern)
                    .map_err(|_| invalid("Invalid parser regular expression"))?;
                self.regexes.insert(pattern.clone(), compiled);
            }
        }
        Ok(())
    }
    pub(crate) fn capture_repeats(&self, id: u32) -> bool {
        match &self.instructions[id as usize] {
            Instruction::Repeat(_, Quantifier::Star | Quantifier::Plus)
            | Instruction::Project {
                quantifier: Some(Quantifier::Star | Quantifier::Plus),
                ..
            } => true,
            Instruction::Capture(_, t)
            | Instruction::Mark { term: t, .. }
            | Instruction::Example { term: t, .. } => self.capture_repeats(*t),
            _ => false,
        }
    }
}
fn invalid(message: &str) -> Diagnostic {
    Diagnostic::error("yl.artifact", message, Span::new("<ylc>", 0, 0))
}

#[derive(PartialEq)]
enum CheckType {
    Bool(bool),
    Text,
    Enum(String),
    Absent,
}

#[cfg(test)]
mod tests {
    use super::*;
    fn compile(source: &str) -> CompiledLanguage {
        crate::compile_sources(
            "test.yl",
            &BTreeMap::from([("test.yl".into(), source.into())]),
        )
        .unwrap()
    }
    fn image(source: &str) -> ParserImage {
        compile(source).image.unwrap().parser
    }
    #[test]
    fn instruction_and_check_edges_reject_cycles_and_invalid_indices() {
        let mut parser = image("node Root = name: /[a-z]+/ entry Root");
        parser.instructions[0] = Instruction::Capture("cycle".into(), 0);
        assert!(parser.validate().is_err());
        let mut parser = image("node Root = \"x\" entry Root");
        parser.instructions[0] = Instruction::Ref(u32::MAX);
        assert!(parser.validate().is_err());
        let mut parser = image("node Root = \"x\" entry Root");
        parser.checks.push(CheckExpression::Not(0));
        assert!(parser.validate().is_err());
    }
    #[test]
    fn checks_validate_capture_contracts_and_operand_types() {
        let mut parser = image(
            r#"import { warn } from "core/diagnostics"
node Root = name: /x/ { checks { when name.matches(/x/) { warn("matched") } } } entry Root"#,
        );
        let root = parser.rules[parser.entry as usize].constraints[0].condition;
        parser.checks[root as usize] = CheckExpression::Present("missing".into());
        assert!(parser.validate().is_err());
        parser.checks[root as usize] = CheckExpression::Text("not boolean".into());
        assert!(parser.validate().is_err());
    }
    #[test]
    fn compiled_operator_edges_are_validated_not_recomputed_at_parse_time() {
        let mut parser = image(
            r#"node Expression {
node Number = /[0-9]+/
node Sum = left: Expression "+" right: Expression
precedence { Sum }
} entry Expression"#,
        );
        let ImageBody::Abstract { operators, .. } = &mut parser.rules[parser.entry as usize].body
        else {
            panic!("abstract entry")
        };
        assert!(operators[0].left_edge.is_some());
        operators[0].left_edge = None;
        assert!(parser.validate().is_err());
    }
    #[test]
    fn parser_runs_without_compiler_rules_and_regexes_prepare_once() {
        let mut language = compile("node Root = name: /[a-z]+/ entry Root");
        let mut image = language.image.take().unwrap();
        assert_eq!(image.parser.regexes.len(), 1);
        let serialized = serde_json::to_string(&image.parser).unwrap();
        let mut loaded: ParserImage = serde_json::from_str(&serialized).unwrap();
        assert!(loaded.regexes.is_empty());
        loaded.validate().unwrap();
        loaded.prepare().unwrap();
        assert_eq!(image.parser, loaded);
        image.parser = loaded;
        language.rules.clear();
        language.image = Some(image);
        assert!(crate::parse(&language, "abc").ast.is_some());
        assert!(crate::parse(&language, "42").has_errors());
    }
}
