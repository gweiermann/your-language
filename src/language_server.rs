//! YL authoring services. Native meanings are validated, never executed.
use crate::{
    diagnostic::{Diagnostic, Severity},
    examples::Preview,
    frontend::parse_yl,
    semantics::SemanticEngine,
    syntax::DeclKind,
    CompiledLanguage,
};
use lsp_server::{Connection, Message, Notification, Request, RequestId, Response};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Component, Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    thread,
};
use url::Url;

#[derive(Clone)]
struct Document {
    text: String,
    version: i64,
}
#[derive(Default)]
struct Workspace {
    documents: BTreeMap<String, Document>,
    entry: Option<String>,
    published: BTreeSet<String>,
    hints: BTreeMap<String, Vec<Value>>,
    lenses: BTreeMap<String, Vec<Value>>,
    display: ExampleDisplay,
    tokens: BTreeMap<String, Value>,
    compiled: BTreeMap<String, CachedLanguage>,
    programs: BTreeMap<String, CachedProgram>,
    cache_epoch: u64,
    #[cfg(test)]
    work: WorkCounts,
}
#[cfg(test)]
#[derive(Default, Debug, PartialEq)]
struct WorkCounts {
    compilations: usize,
    analyses: usize,
    highlights: usize,
}
// Retain a small working set across close/reopen without retaining analysis graphs.
const CACHE_ENTRIES: usize = 16;
struct CachedProgram {
    language: std::sync::Weak<CompiledLanguage>,
    source: String,
    diagnostics: Vec<Diagnostic>,
    tokens: Value,
    used: u64,
}
struct CachedLanguage {
    used: u64,
    sources: BTreeMap<String, String>,
    result: Result<Arc<CompiledLanguage>, Vec<Diagnostic>>,
}
// Standard LSP categories; themes own their colors.
const TOKEN_TYPES: &[&str] = crate::syntax::HIGHLIGHT_TYPES;
#[derive(Default, PartialEq)]
enum ExampleDisplay {
    #[default]
    Above,
    Inline,
    Off,
}
fn example_display(options: &Value) -> ExampleDisplay {
    match options["exampleDisplay"].as_str() {
        Some("inline") => ExampleDisplay::Inline,
        Some("off") => ExampleDisplay::Off,
        _ => ExampleDisplay::Above,
    }
}
fn example_title(text: &str) -> String {
    let line = text
        .replace('\r', "\\r")
        .replace('\n', "\\n")
        .replace('\t', "\\t");
    let mut title: String = line.chars().take(120).collect();
    if line.chars().count() > 120 {
        title.push('…');
    }
    format!("example: {title}")
}

