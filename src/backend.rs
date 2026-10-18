//! Host-controlled backends. Dispatch never evaluates children or stores runtime values.
use crate::{
    diagnostic::{Diagnostic, Severity, Span},
    semantics::{AnalysisResult, Occurrence, OccurrenceSelection, SemanticEngine, SourceCapture},
    CompiledLanguage,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
};

/// A module-qualified hook contract, independent of any host implementation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HookContract {
    pub id: String,
    pub parameters: Vec<String>,
    pub span: Span,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CallPlan {
    pub hook: u32,
    pub arguments: Vec<ArgumentPlan>,
    pub span: Span,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ArgumentPlan {
    SelfSyntax,
    Capture(String),
    Tuple(Vec<ArgumentPlan>),
    List(Vec<ArgumentPlan>),
}
/// Normalized, source-independent dispatch plans. No surface expressions are interpreted.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackendImage {
    pub hooks: Vec<HookContract>,
    pub calls: BTreeMap<String, CallPlan>,
}
impl BackendImage {
    /// Keep only entry-reachable mappings, preserving canonical hook order and
    /// updating every retained call's table index. All mappings must be validated
    /// first so malformed unused declarations remain compile errors.
    pub(crate) fn retain_reachable(&mut self, owners: &BTreeSet<String>) {
        self.calls.retain(|owner, _| owners.contains(owner));
        let used: BTreeSet<_> = self.calls.values().map(|call| call.hook).collect();
        let mut remap = BTreeMap::new();
        let mut hooks = Vec::with_capacity(used.len());
        for (index, hook) in std::mem::take(&mut self.hooks).into_iter().enumerate() {
            if used.contains(&(index as u32)) {
                remap.insert(index as u32, hooks.len() as u32);
                hooks.push(hook);
            }
        }
        self.hooks = hooks;
        for call in self.calls.values_mut() {
            call.hook = remap[&call.hook];
        }
    }

