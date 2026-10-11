//! YL authoring services. Native meanings are validated, never executed.
use crate::{
    diagnostic::{Diagnostic, Severity},
    examples::Preview,
    frontend::parse_yl,
    semantics::SemanticEngine,
    syntax::DeclKind,
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
}

/// Start a stdio LSP session. Stdout is reserved for framed protocol messages.
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let (connection, threads) = Connection::stdio();
    let (id, params) = connection.initialize_start()?;
    let params: Value = serde_json::from_value(params)?;
    let refresh_hints = params["capabilities"]["workspace"]["inlayHint"]["refreshSupport"] == true;
    connection.initialize_finish(
        id,
        json!({"capabilities": {
        "positionEncoding":"utf-16", "textDocumentSync":{"openClose":true,"change":1},
        "documentFormattingProvider":true, "inlayHintProvider":true
    },"serverInfo":{"name":"Your Language","version":env!("CARGO_PKG_VERSION")}}),
    )?;
    let mut workspace = Workspace {
        entry: entry_option(&params["initializationOptions"]),
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
                        } else {
                            workspace.published = published;
                            workspace.hints.clear();
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
fn notification(method: &str, params: Value) -> Notification {
    Notification::new(method.into(), params)
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
            }
            "workspace/didChangeWatchedFiles" => {}
            _ => return false,
        }
        self.hints.clear();
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
        self.hints.clear();
        let mut sources: BTreeMap<String, String> = self
            .documents
            .iter()
            .map(|(uri, d)| (uri.clone(), d.text.clone()))
            .collect();
        let mut diagnostics: BTreeMap<String, Vec<Value>> =
            self.documents.keys().map(|u| (u.clone(), vec![])).collect();
        let mut roots = BTreeSet::new();
        if let Some(entry) = &self.entry {
            roots.insert(entry.clone());
        }
        for (uri, doc) in &self.documents {
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
            match SemanticEngine::default().compile(&entry_id, &graph) {
                Ok(language) => {
                    let errors = language
                        .diagnostics
                        .iter()
                        .map(|d| remap(d, &paths))
                        .collect::<Vec<_>>();
                    self.add_diagnostics(&errors, &sources, &mut diagnostics);
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
                    }
                }
                Err(errors) => {
                    let errors = errors.iter().map(|d| remap(d, &paths)).collect::<Vec<_>>();
                    self.add_diagnostics(&errors, &sources, &mut diagnostics);
                }
            }
        }
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
    fn request(&self, method: &str, params: &Value) -> Result<Value, (i32, String)> {
        let uri = params["textDocument"]["uri"].as_str().unwrap_or("");
        match method {
            "textDocument/formatting" => {
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
            _ => Err((-32601, format!("Unsupported method {method}"))),
        }
    }
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
    d
}
#[cfg(test)]
mod tests {
    use super::*;
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
}
