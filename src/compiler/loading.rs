//! Module graph collection and canonical import identities.

use super::*;

impl Compiler {
    /// Load only modules reachable from `id`, resolving relative edges against source-map IDs.
    /// Bundled standard IDs are canonicalized before insertion, so aliases cannot duplicate a module.
    pub(super) fn read_sources(
        &mut self,
        id: &str,
        sources: &BTreeMap<String, String>,
    ) -> Result<()> {
        if self.modules.contains_key(id) || id == "core/parser" {
            return Ok(());
        }
        if self.modules.len() >= 256 {
            return Err(Diagnostic::error(
                "yl.module_limit",
                "Module graph exceeds resource limit",
                Span::new(id, 0, 0),
            ));
        }
        let source = if let Some(source) = standard_source(id) {
            source
        } else {
            sources.get(id).ok_or_else(|| {
                Diagnostic::error(
                    "yl.unresolved_import",
                    format!("Missing module {id}"),
                    Span::new(id, 0, 0),
                )
            })?
        };
        let module = parse_yl(id, source).map_err(|e| {
            e.into_iter().next().unwrap_or_else(|| {
                Diagnostic::error("yl.parse", "YL parser failed", Span::new(id, 0, 0))
            })
        })?;
        let dependencies: Vec<_> = module
            .declarations
            .iter()
            .filter_map(|d| {
                if let DeclKind::Import { path, .. } | DeclKind::NamespaceImport { path, .. } =
                    &d.kind
                {
                    Some((path.clone(), import_path(id, path), d.span.clone()))
                } else {
                    None
                }
            })
            .collect();
        self.modules.insert(id.into(), module);
        for (specifier, dependency, span) in dependencies {
            let dependency = if self.registry.contains_module(&specifier) {
                specifier.clone()
            } else {
                dependency
            };
            self.dependencies
                .insert((id.into(), specifier), dependency.clone());
            if self.registry.contains_module(&dependency) {
                continue;
            }
            self.read_sources(&dependency, sources).map_err(|mut e| {
                e.secondary.push(*e.primary.clone());
                e.primary = Box::new(span);
                e
            })?;
        }
        Ok(())
    }
    /// Collect the filesystem graph before symbol resolution. `root` anchors diagnostic module IDs;
    /// `path` is canonicalized for deduplication before following its import edges.
    pub(super) fn read_module(&mut self, path: &Path, root: &Path) -> Result<()> {
        let id = module_id(path, root);
        if self.modules.contains_key(&id) {
            return Ok(());
        }
        if self.modules.len() >= 256 {
            return Err(Diagnostic::error(
                "yl.module_limit",
                "Module graph exceeds resource limit",
                Span::new(&id, 0, 0),
            ));
        }
        let source = std::fs::read_to_string(path).map_err(|e| {
            Diagnostic::error("yl.unresolved_import", e.to_string(), Span::new(&id, 0, 0))
        })?;
        let module = parse_yl(&id, &source).map_err(|e| {
            e.into_iter().next().unwrap_or_else(|| {
                Diagnostic::error("yl.parse", "YL parser failed", Span::new(&id, 0, 0))
            })
        })?;
        let dependencies: Vec<_> = module
            .declarations
            .iter()
            .filter_map(|d| {
                if let DeclKind::Import { path, .. } | DeclKind::NamespaceImport { path, .. } =
                    &d.kind
                {
                    Some((path.clone(), d.span.clone()))
                } else {
                    None
                }
            })
            .collect();
        self.modules.insert(id.clone(), module);
        for (dependency, span) in dependencies {
            if let Some(standard) = standard_module(&dependency) {
                self.dependencies
                    .insert((id.clone(), dependency), standard.into());
                self.read_sources(standard, &BTreeMap::new())
                    .map_err(|mut error| {
                        error.secondary.push(*error.primary.clone());
                        error.primary = Box::new(span);
                        error
                    })?;
                continue;
            }
            if self.registry.contains_module(&dependency) {
                self.dependencies
                    .insert((id.clone(), dependency.clone()), dependency);
                continue;
            }
            if dependency == "core/parser" {
                self.dependencies
                    .insert((id.clone(), dependency.clone()), dependency);
                continue;
            }
            let mut dep = path.parent().unwrap_or(root).join(&dependency);
            if dep.extension().is_none() {
                dep.set_extension("yl");
            }
            let dep = std::fs::canonicalize(dep).map_err(|e| {
                Diagnostic::error("yl.unresolved_import", format!("{dependency}: {e}"), span)
            })?;
            self.dependencies
                .insert((id.clone(), dependency), module_id(&dep, root));
            self.read_module(&dep, root)?;
        }
        Ok(())
    }
}

fn module_id(path: &Path, root: &Path) -> String {
    let target: Vec<_> = path.components().collect();
    let base: Vec<_> = root.components().collect();
    let common = target.iter().zip(&base).take_while(|(a, b)| a == b).count();
    if common == 0 {
        return path.to_string_lossy().replace('\\', "/");
    }
    let mut relative = PathBuf::new();
    for _ in common..base.len() {
        relative.push("..");
    }
    for component in target.iter().skip(common) {
        relative.push(component.as_os_str());
    }
    relative.to_string_lossy().replace('\\', "/")
}
/// Reserved standard modules have stable IDs in filesystem and in-memory graphs.
/// The historical variables spelling denotes the same module, not a second copy.
fn standard_module(path: &str) -> Option<&'static str> {
    match path {
        "std/parser" => Some("std/parser"),
        "std/references" | "std/variables" => Some("std/references"),
        "std/types" => Some("std/types"),
        "std/types/structural" => Some("std/types/structural"),
        "std/types/arithmetic" => Some("std/types/arithmetic"),
        "std/backend/integer" => Some("std/backend/integer"),
        _ => None,
    }
}
fn standard_source(id: &str) -> Option<&'static str> {
    match id {
        "std/parser" => Some(include_str!("../../stdlib/parser.yl")),
        "std/references" => Some(include_str!("../../stdlib/references.yl")),
        "std/types" => Some(include_str!("../../stdlib/types.yl")),
        "std/types/structural" => Some(include_str!("../../stdlib/types/structural.yl")),
        "std/types/arithmetic" => Some(include_str!("../../stdlib/types/arithmetic.yl")),
        "std/backend/integer" => Some(include_str!("../../stdlib/backend/integer.yl")),
        _ => None,
    }
}
fn import_path(module: &str, path: &str) -> String {
    if let Some(id) = standard_module(path) {
        return id.into();
    }
    if path == "core/parser" {
        return path.into();
    }
    let mut parts = vec![];
    let parent = module.rsplit_once('/').map(|p| p.0).unwrap_or("");
    for part in format!("{parent}/{path}").split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if parts.last().is_some_and(|p| *p != "..") {
                    parts.pop();
                } else {
                    parts.push(part.to_owned());
                }
            }
            _ => parts.push(part.to_owned()),
        }
    }
    let mut path = parts.join("/");
    if !path.ends_with(".yl") {
        path.push_str(".yl");
    }
    path
}