    pub(crate) fn validate(
        &self,
        captures: &BTreeMap<String, BTreeSet<String>>,
    ) -> Result<(), Diagnostic> {
        let invalid = crate::image::invalid;
        let mut ids = BTreeSet::new();
        for hook in &self.hooks {
            if hook.id.is_empty()
                || !hook.id.contains('#')
                || !ids.insert(&hook.id)
                || hook.parameters.iter().any(|p| p.is_empty())
                || hook.parameters.iter().collect::<BTreeSet<_>>().len() != hook.parameters.len()
            {
                return Err(invalid("Invalid or duplicate backend hook contract"));
            }
        }
        fn check(
            arg: &ArgumentPlan,
            available: &BTreeSet<String>,
            depth: usize,
        ) -> Result<(), Diagnostic> {
            if depth > 64 {
                return Err(crate::image::invalid(
                    "Backend argument exceeds nesting limit",
                ));
            }
            match arg {
                ArgumentPlan::Capture(name)
                    if !available.contains(name.split('.').next().unwrap_or_default()) =>
                {
                    Err(crate::image::invalid(
                        "Backend argument names an unavailable capture",
                    ))
                }
                ArgumentPlan::Tuple(items) | ArgumentPlan::List(items) => {
                    for item in items {
                        check(item, available, depth + 1)?;
                    }
                    Ok(())
                }
                _ => Ok(()),
            }
        }
        for (owner, call) in &self.calls {
            let available = captures
                .get(owner)
                .ok_or_else(|| invalid("Backend call has an unknown occurrence owner"))?;
            let hook = self
                .hooks
                .get(call.hook as usize)
                .ok_or_else(|| invalid("Backend call has an invalid hook ID"))?;
            if hook.parameters.len() != call.arguments.len() {
                return Err(invalid("Backend hook call has incompatible arity"));
            }
            for arg in &call.arguments {
                check(arg, available, 0).map_err(|mut error| {
                    error.primary = Box::new(call.span.clone());
                    error
                })?;
            }
        }
        Ok(())
    }
}
/// An opaque handle belonging to exactly one analyzed program, including terminal captures.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SyntaxRef {
    program: u64,
    id: usize,
    terminal: bool,
}
/// Unevaluated syntax arguments; tuple/list/optional shape is retained.
#[derive(Clone, Debug, PartialEq)]
pub enum Capture {
    Syntax(SyntaxRef),
    List(Vec<Capture>),
    Tuple(Vec<Capture>),
    Absent,
}
impl Capture {
    pub fn syntax(&self) -> Result<SyntaxRef, Diagnostic> {
        if let Self::Syntax(value) = self {
            Ok(*value)
        } else {
            Err(Diagnostic::error(
                "yl.backend_shape",
                "Expected one syntax occurrence",
                Span::default(),
            ))
        }
    }
    pub fn list(&self) -> Result<&[Capture], Diagnostic> {
        if let Self::List(value) = self {
            Ok(value)
        } else {
            Err(Diagnostic::error(
                "yl.backend_shape",
                "Expected a syntax list",
                Span::default(),
            ))
        }
    }
}
/// Hosts supply signatures before any hook is invoked. Extra bindings are allowed.
#[derive(Clone, Debug)]
pub struct HookBinding {
    pub id: String,
    pub parameters: Vec<String>,
}
impl HookBinding {
    pub fn new(id: impl Into<String>, parameters: &[&str]) -> Self {
        Self {
            id: id.into(),
            parameters: parameters.iter().map(|s| (*s).into()).collect(),
        }
    }
}
/// Runtime values and emitted output belong entirely to the consumer.
pub trait Backend: Sized {
    type Value;
    fn bindings(&self) -> Vec<HookBinding>;
    fn invoke(
        &mut self,
        context: &mut BackendContext<'_, '_, Self>,
        hook: &HookContract,
        arguments: &[Capture],
    ) -> Result<Self::Value, Diagnostic>;
}
#[derive(Clone, Debug)]
pub struct DispatchOptions {
    pub max_calls: usize,
    pub max_depth: usize,
    pub cancelled: Option<Arc<AtomicBool>>,
}
impl Default for DispatchOptions {
    fn default() -> Self {
        Self {
            max_calls: 1_000_000,
            max_depth: 128,
            cancelled: None,
        }
    }
}
/// Immutable syntax and analysis, shared by interpreters and code generators.
pub struct BackendProgram<'language> {
    language: &'language CompiledLanguage,
    source: String,
    analysis: AnalysisResult,
    id: u64,
    occurrences: BTreeMap<usize, Vec<usize>>,
    terminals: BTreeMap<usize, Span>,
    root: SyntaxRef,
}
static NEXT_PROGRAM: AtomicU64 = AtomicU64::new(1);
impl CompiledLanguage {
    /// The host contracts stored in this language image. New compilation keeps
    /// only hooks referenced by entry-reachable backend mappings.
    pub fn required_hooks(&self) -> &[HookContract] {
        &self.executable_image().backend.hooks
    }
    pub fn prepare_backend(
        &self,
        file: &str,
        source: &str,
    ) -> Result<BackendProgram<'_>, Vec<Diagnostic>> {
        let analysis = SemanticEngine::default().analyze(self, file, source);
        if analysis.ast.is_none()
            || analysis
                .diagnostics
                .iter()
                .any(|d| d.severity == Severity::Error)
        {
            return Err(analysis.diagnostics);
        }
        let id = NEXT_PROGRAM.fetch_add(1, Ordering::Relaxed);
        let root = analysis
            .occurrences
            .first()
            .ok_or_else(|| {
                vec![Diagnostic::error(
                    "yl.backend_missing",
                    "Language has no backend occurrences",
                    Span::new(file, 0, source.len()),
                )]
            })?
            .id;
        let mut occurrences = BTreeMap::new();
        let mut terminals = BTreeMap::new();
        fn terminal(
            selection: &OccurrenceSelection,
            span: &Span,
            element_spans: &[Span],
            terminals: &mut BTreeMap<usize, Span>,
        ) {
            match selection {
                OccurrenceSelection::Terminal(id) => {
                    terminals.insert(*id, span.clone());
                }
                OccurrenceSelection::List(items) => {
                    for (index, item) in items.iter().enumerate() {
                        terminal(
                            item,
                            element_spans.get(index).unwrap_or(span),
                            &[],
                            terminals,
                        );
                    }
                }
                _ => {}
            }
        }
        fn collect(
            items: &[Occurrence],
            out: &mut BTreeMap<usize, Vec<usize>>,
            terminals: &mut BTreeMap<usize, Span>,
            path: &mut Vec<usize>,
        ) {
            for (index, item) in items.iter().enumerate() {
                path.push(index);
                for capture in item.captures.values() {
                    if let Some(selection) = capture.occurrence_selection() {
                        terminal(&selection, &capture.span, &capture.element_spans, terminals);
                    }
                }
                out.insert(item.id, path.clone());
                collect(&item.children, out, terminals, path);
                path.pop();
            }
        }
        collect(
            &analysis.occurrences,
            &mut occurrences,
            &mut terminals,
            &mut vec![],
        );
        Ok(BackendProgram {
            language: self,
            source: source.into(),
            analysis,
            id,
            occurrences,
            terminals,
            root: SyntaxRef {
                program: id,
                id: root,
                terminal: false,
            },
        })
    }
}
impl BackendProgram<'_> {
    fn occurrence(&self, id: usize) -> Option<&Occurrence> {
        let path = self.occurrences.get(&id)?;
        let mut children = &self.analysis.occurrences;
        let mut found = None;
        for index in path {
            let item = children.get(*index)?;
            found = Some(item);
            children = &item.children;
        }
        found
    }

    pub fn analysis(&self) -> &AnalysisResult {
        &self.analysis
    }
    pub fn root(&self) -> SyntaxRef {
        self.root
    }
    pub fn span(&self, syntax: SyntaxRef) -> Result<&Span, Diagnostic> {
        self.validate_handle(syntax)?;
        (if syntax.terminal {
            self.terminals.get(&syntax.id)
        } else {
            self.occurrence(syntax.id).map(|o| &o.span)
        })
        .ok_or_else(|| self.error("yl.backend_handle", "Invalid syntax handle", self.root))
    }
    pub fn text(&self, syntax: SyntaxRef) -> Result<&str, Diagnostic> {
        let span = self.span(syntax)?;
        self.source
            .get(span.start..span.end)
            .ok_or_else(|| self.error("yl.backend_handle", "Invalid syntax extent", syntax))
    }
    /// Inspect a capture without invoking any backend hook.
    pub fn capture(&self, syntax: SyntaxRef, name: &str) -> Result<Capture, Diagnostic> {
        self.validate_handle(syntax)?;
        let occurrence = if syntax.terminal {
            None
        } else {
            self.occurrence(syntax.id)
        };
        let occurrence = occurrence.ok_or_else(|| {
            self.error(
                "yl.backend_capture",
                "Terminal syntax has no captures",
                syntax,
            )
        })?;
        self.named_capture(occurrence, name)
    }
    /// Bridge a checked syntax handle to this program's static attachment identity.
    pub fn semantic_identity(
        &self,
        syntax: SyntaxRef,
    ) -> Result<crate::semantics::SemanticIdentity, Diagnostic> {
        self.validate_handle(syntax)?;
        Ok(if syntax.terminal {
            crate::semantics::SemanticIdentity::Service {
                service: "core/syntax".into(),
                id: syntax.id,
            }
        } else {
            crate::semantics::SemanticIdentity::Occurrence(syntax.id)
        })
    }
    pub fn definition(&self, syntax: SyntaxRef) -> Result<Option<&str>, Diagnostic> {
        self.validate_handle(syntax)?;
        Ok(if syntax.terminal {
            None
        } else {
            self.occurrence(syntax.id).map(|o| o.definition.as_str())
        })
    }
    fn validate_handle(&self, syntax: SyntaxRef) -> Result<(), Diagnostic> {
        if syntax.program != self.id
            || (if syntax.terminal {
                !self.terminals.contains_key(&syntax.id)
            } else {
                !self.occurrences.contains_key(&syntax.id)
            })
        {
            Err(self.error(
                "yl.backend_handle",
                "Syntax belongs to another program or is invalid",
                self.root,
            ))
        } else {
            Ok(())
        }
    }
    pub fn error(&self, code: &str, message: impl Into<String>, syntax: SyntaxRef) -> Diagnostic {
        let span = if syntax.program == self.id {
            (if syntax.terminal {
                self.terminals.get(&syntax.id).cloned()
            } else {
                self.occurrence(syntax.id).map(|o| o.span.clone())
            })
            .unwrap_or_default()
        } else {
            Span::default()
        };
        Diagnostic::error(code, message, span)
    }
    pub fn run<H: Backend>(&self, host: &mut H) -> Result<H::Value, Diagnostic> {
        self.run_with_options(host, DispatchOptions::default())
    }
    pub fn run_with_options<H: Backend>(
        &self,
        host: &mut H,
        options: DispatchOptions,
    ) -> Result<H::Value, Diagnostic> {
        let mut bindings = BTreeMap::new();
        for binding in host.bindings() {
            if bindings.insert(binding.id, binding.parameters).is_some() {
                return Err(self.error(
                    "yl.backend_binding",
                    "Duplicate host hook binding",
                    self.root,
                ));
            }
        }
        for hook in &self.language.executable_image().backend.hooks {
            match bindings.get(&hook.id) {
                None => {
                    return Err(self.error(
                        "yl.backend_binding",
                        format!("Missing host hook {}", hook.id),
                        self.root,
                    ))
                }
                Some(parameters) if parameters != &hook.parameters => {
                    return Err(self.error(
                        "yl.backend_binding",
                        format!("Incompatible host signature for {}", hook.id),
                        self.root,
                    ))
                }
                _ => {}
            }
        }
        let mut context = BackendContext {
            program: self,
            options,
            calls: 0,
            depth: 0,
            _host: std::marker::PhantomData,
        };
        context.dispatch(host, self.root)
    }
    fn named_capture(&self, occurrence: &Occurrence, name: &str) -> Result<Capture, Diagnostic> {
        if let Some(capture) = occurrence.captures.get(name) {
            self.capture_source(capture)
        } else {
            self.capture_source(&occurrence.capture_path(name)?)
        }
    }
    fn capture_source(&self, source: &SourceCapture) -> Result<Capture, Diagnostic> {
        fn select(program: &BackendProgram<'_>, selection: OccurrenceSelection) -> Capture {
            match selection {
                OccurrenceSelection::One(id) => Capture::Syntax(SyntaxRef {
                    program: program.id,
                    id,
                    terminal: false,
                }),
                OccurrenceSelection::Terminal(id) => Capture::Syntax(SyntaxRef {
                    program: program.id,
                    id,
                    terminal: true,
                }),
                OccurrenceSelection::List(items) => {
                    Capture::List(items.into_iter().map(|s| select(program, s)).collect())
                }
                OccurrenceSelection::Absent => Capture::Absent,
            }
        }
        source
            .occurrence_selection()
            .map(|s| select(self, s))
            .ok_or_else(|| {
                self.error(
                    "yl.backend_capture",
                    "Capture does not select a unique syntax occurrence; capture a named pattern",
                    self.root,
                )
            })
    }
}
/// A bounded dispatch session; the host explicitly chooses every child dispatch.
pub struct BackendContext<'program, 'language, H: Backend> {
    program: &'program BackendProgram<'language>,
    options: DispatchOptions,
    calls: usize,
    depth: usize,
    _host: std::marker::PhantomData<H>,
}
impl<H: Backend> BackendContext<'_, '_, H> {
    pub fn program(&self) -> &BackendProgram<'_> {
        self.program
    }
    pub fn dispatch(&mut self, host: &mut H, syntax: SyntaxRef) -> Result<H::Value, Diagnostic> {
        self.program.validate_handle(syntax)?;
        if self.calls >= self.options.max_calls || self.depth >= self.options.max_depth.min(128) {
            return Err(self.program.error(
                "yl.backend_limit",
                "Backend dispatch exceeded its call or depth budget",
                syntax,
            ));
        }
        if self
            .options
            .cancelled
            .as_ref()
            .is_some_and(|c| c.load(Ordering::Relaxed))
        {
            return Err(self.program.error(
                "yl.backend_cancelled",
                "Backend dispatch was cancelled",
                syntax,
            ));
        }
        self.calls += 1;
        self.depth += 1;
        let result = self.dispatch_inner(host, syntax);
        self.depth -= 1;
        result
    }
    fn dispatch_inner(&mut self, host: &mut H, syntax: SyntaxRef) -> Result<H::Value, Diagnostic> {
        let occurrence = (if syntax.terminal {
            None
        } else {
            self.program.occurrence(syntax.id)
        })
        .ok_or_else(|| {
            self.program.error(
                "yl.backend_missing",
                "Terminal syntax has no backend hook",
                syntax,
            )
        })?;
        let image = &self.program.language.executable_image().backend;
        if let Some(call) = image.calls.get(&occurrence.definition) {
            fn argument(
                program: &BackendProgram<'_>,
                occurrence: &Occurrence,
                arg: &ArgumentPlan,
            ) -> Result<Capture, Diagnostic> {
                match arg {
                    ArgumentPlan::SelfSyntax => Ok(Capture::Syntax(SyntaxRef {
                        program: program.id,
                        id: occurrence.id,
                        terminal: false,
                    })),
                    ArgumentPlan::Capture(name) => program.named_capture(occurrence, name),
                    ArgumentPlan::Tuple(items) => Ok(Capture::Tuple(
                        items
                            .iter()
                            .map(|a| argument(program, occurrence, a))
                            .collect::<Result<_, _>>()?,
                    )),
                    ArgumentPlan::List(items) => Ok(Capture::List(
                        items
                            .iter()
                            .map(|a| argument(program, occurrence, a))
                            .collect::<Result<_, _>>()?,
                    )),
                }
            }
            let hook = image.hooks[call.hook as usize].clone();
            let args = call
                .arguments
                .iter()
                .map(|a| argument(self.program, occurrence, a))
                .collect::<Result<Vec<_>, _>>()?;
            host.invoke(self, &hook, &args).map_err(|mut error| {
                if error.primary.file.is_empty() {
                    error.primary = Box::new(occurrence.span.clone());
                }
                error
            })
        } else {
            // Only abstract grammar nodes dispatch transparently to their selected member.
            let parser = &self.program.language.executable_image().parser;
            let abstract_node = parser.rule_id(&occurrence.definition).is_some_and(|id| {
                matches!(
                    parser.rules[id as usize].body,
                    crate::image::parser::ImageBody::Abstract { .. }
                )
            });
            if abstract_node && occurrence.children.len() == 1 {
                let child = SyntaxRef {
                    program: self.program.id,
                    id: occurrence.children[0].id,
                    terminal: false,
                };
                self.dispatch(host, child)
            } else {
                Err(self.program.error(
                    "yl.backend_missing",
                    format!("No backend hook for {}", occurrence.definition),
                    syntax,
                ))
            }
        }
    }
}