/// Start a stdio LSP session. Stdout is reserved for framed protocol messages.
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let (connection, threads) = Connection::stdio();
    let (id, params) = connection.initialize_start()?;
    let params: Value = serde_json::from_value(params)?;
    let refresh_hints = params["capabilities"]["workspace"]["inlayHint"]["refreshSupport"] == true;
    let refresh_lenses = params["capabilities"]["workspace"]["codeLens"]["refreshSupport"] == true;
    let refresh_tokens =
        params["capabilities"]["workspace"]["semanticTokens"]["refreshSupport"] == true;
    connection.initialize_finish(
        id,
        json!({"capabilities": {
        "positionEncoding":"utf-16", "textDocumentSync":{"openClose":true,"change":1},
        "documentFormattingProvider":true, "inlayHintProvider":true, "codeLensProvider":{"resolveProvider":false},
        "semanticTokensProvider":{"legend":{"tokenTypes":TOKEN_TYPES,"tokenModifiers":[]},"full":true}
    },"serverInfo":{"name":"Your Language","version":env!("CARGO_PKG_VERSION")}}),
    )?;
    let mut workspace = Workspace {
        entry: entry_option(&params["initializationOptions"]),
        display: example_display(&params["initializationOptions"]),
        ..Workspace::default()
    };
    let revision = Arc::new(AtomicU64::new(0));
    let cancelled = Arc::new(Mutex::new(BTreeSet::<RequestId>::new()));
    let (sender, receiver) = std::sync::mpsc::channel();
    let reader_revision = revision.clone();
    let reader_cancelled = cancelled.clone();
    let protocol_sender = connection.sender;
    let reader = thread::spawn(move || {
        for message in connection.receiver {
            let changed = matches!(&message, Message::Notification(n) if matches!(n.method.as_str(),
                "textDocument/didOpen" | "textDocument/didChange" | "textDocument/didClose" | "workspace/didChangeWatchedFiles" | "workspace/didChangeConfiguration"));
            let epoch = if changed {
                reader_revision.fetch_add(1, Ordering::SeqCst) + 1
            } else {
                reader_revision.load(Ordering::SeqCst)
            };
            if let Message::Notification(n) = &message {
                if n.method == "$/cancelRequest" {
                    if let Ok(id) = serde_json::from_value(n.params["id"].clone()) {
                        if let Ok(mut ids) = reader_cancelled.lock() {
                            if ids.len() >= 1024 {
                                ids.clear();
                            }
                            ids.insert(id);
                        }
                    }
                }
            }
            if sender.send((epoch, message)).is_err() {
                break;
            }
        }
    });
    let mut shutdown = false;
    let mut exited = false;
    let mut state_epoch = 0;
    while let Ok((epoch, message)) = receiver.recv() {
        match message {
            Message::Notification(n) => {
                if n.method == "exit" {
                    exited = true;
                    break;
                }
                if shutdown {
                    continue;
                }
                if workspace.update(&n) {
                    state_epoch = epoch;
                    if epoch == revision.load(Ordering::SeqCst) {
                        let published = workspace.published.clone();
                        let notifications = workspace.analyze();
                        if epoch == revision.load(Ordering::SeqCst) {
                            for n in notifications {
                                protocol_sender.send(Message::Notification(n))?;
                            }
                            if refresh_hints {
                                protocol_sender.send(Message::Request(Request::new(
                                    RequestId::from(format!("yl-hints-{epoch}")),
                                    "workspace/inlayHint/refresh".into(),
                                    Value::Null,
                                )))?;
                            }
                            if refresh_lenses {
                                protocol_sender.send(Message::Request(Request::new(
                                    RequestId::from(format!("yl-lenses-{epoch}")),
                                    "workspace/codeLens/refresh".into(),
                                    Value::Null,
                                )))?;
                            }
                            if refresh_tokens {
                                protocol_sender.send(Message::Request(Request::new(
                                    RequestId::from(format!("yl-tokens-{epoch}")),
                                    "workspace/semanticTokens/refresh".into(),
                                    Value::Null,
                                )))?;
                            }
                        } else {
                            workspace.published = published;
                            workspace.hints.clear();
                            workspace.lenses.clear();
                            workspace.tokens.clear();
                        }
                    }
                }
            }
            Message::Request(request) => {
                let id = request.id.clone();
                let mut response = if request.method == "shutdown" {
                    shutdown = true;
                    Response::new_ok(id.clone(), Value::Null)
                } else if shutdown {
                    Response::new_err(id.clone(), -32600, "Server has shut down".into())
                } else {
                    match workspace.request(&request.method, &request.params) {
                        Ok(value) => Response::new_ok(id.clone(), value),
                        Err((code, message)) => Response::new_err(id.clone(), code, message),
                    }
                };
                if cancelled
                    .lock()
                    .map(|mut ids| ids.remove(&id))
                    .unwrap_or(false)
                {
                    response = Response::new_err(id, -32800, "Request cancelled".into());
                } else if !shutdown && state_epoch != revision.load(Ordering::SeqCst) {
                    response =
                        Response::new_err(id, -32801, "Document changed during request".into());
                }
                protocol_sender.send(Message::Response(response))?;
            }
            Message::Response(_) => {}
        }
    }
    drop(protocol_sender);
    reader.join().map_err(|_| "Protocol reader failed")?;
    threads.join()?;
    if exited && !shutdown {
        Err("exit before shutdown".into())
    } else {
        Ok(())
    }
}

