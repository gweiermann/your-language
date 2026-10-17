//! Indexed semantic program. Recursive compiler expressions never enter the executor.
use crate::semantics::queries::{QueryAction, QueryCase, QueryExpr, QueryMessagePart};
use crate::{
    diagnostic::{Diagnostic, Severity, Span},
    ir::CompiledLanguage,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub type ExprId = u32;
pub type ActionId = u32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Opcode {
    Get,
    GetEarlier,
    GetAll,
    Contribute,
    ContributeAll,
    Ancestors,
    Children,
    Find,
    Map,
    Filter,
    Choose,
    Values,
    Alternatives,
    Flatten,
    Last,
    Tail,
    IsList,
    Length,
    HypotheticalAll,
    HypotheticalChecked,
    Every,
    Text,
    IsPresent,
    Equal,
    HasMultiple,
    First,
    Hypothetical,
    Facts,
    Concat,
    Contains,
    Unique,
    IsSubset,
    Lookup,
    Keys,
    Zip,
}
impl Opcode {
    pub fn from_name(name: &str) -> Option<Self> {
        let name = name
            .strip_prefix("core/query#")
            .or_else(|| name.strip_prefix("std/query#"))?;
        Some(match name {
            "get" => Self::Get,
            "getEarlier" => Self::GetEarlier,
            "getAll" => Self::GetAll,
            "contribute" => Self::Contribute,
            "contributeAll" => Self::ContributeAll,
            "ancestors" => Self::Ancestors,
            "children" => Self::Children,
            "find" => Self::Find,
            "map" => Self::Map,
            "filter" => Self::Filter,
            "choose" => Self::Choose,
            "values" => Self::Values,
            "alternatives" => Self::Alternatives,
            "flatten" => Self::Flatten,
            "last" => Self::Last,
            "tail" => Self::Tail,
            "isList" => Self::IsList,
            "length" => Self::Length,
            "hypotheticalAll" => Self::HypotheticalAll,
            "hypotheticalChecked" => Self::HypotheticalChecked,
            "every" => Self::Every,
            "text" => Self::Text,
            "isPresent" => Self::IsPresent,
            "equal" => Self::Equal,
            "hasMultiple" => Self::HasMultiple,
            "first" => Self::First,
            "hypothetical" => Self::Hypothetical,
            "facts" => Self::Facts,
            "concat" => Self::Concat,
            "contains" => Self::Contains,
            "unique" => Self::Unique,
            "isSubset" => Self::IsSubset,
            "lookup" => Self::Lookup,
            "keys" => Self::Keys,
            "zip" => Self::Zip,
            _ => return None,
        })
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Get => "get",
            Self::GetEarlier => "getEarlier",
            Self::GetAll => "getAll",
            Self::Contribute => "contribute",
            Self::ContributeAll => "contributeAll",
            Self::Ancestors => "ancestors",
            Self::Children => "children",
            Self::Find => "find",
            Self::Map => "map",
            Self::Filter => "filter",
            Self::Choose => "choose",
            Self::Values => "values",
            Self::Alternatives => "alternatives",
            Self::Flatten => "flatten",
            Self::Last => "last",
            Self::Tail => "tail",
            Self::IsList => "isList",
            Self::Length => "length",
            Self::HypotheticalAll => "hypotheticalAll",
            Self::HypotheticalChecked => "hypotheticalChecked",
            Self::Every => "every",
            Self::Text => "text",
            Self::IsPresent => "isPresent",
            Self::Equal => "equal",
            Self::HasMultiple => "hasMultiple",
            Self::First => "first",
            Self::Hypothetical => "hypothetical",
            Self::Facts => "facts",
            Self::Concat => "concat",
            Self::Contains => "contains",
            Self::Unique => "unique",
            Self::IsSubset => "isSubset",
            Self::Lookup => "lookup",
            Self::Keys => "keys",
            Self::Zip => "zip",
        }
    }
    pub fn arity(self) -> usize {
        match self {
            Self::Contribute
            | Self::ContributeAll
            | Self::Find
            | Self::Map
            | Self::Filter
            | Self::Every
            | Self::Equal
            | Self::Contains
            | Self::IsSubset
            | Self::Lookup
            | Self::Zip
            | Self::Hypothetical
            | Self::HypotheticalAll
            | Self::HypotheticalChecked => 2,
            Self::Choose => 3,
            _ => 1,
        }
    }
    pub fn is_callback(self) -> bool {
        matches!(self, Self::Find | Self::Map | Self::Filter | Self::Every)
    }
    pub fn is_contribution(self) -> bool {
        matches!(self, Self::Contribute | Self::ContributeAll)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Expr {
    SelfValue,
    Capture(String),
    Local(String),
    Tuple(Vec<ExprId>),
    List(Vec<ExprId>),
    Index {
        value: ExprId,
        index: usize,
    },
    Invoke {
        name: String,
        args: Vec<ExprId>,
    },
    Match {
        inputs: Vec<ExprId>,
        cases: Vec<ValueCase>,
    },
    BindingValue {
        invocation: String,
        result: ExprId,
    },
    Block {
        invocation: String,
        actions: Vec<ActionId>,
        result: ExprId,
    },
    Call {
        opcode: Opcode,
        args: Vec<ExprId>,
    },
    Lambda {
        parameter: String,
        body: ExprId,
    },
    Bool(bool),
    Text(String),
    Symbol(String),
    Absent,
    Not(ExprId),
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum MessagePart {
    Text(String),
    Value(ExprId),
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Case {
    pub alternatives: Vec<Vec<Option<ExprId>>>,
    pub actions: Vec<ActionId>,
    pub span: Span,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Action {
    Call {
        expression: ExprId,
        span: Span,
    },
    If {
        condition: ExprId,
        then_actions: Vec<ActionId>,
        else_actions: Vec<ActionId>,
        span: Span,
    },
    Match {
        inputs: Vec<ExprId>,
        cases: Vec<Case>,
        span: Span,
    },
    Emit {
        severity: Severity,
        message: Vec<MessagePart>,
        span: Span,
        target: Option<ExprId>,
        related: Vec<ExprId>,
    },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ImageGroup {
    pub name: Option<String>,
    pub actions: Vec<ActionId>,
    pub span: Span,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ValueCase {
    pub alternatives: Vec<Vec<Option<ExprId>>>,
    pub value: ExprId,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Function {
    pub parameters: Vec<String>,
    pub body: ExprId,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct QueryImage {
    pub expressions: Vec<Expr>,
    pub actions: Vec<Action>,
    pub groups: BTreeMap<String, Vec<ImageGroup>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub functions: BTreeMap<String, Function>,
}
fn bad(message: impl Into<String>) -> Diagnostic {
    Diagnostic::error("yl.artifact", message, Span::default())
}
impl QueryImage {
    pub fn compile(language: &CompiledLanguage) -> Result<Self, Diagnostic> {
        struct Builder {
            image: QueryImage,
            expressions: BTreeMap<String, ExprId>,
            actions: BTreeMap<String, ActionId>,
            locals: BTreeMap<String, String>,
            lambda_depth: usize,
        }
        impl Builder {
            fn expr(&mut self, source: &QueryExpr) -> Result<ExprId, Diagnostic> {
                let value = match source {
                    QueryExpr::SelfValue => Expr::SelfValue,
                    QueryExpr::Capture(name) => Expr::Capture(name.clone()),
                    QueryExpr::Local(name) => {
                        Expr::Local(self.locals.get(name).unwrap_or(name).clone())
                    }
                    QueryExpr::Tuple(values) => Expr::Tuple(self.exprs(values)?),
                    QueryExpr::List(values) => Expr::List(self.exprs(values)?),
                    QueryExpr::Index { value, index } => Expr::Index {
                        value: self.expr(value)?,
                        index: *index,
                    },
                    QueryExpr::Invoke { name, args } => Expr::Invoke {
                        name: name.clone(),
                        args: self.exprs(args)?,
                    },
                    QueryExpr::Match { inputs, cases } => Expr::Match {
                        inputs: self.exprs(inputs)?,
                        cases: cases
                            .iter()
                            .map(|case| {
                                Ok(ValueCase {
                                    alternatives: case
                                        .alternatives
                                        .iter()
                                        .map(|row| {
                                            row.iter()
                                                .map(|item| {
                                                    item.as_ref()
                                                        .map(|value| self.expr(value))
                                                        .transpose()
                                                })
                                                .collect()
                                        })
                                        .collect::<Result<_, Diagnostic>>()?,
                                    value: self.expr(&case.value)?,
                                })
                            })
                            .collect::<Result<_, Diagnostic>>()?,
                    },
                    QueryExpr::BindingValue { invocation, result } => Expr::BindingValue {
                        invocation: invocation.clone(),
                        result: self.expr(result)?,
                    },
                    QueryExpr::Block {
                        invocation,
                        actions,
                        result,
                    } => Expr::Block {
                        invocation: invocation.clone(),
                        actions: self.actions(actions)?,
                        result: self.expr(result)?,
                    },
                    QueryExpr::Call { name, args } => Expr::Call {
                        opcode: Opcode::from_name(name)
                            .ok_or_else(|| bad(format!("Unknown query operation {name}")))?,
                        args: self.exprs(args)?,
                    },
                    QueryExpr::Lambda { parameter, body } => {
                        let canonical = format!("lambda@{}", self.lambda_depth);
                        let previous = self.locals.insert(parameter.clone(), canonical.clone());
                        self.lambda_depth += 1;
                        let body = self.expr(body);
                        self.lambda_depth -= 1;
                        if let Some(previous) = previous {
                            self.locals.insert(parameter.clone(), previous);
                        } else {
                            self.locals.remove(parameter);
                        }
                        Expr::Lambda {
                            parameter: canonical,
                            body: body?,
                        }
                    }
                    QueryExpr::Bool(value) => Expr::Bool(*value),
                    QueryExpr::Text(value) => Expr::Text(value.clone()),
                    QueryExpr::Symbol(value) => Expr::Symbol(value.clone()),
                    QueryExpr::Absent => Expr::Absent,
                    QueryExpr::Not(value) => Expr::Not(self.expr(value)?),
                };
                let key = serde_json::to_string(&value).map_err(|error| bad(error.to_string()))?;
                if let Some(id) = self.expressions.get(&key) {
                    return Ok(*id);
                }
                let id = u32::try_from(self.image.expressions.len())
                    .map_err(|_| bad("Too many query expressions"))?;
                self.image.expressions.push(value);
                self.expressions.insert(key, id);
                Ok(id)
            }
            fn exprs(&mut self, values: &[QueryExpr]) -> Result<Vec<ExprId>, Diagnostic> {
                values.iter().map(|value| self.expr(value)).collect()
            }
            fn case(&mut self, value: &QueryCase) -> Result<Case, Diagnostic> {
                Ok(Case {
                    alternatives: value
                        .alternatives
                        .iter()
                        .map(|row| {
                            row.iter()
                                .map(|item| item.as_ref().map(|value| self.expr(value)).transpose())
                                .collect()
                        })
                        .collect::<Result<_, _>>()?,
                    actions: self.actions(&value.actions)?,
                    span: value.span.clone(),
                })
            }
            fn actions(&mut self, values: &[QueryAction]) -> Result<Vec<ActionId>, Diagnostic> {
                values
                    .iter()
                    .map(|value| {
                        let action = match value {
                            QueryAction::Call { name, args, span } => Action::Call {
                                expression: self.expr(&QueryExpr::Call {
                                    name: name.clone(),
                                    args: args.clone(),
                                })?,
                                span: span.clone(),
                            },
                            QueryAction::If {
                                condition,
                                then_actions,
                                else_actions,
                                span,
                            } => Action::If {
                                condition: self.expr(condition)?,
                                then_actions: self.actions(then_actions)?,
                                else_actions: self.actions(else_actions)?,
                                span: span.clone(),
                            },
                            QueryAction::Match {
                                inputs,
                                cases,
                                span,
                            } => Action::Match {
                                inputs: self.exprs(inputs)?,
                                cases: cases
                                    .iter()
                                    .map(|case| self.case(case))
                                    .collect::<Result<_, _>>()?,
                                span: span.clone(),
                            },
                            QueryAction::Emit {
                                severity,
                                message,
                                span,
                                target,
                                related,
                            } => Action::Emit {
                                severity: severity.clone(),
                                message: message
                                    .iter()
                                    .map(|part| {
                                        Ok(match part {
                                            QueryMessagePart::Text(text) => {
                                                MessagePart::Text(text.clone())
                                            }
                                            QueryMessagePart::Value(value) => {
                                                MessagePart::Value(self.expr(value)?)
                                            }
                                        })
                                    })
                                    .collect::<Result<_, Diagnostic>>()?,
                                span: span.clone(),
                                target: target
                                    .as_ref()
                                    .map(|value| self.expr(value))
                                    .transpose()?,
                                related: self.exprs(related)?,
                            },
                        };
                        let key = serde_json::to_string(&action)
                            .map_err(|error| bad(error.to_string()))?;
                        if let Some(id) = self.actions.get(&key) {
                            return Ok(*id);
                        }
                        let id = u32::try_from(self.image.actions.len())
                            .map_err(|_| bad("Too many query actions"))?;
                        self.image.actions.push(action);
                        self.actions.insert(key, id);
                        Ok(id)
                    })
                    .collect()
            }
        }
        let mut builder = Builder {
            image: Self::default(),
            expressions: BTreeMap::new(),
            actions: BTreeMap::new(),
            locals: BTreeMap::new(),
            lambda_depth: 0,
        };
        for (definition, groups) in &language.query_meanings {
            let groups = groups
                .iter()
                .map(|group| {
                    Ok(ImageGroup {
                        name: group.name.clone(),
                        actions: builder.actions(&group.actions)?,
                        span: group.span.clone(),
                    })
                })
                .collect::<Result<_, Diagnostic>>()?;
            builder.image.groups.insert(definition.clone(), groups);
        }
        for (name, function) in &language.query_functions {
            let body = builder.expr(&function.body)?;
            builder.image.functions.insert(
                name.clone(),
                Function {
                    parameters: function.parameters.clone(),
                    body,
                },
            );
        }
        builder.image.validate()?;
        Ok(builder.image)
    }
    pub fn validate(&self) -> Result<(), Diagnostic> {
        self.validate_structure()?;
        self.validate_context(&[], true)?;
        for groups in self.groups.values() {
            let actions = groups
                .iter()
                .flat_map(|group| group.actions.iter().copied())
                .collect::<Vec<_>>();
            self.validate_context(&actions, false)?;
        }
        Ok(())
    }
}

impl QueryImage {
    /// Check capture roots against the compiled occurrence owners, without rebuilding YL IR.
    pub fn validate_owner_captures(
        &self,
        captures: &BTreeMap<String, BTreeSet<String>>,
    ) -> Result<(), Diagnostic> {
        let offset = self.expressions.len();
        for (owner, groups) in &self.groups {
            let available = captures
                .get(owner)
                .ok_or_else(|| bad(format!("Unknown query occurrence owner {owner}")))?;
            let mut pending = groups
                .iter()
                .flat_map(|group| group.actions.iter().map(|id| offset + *id as usize))
                .collect::<Vec<_>>();
            let mut visited = BTreeSet::new();
            while let Some(node) = pending.pop() {
                if !visited.insert(node) {
                    continue;
                }
                if node < offset {
                    if let Expr::Capture(name) = &self.expressions[node] {
                        if !available.contains(name.split('.').next().unwrap_or(name)) {
                            return Err(bad(format!(
                                "Query references unavailable capture {name}"
                            )));
                        }
                    }
                }
                pending.extend(self.edges(node)?);
            }
        }
        Ok(())
    }
    fn edges(&self, node: usize) -> Result<Vec<usize>, Diagnostic> {
        let count = self.expressions.len();
        let mut edges = vec![];
        let mut expr = |id: ExprId| -> Result<(), Diagnostic> {
            if id as usize >= count {
                return Err(bad("Query expression index is out of bounds"));
            }
            edges.push(id as usize);
            Ok(())
        };
        let actions: &[ActionId] = if node < count {
            match &self.expressions[node] {
                Expr::Tuple(values) | Expr::List(values) => {
                    for id in values {
                        expr(*id)?;
                    }
                    &[]
                }
                Expr::Call { opcode, args } => {
                    if args.len() != opcode.arity() {
                        return Err(bad("Query opcode has an invalid number of operands"));
                    }
                    for id in args {
                        expr(*id)?;
                    }
                    &[]
                }
                Expr::Invoke { name, args } => {
                    let function = self
                        .functions
                        .get(name)
                        .ok_or_else(|| bad("Unknown compiled query function"))?;
                    if args.len() != function.parameters.len() {
                        return Err(bad(
                            "Query function argument count differs from its signature",
                        ));
                    }
                    for id in args {
                        expr(*id)?;
                    }
                    &[]
                }
                Expr::Match { inputs, cases } => {
                    if inputs.is_empty() || cases.is_empty() {
                        return Err(bad("A value match requires inputs and cases"));
                    }
                    let mut wildcard = false;
                    for id in inputs {
                        expr(*id)?;
                    }
                    for case in cases {
                        if case.alternatives.is_empty() {
                            return Err(bad("A value match case requires patterns"));
                        }
                        for row in &case.alternatives {
                            if wildcard {
                                return Err(bad("A value match wildcard must be last"));
                            }
                            if row.len() != inputs.len() {
                                return Err(bad(
                                    "Value match pattern arity differs from selectors",
                                ));
                            }
                            for id in row.iter().flatten() {
                                expr(*id)?;
                                if !matches!(
                                    &self.expressions[*id as usize],
                                    Expr::Bool(_) | Expr::Text(_) | Expr::Symbol(_) | Expr::Absent
                                ) {
                                    return Err(bad(
                                        "A value match pattern must be a constant or wildcard",
                                    ));
                                }
                            }
                            wildcard = row.iter().all(Option::is_none);
                        }
                        expr(case.value)?;
                    }
                    &[]
                }
                Expr::Index { value, index } => {
                    if u32::try_from(*index).is_err() {
                        return Err(bad("Positional index exceeds the supported range"));
                    }
                    expr(*value)?;
                    &[]
                }
                Expr::BindingValue { result, .. }
                | Expr::Lambda { body: result, .. }
                | Expr::Not(result) => {
                    expr(*result)?;
                    &[]
                }
                Expr::Block {
                    result, actions, ..
                } => {
                    expr(*result)?;
                    actions
                }
                _ => &[],
            }
        } else {
            match self
                .actions
                .get(node - count)
                .ok_or_else(|| bad("Query action index is out of bounds"))?
            {
                Action::Call { expression, .. } => {
                    expr(*expression)?;
                    if !matches!(self.expressions[*expression as usize], Expr::Call { .. }) {
                        return Err(bad("An action call must address a call instruction"));
                    }
                    &[]
                }
                Action::If {
                    condition,
                    then_actions,
                    else_actions,
                    ..
                } => {
                    expr(*condition)?;
                    for id in then_actions.iter().chain(else_actions) {
                        if *id as usize >= self.actions.len() {
                            return Err(bad("Query action index is out of bounds"));
                        }
                        edges.push(count + *id as usize);
                    }
                    &[]
                }
                Action::Match { inputs, cases, .. } => {
                    for input in inputs {
                        expr(*input)?;
                    }
                    if cases.is_empty() {
                        return Err(bad("A semantic match requires cases"));
                    }
                    let mut wildcard = false;
                    for (position, case) in cases.iter().enumerate() {
                        if case.alternatives.is_empty() {
                            return Err(bad("A semantic case requires alternatives"));
                        }
                        for row in &case.alternatives {
                            if row.len() != inputs.len() {
                                return Err(bad("A semantic case does not match the input arity"));
                            }
                            if row.iter().all(Option::is_none) {
                                if position + 1 != cases.len() {
                                    return Err(bad("A wildcard case must be last"));
                                }
                                wildcard = true;
                            }
                            for id in row.iter().flatten() {
                                expr(*id)?;
                            }
                        }
                    }
                    if !wildcard {
                        return Err(bad("A semantic match requires a final wildcard"));
                    }
                    for id in cases.iter().flat_map(|case| &case.actions) {
                        if *id as usize >= self.actions.len() {
                            return Err(bad("Query action index is out of bounds"));
                        }
                        edges.push(count + *id as usize);
                    }
                    &[]
                }
                Action::Emit {
                    message,
                    target,
                    related,
                    ..
                } => {
                    for part in message {
                        if let MessagePart::Value(id) = part {
                            expr(*id)?;
                        }
                    }
                    for id in target.iter().chain(related) {
                        expr(*id)?;
                    }
                    &[]
                }
            }
        };
        for id in actions {
            if *id as usize >= self.actions.len() {
                return Err(bad("Query action index is out of bounds"));
            }
            edges.push(count + *id as usize);
        }
        Ok(edges)
    }
    fn validate_structure(&self) -> Result<(), Diagnostic> {
        let count = self.expressions.len().saturating_add(self.actions.len());
        if count > 1_000_000 {
            return Err(bad("Query image exceeds the instruction limit"));
        }
        let mut keys = BTreeSet::new();
        for expr in &self.expressions {
            let key = serde_json::to_string(expr).map_err(|error| bad(error.to_string()))?;
            if !keys.insert(key) {
                return Err(bad("Query expressions must use canonical shared indices"));
            }
        }
        // Validate every edge before traversal, including unreachable data. The iterative
        // color walk rejects cycles without recursing on attacker-controlled input.
        let graph = (0..count)
            .map(|id| self.edges(id))
            .collect::<Result<Vec<_>, _>>()?;
        let mut colors = vec![0u8; count];
        let mut heights = vec![0usize; count];
        for root in 0..count {
            if colors[root] == 2 {
                continue;
            }
            let mut pending = vec![(root, false)];
            while let Some((node, exiting)) = pending.pop() {
                if exiting {
                    heights[node] =
                        1 + graph[node].iter().map(|id| heights[*id]).max().unwrap_or(0);
                    if heights[node] > 512 {
                        return Err(bad("Query instruction nesting exceeds the execution limit"));
                    }
                    colors[node] = 2;
                    continue;
                }
                match colors[node] {
                    1 => return Err(bad("Query instruction graph contains a cycle")),
                    2 => continue,
                    _ => {}
                }
                colors[node] = 1;
                pending.push((node, true));
                for next in graph[node].iter().rev() {
                    pending.push((*next, false));
                }
            }
        }
        for group in self.groups.values().flatten() {
            for id in &group.actions {
                if *id as usize >= self.actions.len() {
                    return Err(bad("Query group action index is out of bounds"));
                }
            }
        }
        for function in self.functions.values() {
            if function.body as usize >= self.expressions.len() {
                return Err(bad("Query function body index is out of bounds"));
            }
            let unique: BTreeSet<_> = function.parameters.iter().collect();
            if unique.len() != function.parameters.len() {
                return Err(bad("Duplicate query function parameter"));
            }
        }
        Ok(())
    }
    fn validate_context(
        &self,
        roots: &[ActionId],
        include_functions: bool,
    ) -> Result<(), Diagnostic> {
        #[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
        struct Context {
            node: usize,
            locals: BTreeSet<String>,
            callback: bool,
            action_call: bool,
            function: bool,
        }
        let offset = self.expressions.len();
        let mut pending = roots
            .iter()
            .map(|id| Context {
                node: offset + *id as usize,
                locals: BTreeSet::new(),
                callback: false,
                action_call: false,
                function: false,
            })
            .collect::<Vec<_>>();
        pending.extend(
            self.functions
                .values()
                .filter(|_| include_functions)
                .map(|function| Context {
                    node: function.body as usize,
                    locals: function.parameters.iter().cloned().collect(),
                    callback: true,
                    action_call: false,
                    function: true,
                }),
        );
        let mut seen = BTreeSet::new();
        let mut blocks = BTreeMap::<String, (Vec<ActionId>, ExprId)>::new();
        let mut bindings = BTreeSet::new();
        while let Some(context) = pending.pop() {
            if !seen.insert(context.clone()) {
                continue;
            }
            if seen.len() > 1_000_000 {
                return Err(bad("Query context expansion exceeds the execution limit"));
            }
            if context.node >= offset {
                let action = &self.actions[context.node - offset];
                let mut next = |node: usize, action_call: bool| {
                    pending.push(Context {
                        node,
                        action_call,
                        ..context.clone()
                    });
                };
                match action {
                    Action::Call { expression, .. } => next(*expression as usize, true),
                    Action::If {
                        condition,
                        then_actions,
                        else_actions,
                        ..
                    } => {
                        next(*condition as usize, false);
                        for id in then_actions.iter().chain(else_actions) {
                            next(offset + *id as usize, false);
                        }
                    }
                    Action::Match { inputs, cases, .. } => {
                        for id in inputs {
                            next(*id as usize, false);
                        }
                        for case in cases {
                            for id in case.alternatives.iter().flatten().flatten() {
                                next(*id as usize, false);
                            }
                            for id in &case.actions {
                                next(offset + *id as usize, false);
                            }
                        }
                    }
                    Action::Emit {
                        message,
                        target,
                        related,
                        ..
                    } => {
                        for part in message {
                            if let MessagePart::Value(id) = part {
                                next(*id as usize, false);
                            }
                        }
                        for id in target.iter().chain(related) {
                            next(*id as usize, false);
                        }
                    }
                }
                continue;
            }
            let value = &self.expressions[context.node];
            match value {
                Expr::SelfValue | Expr::Capture(_) if context.function => {
                    return Err(bad(
                        "Compiled query functions require explicit syntax parameters",
                    ))
                }
                Expr::Local(name) if !context.locals.contains(name) => {
                    return Err(bad("Unbound query callback parameter"))
                }
                Expr::Lambda { .. } => {
                    return Err(bad("A callback must occupy a callback operand"))
                }
                Expr::BindingValue { invocation, result } => {
                    bindings.insert(invocation.clone());
                    pending.push(Context {
                        node: *result as usize,
                        action_call: false,
                        ..context
                    });
                }
                Expr::Block {
                    invocation,
                    actions,
                    result,
                } => {
                    if context.callback {
                        return Err(bad("Effectful blocks cannot execute inside a callback"));
                    }
                    if invocation.is_empty() {
                        return Err(bad("A query invocation requires a stable identifier"));
                    }
                    let definition = (actions.clone(), *result);
                    if blocks
                        .get(invocation)
                        .is_some_and(|previous| previous != &definition)
                    {
                        return Err(bad("A query invocation has conflicting definitions"));
                    }
                    blocks.insert(invocation.clone(), definition);
                    for id in actions {
                        pending.push(Context {
                            node: offset + *id as usize,
                            action_call: false,
                            ..context.clone()
                        });
                    }
                    pending.push(Context {
                        node: *result as usize,
                        action_call: false,
                        ..context
                    });
                }
                Expr::Call { opcode, args } => {
                    if opcode.is_contribution() && !context.action_call {
                        return Err(bad("A contribution cannot execute as a pure expression"));
                    }
                    for (index, id) in args.iter().enumerate() {
                        if opcode.is_callback() && index == 1 {
                            let Expr::Lambda { parameter, body } = &self.expressions[*id as usize]
                            else {
                                return Err(bad("Expected query callback instruction"));
                            };
                            let mut locals = context.locals.clone();
                            locals.insert(parameter.clone());
                            pending.push(Context {
                                node: *body as usize,
                                locals,
                                callback: true,
                                action_call: false,
                                function: context.function,
                            });
                        } else {
                            pending.push(Context {
                                node: *id as usize,
                                action_call: false,
                                ..context.clone()
                            });
                        }
                    }
                }
                Expr::Tuple(values) | Expr::List(values) => {
                    for id in values {
                        pending.push(Context {
                            node: *id as usize,
                            action_call: false,
                            ..context.clone()
                        });
                    }
                }
                Expr::Invoke { args, .. } => {
                    for id in args {
                        pending.push(Context {
                            node: *id as usize,
                            action_call: false,
                            ..context.clone()
                        });
                    }
                }
                Expr::Match { inputs, cases } => {
                    for id in inputs
                        .iter()
                        .chain(
                            cases
                                .iter()
                                .flat_map(|case| case.alternatives.iter().flatten().flatten()),
                        )
                        .chain(cases.iter().map(|case| &case.value))
                    {
                        pending.push(Context {
                            node: *id as usize,
                            action_call: false,
                            ..context.clone()
                        });
                    }
                }
                Expr::Not(id) | Expr::Index { value: id, .. } => pending.push(Context {
                    node: *id as usize,
                    action_call: false,
                    ..context
                }),
                _ => {}
            }
        }
        if bindings
            .iter()
            .any(|invocation| !blocks.contains_key(invocation))
        {
            return Err(bad("A query binding has no defining invocation"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::semantics::{queries::QueryKey, SemanticEngine, SemanticValue};

    fn image(expressions: Vec<Expr>, actions: Vec<Action>) -> QueryImage {
        QueryImage {
            expressions,
            actions,
            functions: BTreeMap::new(),
            groups: BTreeMap::from([(
                "test#Root".into(),
                vec![ImageGroup {
                    name: None,
                    actions: vec![0],
                    span: Span::default(),
                }],
            )]),
        }
    }
    #[test]
    fn image_executes_without_recursive_compiler_queries() {
        let engine = SemanticEngine::default();
        let language = engine
            .compile(
                "test.yl",
                &BTreeMap::from([(
                    "test.yl".into(),
                    r#"
import { contribute, get, map, equal } from "core/query"
import { error } from "core/diagnostics"
operation record(key, value) {
    contribute(key, value)
    return get(key)
}
node Root = "x" {
    semantics {
        result := record("result", "ready")
        contribute("values", map(["a", "b"], item => (item, result)))
        when !equal(result, "ready") { error("Binding result changed") }
    }
}
entry Root
"#
                    .into(),
                )]),
            )
            .unwrap();
        let loaded = engine.load(&language.to_bytes().unwrap()).unwrap();
        assert!(loaded.query_meanings.is_empty());
        assert!(!loaded
            .image
            .as_ref()
            .unwrap()
            .queries
            .expressions
            .is_empty());
        let result = engine.analyze(&loaded, "program", "x");
        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
        assert!(result
            .graph
            .query_entries
            .iter()
            .any(|entry| entry.key == QueryKey::Text("result".into())
                && entry.value == SemanticValue::Text("ready".into())));
        assert_eq!(result, engine.analyze(&language, "program", "x"));
    }
    #[test]
    fn invalid_query_indices_and_expression_cycles_are_rejected() {
        let mut value = image(
            vec![
                Expr::Not(0),
                Expr::Call {
                    opcode: Opcode::First,
                    args: vec![0],
                },
            ],
            vec![Action::Call {
                expression: 1,
                span: Span::default(),
            }],
        );
        assert!(value.validate().unwrap_err().message.contains("cycle"));
        value.expressions[0] = Expr::Not(u32::MAX);
        assert!(value.validate().is_err());
        value.groups.get_mut("test#Root").unwrap()[0].actions[0] = u32::MAX;
        assert!(value.validate_owner_captures(&BTreeMap::new()).is_err());
    }
    #[test]
    fn positional_index_operands_are_validated_before_execution() {
        let mut value = image(
            vec![
                Expr::Index { value: 0, index: 0 },
                Expr::Call {
                    opcode: Opcode::First,
                    args: vec![0],
                },
            ],
            vec![Action::Call {
                expression: 1,
                span: Span::default(),
            }],
        );
        assert!(value.validate().unwrap_err().message.contains("cycle"));
        value.expressions[0] = Expr::Index {
            value: u32::MAX,
            index: 0,
        };
        assert!(value.validate().is_err());
        if usize::BITS > 32 {
            value.expressions[0] = Expr::Index {
                value: 2,
                index: u32::MAX as usize + 1,
            };
            value.expressions.push(Expr::Tuple(vec![]));
            assert!(value.validate().unwrap_err().message.contains("range"));
        }
    }
    #[test]
    fn action_cycles_cannot_hide_inside_operation_blocks() {
        let value = image(
            vec![
                Expr::Block {
                    invocation: "block".into(),
                    actions: vec![0],
                    result: 1,
                },
                Expr::Absent,
                Expr::Call {
                    opcode: Opcode::First,
                    args: vec![0],
                },
            ],
            vec![Action::Call {
                expression: 2,
                span: Span::default(),
            }],
        );
        assert!(value.validate().is_err());
    }
    #[test]
    fn callback_operands_and_local_bindings_are_validated() {
        let value = image(
            vec![
                Expr::List(vec![]),
                Expr::Local("unbound".into()),
                Expr::Lambda {
                    parameter: "item".into(),
                    body: 1,
                },
                Expr::Call {
                    opcode: Opcode::Map,
                    args: vec![0, 2],
                },
            ],
            vec![Action::Call {
                expression: 3,
                span: Span::default(),
            }],
        );
        assert!(value.validate().is_err());
        let mut valid = value;
        valid.expressions[1] = Expr::Local("item".into());
        assert!(valid.validate().is_ok());
        valid.expressions[3] = Expr::Call {
            opcode: Opcode::Map,
            args: vec![0, 1],
        };
        assert!(valid.validate().is_err());
    }
    #[test]
    fn indexed_query_captures_use_their_own_occurrence_owner() {
        let value = image(
            vec![
                Expr::Capture("missing.member".into()),
                Expr::Call {
                    opcode: Opcode::First,
                    args: vec![0],
                },
            ],
            vec![Action::Call {
                expression: 1,
                span: Span::default(),
            }],
        );
        assert!(value.validate().is_ok());
        assert!(value
            .validate_owner_captures(&BTreeMap::from([(
                "test#Root".into(),
                BTreeSet::from(["name".into()])
            )]))
            .is_err());
    }
    #[test]
    fn equivalent_expressions_cannot_receive_different_correlation_addresses() {
        let value = image(
            vec![
                Expr::Bool(true),
                Expr::Bool(true),
                Expr::Call {
                    opcode: Opcode::First,
                    args: vec![0],
                },
            ],
            vec![Action::Call {
                expression: 2,
                span: Span::default(),
            }],
        );
        assert!(value.validate().unwrap_err().message.contains("canonical"));
    }
    #[test]
    fn excessive_indexed_nesting_is_rejected_without_recursive_validation() {
        let mut expressions = vec![Expr::Absent];
        for index in 0..512 {
            expressions.push(Expr::Not(index));
        }
        expressions.push(Expr::Call {
            opcode: Opcode::First,
            args: vec![512],
        });
        let value = image(
            expressions,
            vec![Action::Call {
                expression: 513,
                span: Span::default(),
            }],
        );
        assert!(value.validate().unwrap_err().message.contains("nesting"));
    }
    #[test]
    fn contributions_are_not_legal_in_pure_operands() {
        let value = image(
            vec![
                Expr::Text("k".into()),
                Expr::Bool(true),
                Expr::Call {
                    opcode: Opcode::Contribute,
                    args: vec![0, 1],
                },
                Expr::Call {
                    opcode: Opcode::First,
                    args: vec![2],
                },
            ],
            vec![Action::Call {
                expression: 3,
                span: Span::default(),
            }],
        );
        assert!(value.validate().is_err());
    }
}