fn entry_option(options: &Value) -> Option<String> {
    options["entry"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(|s| {
            Url::parse(s)
                .ok()
                .filter(|u| u.scheme() == "file")
                .map(|u| u.to_string())
                .unwrap_or_else(|| file_uri(Path::new(s)).unwrap_or_else(|| s.into()))
        })
}
fn file_uri(path: &Path) -> Option<String> {
    Url::from_file_path(normalize(path))
        .ok()
        .map(|u| u.to_string())
}
fn file_path(uri: &str) -> Option<PathBuf> {
    Url::parse(uri)
        .ok()?
        .to_file_path()
        .ok()
        .map(|p| normalize(&p))
}
fn same_path(left: &Path, right: &Path) -> bool {
    if cfg!(windows) {
        left.to_string_lossy()
            .eq_ignore_ascii_case(&right.to_string_lossy())
    } else {
        left == right
    }
}
fn normalize(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !result.pop() {
                    result.push("..");
                }
            }
            other => result.push(other.as_os_str()),
        }
    }
    result
}
fn relative(path: &Path, root: &Path) -> String {
    let a: Vec<_> = path.components().collect();
    let b: Vec<_> = root.components().collect();
    let shared = a.iter().zip(&b).take_while(|(a, b)| a == b).count();
    let mut p = PathBuf::new();
    for _ in shared..b.len() {
        p.push("..");
    }
    for c in &a[shared..] {
        p.push(c.as_os_str());
    }
    p.to_string_lossy().replace('\\', "/")
}
/// Convert a UTF-8 source byte offset to the LSP UTF-16 position. CRLF counts as one newline.
fn position(source: &str, offset: usize) -> Value {
    let mut end = offset.min(source.len());
    while !source.is_char_boundary(end) {
        end -= 1;
    }
    let prefix = &source[..end];
    let line = prefix.bytes().filter(|b| *b == b'\n').count();
    let tail = prefix.rsplit('\n').next().unwrap_or("");
    json!({"line":line,"character":tail.trim_end_matches('\r').encode_utf16().count()})
}
fn range(source: &str, start: usize, end: usize) -> Value {
    json!({"start":position(source,start),"end":position(source,end)})
}
fn is_yl(uri: &str) -> bool {
    uri.starts_with("untitled:")
        || file_path(uri)
            .is_some_and(|path| path.extension().is_some_and(|extension| extension == "yl"))
}
fn semantic_tokens(source: &str, spans: Vec<crate::HighlightSpan>) -> Value {
    let mut tokens = Vec::new();
    for highlight in spans {
        let Some(kind) = TOKEN_TYPES
            .iter()
            .position(|kind| *kind == highlight.token_type)
        else {
            continue;
        };
        if source
            .get(highlight.span.start..highlight.span.end)
            .is_none()
        {
            continue;
        }
        let mut start = highlight.span.start;
        for line in source[highlight.span.start..highlight.span.end].split_inclusive('\n') {
            let text = line.trim_end_matches(['\r', '\n']);
            if !text.is_empty() {
                let position = position(source, start);
                tokens.push((
                    position["line"].as_u64().unwrap_or(0),
                    position["character"].as_u64().unwrap_or(0),
                    text.encode_utf16().count() as u64,
                    kind,
                ));
            }
            start += line.len();
        }
    }
    tokens.sort_unstable();
    let mut data = Vec::new();
    let (mut previous_line, mut previous_character) = (0, 0);
    for (line, character, length, kind) in tokens {
        data.extend([
            line - previous_line,
            if line == previous_line {
                character - previous_character
            } else {
                character
            },
            length,
            kind as u64,
            0,
        ]);
        previous_line = line;
        previous_character = character;
    }
    json!({"data":data})
}
fn notification(method: &str, params: Value) -> Notification {
    Notification::new(method.into(), params)
}
fn trim_cache<T>(cache: &mut BTreeMap<String, T>, used: impl Fn(&T) -> u64) {
    while cache.len() > CACHE_ENTRIES {
        let oldest = cache
            .iter()
            .min_by_key(|(_, value)| used(value))
            .map(|(key, _)| key.clone());
        if let Some(oldest) = oldest {
            cache.remove(&oldest);
        }
    }
}

impl Workspace {
    fn uri_for_path(&self, path: &Path) -> Option<String> {
        self.documents
            .keys()
            .find(|uri| file_path(uri).is_some_and(|p| same_path(&p, path)))
            .cloned()
            .or_else(|| file_uri(path))
    }
    fn update(&mut self, notification: &Notification) -> bool {
        let p = &notification.params;
        let d = &p["textDocument"];
        match notification.method.as_str() {
            "textDocument/didOpen" => {
                if let (Some(uri), Some(text), Some(version)) =
                    (d["uri"].as_str(), d["text"].as_str(), d["version"].as_i64())
                {
                    self.documents.insert(
                        uri.into(),
                        Document {
                            text: text.into(),
                            version,
                        },
                    );
                }
            }
            "textDocument/didChange" => {
                if let (Some(uri), Some(text), Some(version)) = (
                    d["uri"].as_str(),
                    p["contentChanges"]
                        .as_array()
                        .and_then(|a| a.last())
                        .and_then(|c| c["text"].as_str()),
                    d["version"].as_i64(),
                ) {
                    if let Some(doc) = self.documents.get_mut(uri) {
                        if version > doc.version {
                            doc.text = text.into();
                            doc.version = version;
                        }
                    }
                }
            }
            "textDocument/didClose" => {
                if let Some(uri) = d["uri"].as_str() {
                    self.documents.remove(uri);
                }
            }
            "workspace/didChangeConfiguration" => {
                self.entry = entry_option(&p["settings"]);
                self.display = example_display(&p["settings"]);
            }
            "workspace/didChangeWatchedFiles" => {}
            _ => return false,
        }
        self.hints.clear();
        self.lenses.clear();
        self.tokens.clear();
        true
    }
    fn source(&self, uri: &str) -> Option<String> {
        self.documents
            .get(uri)
            .map(|d| d.text.clone())
            .or_else(|| {
                file_path(uri)
                    .and_then(|path| self.uri_for_path(&path))
                    .and_then(|key| self.documents.get(&key).map(|d| d.text.clone()))
            })
            .or_else(|| std::fs::read_to_string(file_path(uri)?).ok())
    }
    fn analyze(&mut self) -> Vec<Notification> {
        self.cache_epoch = self.cache_epoch.wrapping_add(1);
        self.hints.clear();
        self.lenses.clear();
        self.tokens.clear();
        let mut sources: BTreeMap<String, String> = self
            .documents
            .iter()
            .map(|(uri, d)| (uri.clone(), d.text.clone()))
            .collect();
        let mut diagnostics: BTreeMap<String, Vec<Value>> =
            self.documents.keys().map(|u| (u.clone(), vec![])).collect();
        let mut roots = BTreeSet::new();
        let mut programs: BTreeMap<String, Vec<String>> = BTreeMap::new();
        if let Some(entry) = &self.entry {
            roots.insert(entry.clone());
        }
        for (uri, doc) in &self.documents {
            if file_path(uri).is_some_and(|p| p.file_name().is_some_and(|n| n == "yl.json")) {
                let path = file_path(uri).expect("file URI");
                if let Err(errors) = crate::project::Project::parse(&path, &doc.text) {
                    self.add_path_diagnostics(&errors, &mut sources, &mut diagnostics);
                }
                continue;
            }
            if !is_yl(uri) {
                if let Some(path) = file_path(uri) {
                    match crate::project::Project::discover_with(&path, |path| {
                        self.uri_for_path(path).and_then(|uri| self.source(&uri))
                    }) {
                        Ok(Some(project)) if project.matches(&path) => {
                            if let Some(entry) = self.uri_for_path(&project.entry) {
                                roots.insert(entry.clone());
                                programs.entry(entry).or_default().push(uri.clone());
                            }
                        }
                        Err(errors) => {
                            self.add_path_diagnostics(&errors, &mut sources, &mut diagnostics)
                        }
                        _ => {}
                    }
                }
                continue;
            }
            match parse_yl(uri, &doc.text) {
                Ok(module) => {
                    if module
                        .declarations
                        .iter()
                        .any(|d| matches!(d.kind, DeclKind::Entry { .. }))
                    {
                        roots.insert(uri.clone());
                    }
                }
                Err(errors) => self.add_diagnostics(&errors, &sources, &mut diagnostics),
            }
        }
        for entry_uri in roots {
            if self.source(&entry_uri).is_none() {
                diagnostics
                    .entry(entry_uri.clone())
                    .or_default()
                    .push(json!({
                        "range":range("",0,0),"severity":1,"code":"yl.io","source":"YL",
                        "message":"Cannot read the configured entry module"
                    }));
                continue;
            }
            let Some(entry_path) = file_path(&entry_uri) else {
                continue;
            };
            let Some(root) = entry_path.parent() else {
                continue;
            };
            let mut graph = BTreeMap::new();
            let mut paths = BTreeMap::new();
            let mut queue = vec![entry_path.clone()];
            while let Some(path) = queue.pop() {
                let id = relative(&path, root);
                if graph.contains_key(&id) {
                    continue;
                }
                let Some(uri) = self.uri_for_path(&path) else {
                    continue;
                };
                let Some(text) = self.source(&uri) else {
                    continue;
                };
                if let Ok(module) = parse_yl(&id, &text) {
                    for declaration in module.declarations {
                        if let DeclKind::Import { path: import, .. } = declaration.kind {
                            if import.starts_with("std/") || import.starts_with("core/") {
                                continue;
                            }
                            let mut target = normalize(&path.parent().unwrap_or(root).join(import));
                            if target.extension().is_none() {
                                target.set_extension("yl");
                            }
                            queue.push(target);
                        }
                    }
                }
                sources.insert(uri.clone(), text.clone());
                paths.insert(id.clone(), uri);
                graph.insert(id, text);
            }
            let entry_id = relative(&entry_path, root);
            let needs_compile = self
                .compiled
                .get(&entry_uri)
                .is_none_or(|cached| cached.sources != graph);
            if needs_compile {
                #[cfg(test)]
                {
                    self.work.compilations += 1;
                }
                let result = SemanticEngine::default()
                    .compile(&entry_id, &graph)
                    .map(Arc::new);
                self.compiled.insert(
                    entry_uri.clone(),
                    CachedLanguage {
                        used: self.cache_epoch,
                        sources: graph,
                        result,
                    },
                );
            }
            let cached = self.compiled.get_mut(&entry_uri).expect("compiled entry");
            cached.used = self.cache_epoch;
            let result = cached.result.clone();
            match result {
                Ok(language) => {
                    let errors = language
                        .diagnostics
                        .iter()
                        .map(|d| remap(d, &paths))
                        .collect::<Vec<_>>();
                    self.add_diagnostics(&errors, &sources, &mut diagnostics);
                    for uri in programs.get(&entry_uri).into_iter().flatten() {
                        let Some(source) = sources.get(uri) else {
                            continue;
                        };
                        let reusable = self.programs.get(uri).is_some_and(|cached| {
                            cached.source == *source
                                && cached
                                    .language
                                    .upgrade()
                                    .is_some_and(|previous| Arc::ptr_eq(&previous, &language))
                        });
                        if !reusable {
                            #[cfg(test)]
                            {
                                self.work.analyses += 1;
                                self.work.highlights += 1;
                            }
                            let analysis =
                                SemanticEngine::default().analyze(&language, uri, source);
                            let tokens =
                                semantic_tokens(source, language.highlight_named(uri, source));
                            self.programs.insert(
                                uri.clone(),
                                CachedProgram {
                                    language: Arc::downgrade(&language),
                                    source: source.clone(),
                                    diagnostics: analysis.diagnostics,
                                    tokens,
                                    used: self.cache_epoch,
                                },
                            );
                        }
                        let cached = self.programs.get_mut(uri).expect("analyzed program");
                        cached.used = self.cache_epoch;
                        let errors = cached
                            .diagnostics
                            .iter()
                            .map(|d| remap(d, &paths))
                            .collect::<Vec<_>>();
                        self.tokens.insert(uri.clone(), cached.tokens.clone());
                        self.add_diagnostics(&errors, &sources, &mut diagnostics);
                    }
                    for Preview {
                        definition,
                        span,
                        text,
                        ..
                    } in language.example_previews()
                    {
                        let Some(text) = text else { continue };
                        let Some(uri) = paths.get(&span.file) else {
                            continue;
                        };
                        let Some(source) = sources.get(uri) else {
                            continue;
                        };
                        let hint = json!({"position":position(source,span.end),"label":format!(" example: {}",json!(text)),"kind":1,"paddingLeft":true,"tooltip":format!("Parser-validated example for {definition}. Optional parts omitted; * and + contain one item.")});
                        let hints = self.hints.entry(uri.clone()).or_default();
                        if !hints.contains(&hint) {
                            hints.push(hint);
                        }
                        let lens = json!({"range":range(source,span.start,span.start),"command":{
                            "title":example_title(&text),"command":"yourLanguage.copyExample","arguments":[text]
                        }});
                        let lenses = self.lenses.entry(uri.clone()).or_default();
                        if !lenses.contains(&lens) {
                            lenses.push(lens);
                        }
                    }
                }
                Err(errors) => {
                    let errors = errors.iter().map(|d| remap(d, &paths)).collect::<Vec<_>>();
                    self.add_diagnostics(&errors, &sources, &mut diagnostics);
                }
            }
        }
        // Empty roots are common while switching files. Keep recent immutable
        // languages and final document results, but cap both retained sets.
        trim_cache(&mut self.compiled, |cached| cached.used);
        trim_cache(&mut self.programs, |cached| cached.used);
        let now: BTreeSet<_> = diagnostics.keys().cloned().collect();
        for old in self.published.difference(&now) {
            diagnostics.insert(old.clone(), vec![]);
        }
        self.published = now;
        diagnostics
            .into_iter()
            .map(|(uri, diagnostics)| {
                let mut params = json!({"uri":uri,"diagnostics":diagnostics});
                if let Some(doc) = self.documents.get(&uri) {
                    params["version"] = json!(doc.version);
                }
                notification("textDocument/publishDiagnostics", params)
            })
            .collect()
    }
    fn add_diagnostics(
        &self,
        errors: &[Diagnostic],
        sources: &BTreeMap<String, String>,
        output: &mut BTreeMap<String, Vec<Value>>,
    ) {
        for error in errors {
            if !error.annotations.is_empty() {
                for (uri, item) in grouped_diagnostic_publications(error, sources) {
                    let items = output.entry(uri).or_default();
                    if !items.contains(&item) {
                        items.push(item);
                    }
                }
                continue;
            }
            let uri = &error.primary.file;
            let Some(source) = sources.get(uri) else {
                continue;
            };
            let related:Vec<_>=error.secondary.iter().filter_map(|s| sources.get(&s.file).map(|text| json!({"location":{"uri":s.file,"range":range(text,s.start,s.end)},"message":"Related definition"}))).collect();
            let d = json!({"range":range(source,error.primary.start,error.primary.end),"severity":match error.severity{Severity::Error=>1,Severity::Warning=>2,Severity::Help=>3},"code":error.code,"source":"YL","message":error.message,"relatedInformation":related});
            let items = output.entry(uri.clone()).or_default();
            if !items.contains(&d) {
                items.push(d);
            }
        }
    }
    fn add_path_diagnostics(
        &self,
        errors: &[Diagnostic],
        sources: &mut BTreeMap<String, String>,
        output: &mut BTreeMap<String, Vec<Value>>,
    ) {
        let paths: BTreeMap<_, _> = errors
            .iter()
            .filter_map(|error| {
                self.uri_for_path(Path::new(&error.primary.file))
                    .map(|uri| (error.primary.file.clone(), uri))
            })
            .collect();
        for uri in paths.values() {
            if let Some(text) = self.source(uri) {
                sources.insert(uri.clone(), text);
            }
        }
        self.add_diagnostics(
            &errors
                .iter()
                .map(|error| remap(error, &paths))
                .collect::<Vec<_>>(),
            sources,
            output,
        );
    }
    fn request(&self, method: &str, params: &Value) -> Result<Value, (i32, String)> {
        let uri = params["textDocument"]["uri"].as_str().unwrap_or("");
        match method {
            "yl/project" => {
                let project = file_path(uri).and_then(|path| {
                    crate::project::Project::discover_with(&path, |path| {
                        self.uri_for_path(path).and_then(|uri| self.source(&uri))
                    })
                    .ok()
                    .flatten()
                    .filter(|project| project.matches(&path))
                });
                Ok(json!({"matches":project.is_some()}))
            }
            "textDocument/semanticTokens/full" => Ok(self
                .tokens
                .get(uri)
                .cloned()
                .unwrap_or_else(|| json!({"data":[]}))),
            "textDocument/formatting" => {
                if !is_yl(uri) {
                    return Ok(Value::Null);
                }
                let source = self
                    .source(uri)
                    .ok_or((-32602, "Unknown document".into()))?;
                let text = crate::format_yl(uri, &source).map_err(|errors| {
                    (
                        -32602,
                        errors
                            .iter()
                            .map(|d| d.message.as_str())
                            .collect::<Vec<_>>()
                            .join("; "),
                    )
                })?;
                Ok(json!([{"range":range(&source,0,source.len()),"newText":text}]))
            }
            "textDocument/inlayHint" => {
                if self.display != ExampleDisplay::Inline {
                    return Ok(json!([]));
                }
                let start = &params["range"]["start"];
                let end = &params["range"]["end"];
                let key = |p: &Value| {
                    (
                        p["line"].as_u64().unwrap_or(0),
                        p["character"].as_u64().unwrap_or(0),
                    )
                };
                Ok(json!(self
                    .hints
                    .get(uri)
                    .into_iter()
                    .flatten()
                    .filter(|hint| key(&hint["position"]) >= key(start)
                        && key(&hint["position"]) <= key(end))
                    .collect::<Vec<_>>()))
            }
            "textDocument/codeLens" => {
                if self.display != ExampleDisplay::Above {
                    return Ok(json!([]));
                }
                Ok(json!(self.lenses.get(uri).cloned().unwrap_or_default()))
            }
            _ => Err((-32601, format!("Unsupported method {method}"))),
        }
    }
}
/// LSP has one range per diagnostic. Publish linked entries so every annotation
/// gets a squiggle, retaining the single engine diagnostic's identity in `data`.
fn grouped_diagnostic_publications(
    error: &Diagnostic,
    sources: &BTreeMap<String, String>,
) -> Vec<(String, Value)> {
    use std::hash::{Hash, Hasher};
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    error.code.hash(&mut hash);
    error.message.hash(&mut hash);
    for annotation in error.annotations.iter() {
        annotation.span.file.hash(&mut hash);
        annotation.span.start.hash(&mut hash);
        annotation.span.end.hash(&mut hash);
        annotation.label.hash(&mut hash);
    }
    let group = format!("{:016x}", hash.finish());
    error.annotations.iter().enumerate().filter_map(|(index, annotation)| {
        let source = sources.get(&annotation.span.file)?;
        let related_annotations = if index == 0 { &error.annotations[1..] } else { &error.annotations[..1] };
        let related: Vec<_> = related_annotations.iter().filter_map(|other| {
            let source = sources.get(&other.span.file)?;
            Some(json!({"location":{"uri":other.span.file,"range":range(source,other.span.start,other.span.end)},"message":other.label.as_deref().unwrap_or(&error.message)}))
        }).collect();
        let message = annotation.label.as_ref().map_or_else(||error.message.clone(), |label|format!("{}\n{label}",error.message));
        Some((annotation.span.file.clone(), json!({"range":range(source,annotation.span.start,annotation.span.end),"severity":match error.severity{Severity::Error=>1,Severity::Warning=>2,Severity::Help=>3},"code":error.code,"source":"YL","message":message,"relatedInformation":related,"data":{"group":group,"primary":index == 0}})))
    }).collect()
}
fn remap(diagnostic: &Diagnostic, paths: &BTreeMap<String, String>) -> Diagnostic {
    let mut d = diagnostic.clone();
    if let Some(uri) = paths.get(&d.primary.file) {
        d.primary.file = uri.clone();
    }
    for span in &mut d.secondary {
        if let Some(uri) = paths.get(&span.file) {
            span.file = uri.clone();
        }
    }
    for annotation in d.annotations.iter_mut() {
        if let Some(uri) = paths.get(&annotation.span.file) {
            annotation.span.file = uri.clone();
        }
    }
    d
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn grouped_diagnostics_publish_each_highlight_with_shared_identity() {
        use crate::diagnostic::{DiagnosticAnnotation, Span};
        let mut diagnostic = Diagnostic::error(
            "types.mismatch",
            "Incompatible types",
            Span::new("file:///a", 0, 2),
        );
        diagnostic.annotations = Box::new(vec![
            DiagnosticAnnotation {
                span: Span::new("file:///a", 0, 2),
                label: Some("value".into()),
            },
            DiagnosticAnnotation {
                span: Span::new("file:///b", 3, 4),
                label: None,
            },
        ]);
        let sources = BTreeMap::from([
            ("file:///a".into(), "éx".into()),
            ("file:///b".into(), "let x".into()),
        ]);
        let publications = grouped_diagnostic_publications(&diagnostic, &sources);
        assert_eq!(publications.len(), 2);
        assert_eq!(publications[0].1["range"]["end"]["character"], 1);
        assert_eq!(publications[1].1["range"]["start"]["character"], 3);
        assert_eq!(
            publications[0].1["data"]["group"],
            publications[1].1["data"]["group"]
        );
        assert_eq!(publications[0].1["data"]["primary"], true);
        assert_eq!(publications[1].1["data"]["primary"], false);
        assert_eq!(
            publications[0].1["relatedInformation"][0]["location"]["uri"],
            "file:///b"
        );
        assert_eq!(publications[1].1["message"], "Incompatible types");
    }
    #[test]
    fn unicode_positions() {
        assert_eq!(position("a😀\r\nx", 5), json!({"line":0,"character":3}));
        assert_eq!(position("a😀\r\nx", 8), json!({"line":1,"character":1}));
    }
    #[test]
    fn stale_document_versions_are_ignored() {
        let mut w = Workspace::default();
        w.update(&notification(
            "textDocument/didOpen",
            json!({"textDocument":{"uri":"untitled:a","text":"x","version":3}}),
        ));
        w.update(&notification("textDocument/didChange",json!({"textDocument":{"uri":"untitled:a","version":2},"contentChanges":[{"text":"bad"}]})));
        assert_eq!(w.documents["untitled:a"].text, "x");
    }
    #[test]
    fn unchanged_programs_share_completed_diagnostics_and_tokens() {
        let directory = std::env::temp_dir().join("yl-program-cache-unit-test");
        let entry = file_uri(&directory.join("language.yl")).unwrap();
        let config = file_uri(&directory.join("yl.json")).unwrap();
        let first = file_uri(&directory.join("first.txt")).unwrap();
        let second = file_uri(&directory.join("second.txt")).unwrap();
        let mut workspace = Workspace::default();
        for (uri, text) in [
            (
                &entry,
                "node Root = /[0-9]+/ { ide { highlightAs \"number\" } } entry Root",
            ),
            (&config, r#"{"entry":"language.yl","files":["*.txt"]}"#),
            (&first, "42"),
        ] {
            workspace.update(&notification(
                "textDocument/didOpen",
                json!({"textDocument":{"uri":uri,"text":text,"version":1}}),
            ));
        }
        workspace.analyze();
        let tokens = workspace.tokens[&first].clone();
        workspace.update(&notification(
            "textDocument/didOpen",
            json!({"textDocument":{"uri":second,"text":"43","version":1}}),
        ));
        workspace.analyze();
        assert_eq!(
            workspace.work,
            WorkCounts {
                compilations: 1,
                analyses: 2,
                highlights: 2
            }
        );
        assert_eq!(workspace.tokens[&first], tokens);
        workspace.update(&notification(
            "textDocument/didChange",
            json!({"textDocument":{"uri":first,"version":2},"contentChanges":[{"text":"42"}]}),
        ));
        let publications = workspace.analyze();
        assert_eq!(workspace.work.analyses, 2);
        assert!(publications
            .iter()
            .any(|p| p.params["uri"] == first && p.params["version"] == 2));
        workspace.update(&notification(
            "textDocument/didChange",
            json!({"textDocument":{"uri":first,"version":3},"contentChanges":[{"text":"?"}]}),
        ));
        let publications = workspace.analyze();
        assert_eq!(workspace.work.analyses, 3);
        assert!(publications
            .iter()
            .any(|p| p.params["uri"] == first
                && !p.params["diagnostics"].as_array().unwrap().is_empty()));
        workspace.update(&notification("textDocument/didChange",
            json!({"textDocument":{"uri":entry,"version":2},"contentChanges":[{"text":"node Root = /[a-z]+/ entry Root"}]})));
        workspace.analyze();
        assert_eq!(
            workspace.work,
            WorkCounts {
                compilations: 2,
                analyses: 5,
                highlights: 5
            }
        );
    }
    #[test]
    fn close_reopen_reuses_results_and_disk_import_changes_invalidate_them() {
        let directory =
            std::env::temp_dir().join(format!("yl-reopen-cache-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            directory.join("yl.json"),
            r#"{"entry":"language.yl","files":["*.txt"]}"#,
        )
        .unwrap();
        std::fs::write(
            directory.join("language.yl"),
            "import { Item } from \"./item\" node Root = Item entry Root",
        )
        .unwrap();
        std::fs::write(
            directory.join("item.yl"),
            "export pattern Item = /[0-9]+/ { ide { highlightAs \"number\" } }",
        )
        .unwrap();
        let uri = file_uri(&directory.join("program.txt")).unwrap();
        let mut workspace = Workspace::default();
        let open = |version| {
            notification(
                "textDocument/didOpen",
                json!({"textDocument":{"uri":uri,"text":"42","version":version}}),
            )
        };
        workspace.update(&open(1));
        workspace.analyze();
        let initial_tokens = workspace.tokens[&uri].clone();
        workspace.update(&notification(
            "textDocument/didClose",
            json!({"textDocument":{"uri":uri}}),
        ));
        workspace.analyze();
        assert!(workspace.tokens.is_empty());
        workspace.update(&open(2));
        workspace.analyze();
        assert_eq!(
            workspace.work,
            WorkCounts {
                compilations: 1,
                analyses: 1,
                highlights: 1
            }
        );
        assert_eq!(workspace.tokens[&uri], initial_tokens);
        std::fs::write(directory.join("item.yl"), "export pattern Item = /[a-z]+/").unwrap();
        workspace.update(&notification(
            "workspace/didChangeWatchedFiles",
            json!({"changes":[]}),
        ));
        let publications = workspace.analyze();
        assert_eq!(
            workspace.work,
            WorkCounts {
                compilations: 2,
                analyses: 2,
                highlights: 2
            }
        );
        assert!(publications
            .iter()
            .any(|p| p.params["uri"] == uri
                && !p.params["diagnostics"].as_array().unwrap().is_empty()));
        assert_eq!(workspace.tokens[&uri], json!({"data":[]}));
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn retained_caches_evict_least_recently_used_entries() {
        let mut cache = (0..CACHE_ENTRIES + 3)
            .map(|index| (index.to_string(), index as u64))
            .collect();
        trim_cache(&mut cache, |used| *used);
        assert_eq!(cache.len(), CACHE_ENTRIES);
        assert!(!cache.contains_key("0"));
        assert!(!cache.contains_key("2"));
        assert!(cache.contains_key("3"));
    }
    #[test]
    fn program_edits_reuse_compiled_graph_but_definition_edits_invalidate_it() {
        let directory = std::env::temp_dir().join("yl-cache-unit-test");
        let entry = file_uri(&directory.join("language.yl")).unwrap();
        let config = file_uri(&directory.join("yl.json")).unwrap();
        let program = file_uri(&directory.join("program.txt")).unwrap();
        let mut workspace = Workspace::default();
        for (uri, text) in [
            (&entry, "node Root = /[0-9]+/ entry Root"),
            (&config, r#"{"entry":"language.yl","files":["*.txt"]}"#),
            (&program, "42"),
        ] {
            workspace.update(&notification(
                "textDocument/didOpen",
                json!({"textDocument":{"uri":uri,"text":text,"version":1}}),
            ));
        }
        workspace.analyze();
        let first = workspace.compiled[&entry].result.as_ref().unwrap().clone();
        workspace.update(&notification(
            "textDocument/didChange",
            json!({"textDocument":{"uri":program,"version":2},"contentChanges":[{"text":"43"}]}),
        ));
        workspace.analyze();
        assert!(Arc::ptr_eq(
            &first,
            workspace.compiled[&entry].result.as_ref().unwrap()
        ));
        workspace.update(&notification("textDocument/didChange",json!({"textDocument":{"uri":entry,"version":2},"contentChanges":[{"text":"node Root = /[a-z]+/ entry Root"}]})));
        workspace.analyze();
        assert!(!Arc::ptr_eq(
            &first,
            workspace.compiled[&entry].result.as_ref().unwrap()
        ));
    }
}
