//! Finding trait implementations in a source tree.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::Value;
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::{Attribute, Expr, ExprArray, ExprLit, Lit, Meta, MetaNameValue, Token, UseTree};
use walkdir::WalkDir;

use crate::case;
use crate::cfg::Cfg;
use crate::error::Error;

/// One registered implementation, as the templates see it.
///
/// Serialized field names are the template names: `{{type}}`, `{{path}}`, `{{name}}`,
/// `{{aliases}}`, `{{doc}}`, `{{generics}}`, `{{meta}}`, `{{trait}}`, `{{file}}`, `{{line}}`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Item {
    /// The implementing type's name, without path or generic arguments: `MaxFreeSlot`.
    #[serde(rename = "type")]
    pub type_name: String,
    /// Best-effort full path of the type: `crate::strategies::MaxFreeSlot`.
    pub path: String,
    /// Public name: `#[register(name = "...")]`, else the type name in SCREAMING_SNAKE.
    pub name: String,
    /// Extra public names from `#[register(aliases = [...])]`.
    pub aliases: Vec<String>,
    /// Doc comment of the `impl` block, else of the type definition. Empty if none.
    pub doc: String,
    /// Generic parameters of the `impl` (`T`, `'a`, `N`). Empty for concrete types.
    pub generics: Vec<String>,
    /// Every other `#[register(key = value)]` pair.
    pub meta: BTreeMap<String, Value>,
    /// Best-effort full path of the implemented trait.
    #[serde(rename = "trait")]
    pub trait_path: String,
    /// Source file, as walked from the scan root, with `/` separators.
    pub file: String,
    /// 1-based line of the `impl` keyword (0 for [`Scanner::extra`] items).
    pub line: usize,
}

/// The result of a scan: items by group, each group sorted by [`Item::name`].
///
/// Serializes as `{ "<group>": [<item>, ...], ... }`, which is the template context.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(transparent)]
pub struct Registry {
    groups: BTreeMap<String, Vec<Item>>,
}

impl Registry {
    /// The items of a group; empty if the group does not exist.
    pub fn group(&self, name: &str) -> &[Item] {
        self.groups.get(name).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Group names, sorted.
    pub fn group_names(&self) -> impl Iterator<Item = &str> {
        self.groups.keys().map(String::as_str)
    }

    /// The template context as JSON.
    pub fn to_json(&self) -> Value {
        serde_json::to_value(self).expect("registry serializes")
    }
}

type Check = Box<dyn Fn(&Registry) -> Result<(), Error>>;

struct Query {
    /// Trait path segments, without a leading `crate`. A single segment matches by name.
    segments: Vec<String>,
    group: String,
}

/// Scans a source tree for implementations of one or more traits.
///
/// ```no_run
/// let registry = impl_scan::Scanner::new("src")
///     .trait_impls("Strategy").group("strategies")
///     .trait_impls("crate::tables::ReachTable").group("tables")
///     .scan()
///     .unwrap_or_else(|e| panic!("{e}"));
/// ```
pub struct Scanner {
    root: PathBuf,
    queries: Vec<Query>,
    extras: Vec<(String, String)>,
    checks: Vec<Check>,
    cfg: Option<Cfg>,
    emit_cargo: Option<bool>,
}

impl Scanner {
    /// A scanner over every `.rs` file below `root` (usually `"src"`).
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            queries: Vec::new(),
            extras: Vec::new(),
            checks: Vec::new(),
            cfg: None,
            emit_cargo: None,
        }
    }

    /// Finds `impl <trait_path> for T`. A bare name (`"Strategy"`) matches any trait with
    /// that name; a path (`"crate::a::Strategy"`) must match the end of the trait path as
    /// resolved through the file's `use` items. The group defaults to the trait name.
    pub fn trait_impls(mut self, trait_path: &str) -> Self {
        let mut segments: Vec<String> = trait_path
            .split("::")
            .map(|s| s.trim().to_owned())
            .collect();
        if segments.first().is_some_and(|s| s == "crate") {
            segments.remove(0);
        }
        let group = segments.last().cloned().unwrap_or_default();
        self.queries.push(Query { segments, group });
        self
    }

    /// Names the group of the preceding [`trait_impls`](Self::trait_impls).
    ///
    /// # Panics
    /// If no `trait_impls` precedes it.
    pub fn group(mut self, group: &str) -> Self {
        let query = self
            .queries
            .last_mut()
            .expect("`group` must follow `trait_impls`");
        query.group = group.to_owned();
        self
    }

    /// Registers a type the scan cannot see, for example one whose `impl` is generated
    /// by a macro. `path` is used as written.
    pub fn extra(mut self, group: &str, path: &str) -> Self {
        self.extras.push((group.to_owned(), path.to_owned()));
        self
    }

    /// Adds a check run on the final registry; see [`crate::checks`].
    pub fn check(mut self, check: impl Fn(&Registry) -> Result<(), Error> + 'static) -> Self {
        self.checks.push(Box::new(check));
        self
    }

    /// Evaluates `#[cfg]` against `cfg` instead of the build script's environment.
    pub fn cfg(mut self, cfg: Cfg) -> Self {
        self.cfg = Some(cfg);
        self
    }

    /// Whether to print `cargo:rerun-if-changed` and honour `IMPL_SCAN_DUMP`. Defaults to
    /// true inside a build script (when `OUT_DIR` is set) and false elsewhere.
    pub fn emit_cargo(mut self, emit: bool) -> Self {
        self.emit_cargo = Some(emit);
        self
    }

    /// Runs the scan, the duplicate-name checks and every [`check`](Self::check).
    pub fn scan(self) -> Result<Registry, Error> {
        let in_build_script = std::env::var_os("OUT_DIR").is_some();
        let emit = self.emit_cargo.unwrap_or(in_build_script);
        if emit {
            println!("cargo:rerun-if-changed={}", self.root.display());
            println!("cargo:rerun-if-env-changed=IMPL_SCAN_DUMP");
        }
        let cfg = self.cfg.clone().unwrap_or_else(Cfg::from_env);

        let mut files: Vec<PathBuf> = Vec::new();
        for entry in WalkDir::new(&self.root) {
            let entry = entry
                .map_err(|e| Error::new(format!("cannot walk {}: {e}", self.root.display())))?;
            if entry.file_type().is_file() && entry.path().extension().is_some_and(|x| x == "rs") {
                files.push(entry.into_path());
            }
        }
        files.sort();

        let mut sources = Vec::with_capacity(files.len());
        for file in files {
            let source = std::fs::read_to_string(&file)
                .map_err(|e| Error::new(format!("cannot read file: {e}")).at(&file, None))?;
            sources.push((file, source));
        }

        // Parsing is the expensive part, so it is limited to files that can matter. An
        // impl of `Strategy` names `Strategy` in its file (directly, by path, or in the
        // `use ... as Alias` that renames it), so pass 1 parses only files containing a
        // trait name. A type's doc or #[register] is on its definition, which names the
        // type, so pass 2 parses the remaining files containing a found type's name.
        let mut collector = Collector {
            cfg: &cfg,
            queries: &self.queries,
            candidates: Vec::new(),
            types: HashMap::new(),
        };
        let trait_names: Vec<&str> = self
            .queries
            .iter()
            .filter_map(|q| q.segments.last().map(String::as_str))
            .collect();
        let mut parsed_files = vec![false; sources.len()];
        for (i, (file, source)) in sources.iter().enumerate() {
            if trait_names.iter().any(|t| source.contains(t)) {
                parsed_files[i] = true;
                self.collect(&mut collector, file, source)?;
            }
        }
        let type_names: HashSet<String> = collector
            .candidates
            .iter()
            .map(|c| c.type_name.clone())
            .collect();
        for (i, (file, source)) in sources.iter().enumerate() {
            if !parsed_files[i] && type_names.iter().any(|t| source.contains(t.as_str())) {
                self.collect(&mut collector, file, source)?;
            }
        }

        let mut registry = Registry::default();
        for query in &self.queries {
            registry.groups.entry(query.group.clone()).or_default();
        }
        for candidate in collector.candidates {
            let info = collector.types.get(&candidate.path);
            let register = match (candidate.register, info.and_then(|i| i.register.clone())) {
                (Some(r), _) | (None, Some(r)) => r,
                (None, None) => RegisterArgs::default(),
            };
            let doc = if candidate.doc.is_empty() {
                info.map(|i| i.doc.clone()).unwrap_or_default()
            } else {
                candidate.doc
            };
            let item = Item {
                name: register
                    .name
                    .unwrap_or_else(|| case::screaming(&candidate.type_name)),
                aliases: register.aliases,
                meta: register.meta,
                type_name: candidate.type_name,
                path: candidate.path,
                doc,
                generics: candidate.generics,
                trait_path: candidate.trait_path,
                file: candidate.file,
                line: candidate.line,
            };
            registry
                .groups
                .entry(candidate.group)
                .or_default()
                .push(item);
        }
        for (group, path) in self.extras {
            let type_name = path.rsplit("::").next().unwrap_or(&path).to_owned();
            registry.groups.entry(group).or_default().push(Item {
                name: case::screaming(&type_name),
                type_name,
                path,
                aliases: Vec::new(),
                doc: String::new(),
                generics: Vec::new(),
                meta: BTreeMap::new(),
                trait_path: String::new(),
                file: String::new(),
                line: 0,
            });
        }

        for (group, items) in &mut registry.groups {
            items.sort_by(|a, b| a.name.cmp(&b.name).then(a.path.cmp(&b.path)));
            check_unique(group, items)?;
        }
        for check in &self.checks {
            check(&registry)?;
        }

        if emit && std::env::var_os("IMPL_SCAN_DUMP").is_some() {
            if let Some(out) = std::env::var_os("OUT_DIR") {
                let path = Path::new(&out).join("impl_scan.json");
                let json = serde_json::to_string_pretty(&registry).expect("registry serializes");
                std::fs::write(&path, json)
                    .map_err(|e| Error::new(format!("cannot write dump: {e}")).at(&path, None))?;
                println!(
                    "cargo:warning=impl-scan registry dumped to {}",
                    path.display()
                );
            }
        }
        Ok(registry)
    }

    fn collect(&self, collector: &mut Collector, file: &Path, source: &str) -> Result<(), Error> {
        let parsed = syn::parse_file(source).map_err(|e| {
            Error::new(format!("cannot parse: {e}")).at(file, Some(e.span().start().line))
        })?;
        if !collector.cfg.enabled(&parsed.attrs) {
            return Ok(());
        }
        let display = file.to_string_lossy().replace('\\', "/");
        let module = module_of(&self.root, file);
        collector.items(&parsed.items, &module, &display)
    }
}

fn check_unique(group: &str, items: &[Item]) -> Result<(), Error> {
    let mut paths: HashSet<&str> = HashSet::new();
    let mut names: HashMap<&str, &Item> = HashMap::new();
    for item in items {
        if !paths.insert(&item.path) {
            return Err(Error::new(format!(
                "`{}` is registered twice in group `{group}`",
                item.path
            ))
            .at(&item.file, Some(item.line))
            .help("two impls are active for the same type; check their #[cfg] attributes"));
        }
        for name in std::iter::once(&item.name).chain(&item.aliases) {
            if let Some(other) = names.insert(name, item) {
                return Err(
                    Error::new(format!("duplicate name `{name}` in group `{group}`"))
                        .at(&item.file, Some(item.line))
                        .help(format!(
                            "also used by `{}` ({}:{})",
                            other.path, other.file, other.line
                        )),
                );
            }
        }
    }
    Ok(())
}

/// The module path of a file: `src/lib.rs` -> `crate`, `src/a/mod.rs` and `src/a.rs`
/// -> `crate::a`, `src/a/b.rs` -> `crate::a::b`.
fn module_of(root: &Path, file: &Path) -> Vec<String> {
    let mut module = vec!["crate".to_owned()];
    let relative = file.strip_prefix(root).unwrap_or(file);
    let parts: Vec<String> = relative
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    for (i, part) in parts.iter().enumerate() {
        let last = i + 1 == parts.len();
        if !last {
            module.push(part.clone());
            continue;
        }
        let stem = part.trim_end_matches(".rs");
        let is_root = i == 0 && (stem == "lib" || stem == "main");
        if stem != "mod" && !is_root {
            module.push(stem.to_owned());
        }
    }
    module
}

#[derive(Debug, Clone, Default)]
struct RegisterArgs {
    name: Option<String>,
    aliases: Vec<String>,
    meta: BTreeMap<String, Value>,
}

struct TypeInfo {
    doc: String,
    register: Option<RegisterArgs>,
}

struct Candidate {
    group: String,
    type_name: String,
    path: String,
    trait_path: String,
    generics: Vec<String>,
    doc: String,
    register: Option<RegisterArgs>,
    file: String,
    line: usize,
}

struct Collector<'a> {
    cfg: &'a Cfg,
    queries: &'a [Query],
    candidates: Vec<Candidate>,
    types: HashMap<String, TypeInfo>,
}

/// Names visible in one module: `use` imports and child modules.
struct Scope<'a> {
    module: &'a [String],
    imports: HashMap<String, Vec<String>>,
    child_modules: HashSet<String>,
}

impl Scope<'_> {
    /// Resolves the leading segments of a path written in this module.
    fn resolve(&self, segments: &[String], leading_colon: bool) -> Vec<String> {
        let Some(first) = segments.first() else {
            return Vec::new();
        };
        if leading_colon {
            return segments.to_vec();
        }
        match first.as_str() {
            "crate" => segments.to_vec(),
            "self" => [self.module, &segments[1..]].concat(),
            "super" => {
                let mut base = self.module.to_vec();
                let mut rest = segments;
                while rest.first().is_some_and(|s| s == "super") {
                    if base.len() > 1 {
                        base.pop();
                    }
                    rest = &rest[1..];
                }
                [base.as_slice(), rest].concat()
            }
            _ => {
                if let Some(target) = self.imports.get(first) {
                    [target.as_slice(), &segments[1..]].concat()
                } else if segments.len() == 1 || self.child_modules.contains(first) {
                    // A bare name not imported is taken to be defined in this module.
                    [self.module, segments].concat()
                } else {
                    // An extern crate path such as `std::fmt::Display`.
                    segments.to_vec()
                }
            }
        }
    }
}

impl Collector<'_> {
    fn items(&mut self, items: &[syn::Item], module: &[String], file: &str) -> Result<(), Error> {
        let mut scope = Scope {
            module,
            imports: HashMap::new(),
            child_modules: HashSet::new(),
        };
        for item in items {
            if let syn::Item::Mod(m) = item {
                scope.child_modules.insert(m.ident.to_string());
            }
        }
        for item in items {
            if let syn::Item::Use(u) = item {
                if self.cfg.enabled(&u.attrs) {
                    let mut imports = Vec::new();
                    flatten_use(&u.tree, &mut Vec::new(), &mut imports);
                    for (alias, path) in imports {
                        let resolved = scope.resolve(&path, u.leading_colon.is_some());
                        scope.imports.insert(alias, resolved);
                    }
                }
            }
        }

        for item in items {
            match item {
                syn::Item::Impl(imp) if self.cfg.enabled(&imp.attrs) => {
                    self.impl_block(imp, &scope, file)?
                }
                syn::Item::Mod(m) if self.cfg.enabled(&m.attrs) => {
                    if let Some((_, inner)) = &m.content {
                        let child: Vec<String> = [module, &[m.ident.to_string()]].concat();
                        self.items(inner, &child, file)?;
                    }
                }
                syn::Item::Struct(s) if self.cfg.enabled(&s.attrs) => {
                    self.type_def(&s.ident, &s.attrs, &scope, file)?
                }
                syn::Item::Enum(e) if self.cfg.enabled(&e.attrs) => {
                    self.type_def(&e.ident, &e.attrs, &scope, file)?
                }
                syn::Item::Union(u) if self.cfg.enabled(&u.attrs) => {
                    self.type_def(&u.ident, &u.attrs, &scope, file)?
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn type_def(
        &mut self,
        ident: &syn::Ident,
        attrs: &[Attribute],
        scope: &Scope,
        file: &str,
    ) -> Result<(), Error> {
        let path = [scope.module, &[ident.to_string()]].concat().join("::");
        let register = register_args(attrs, file)?;
        let doc = doc_of(attrs);
        if register.is_some() || !doc.is_empty() {
            self.types.insert(path, TypeInfo { doc, register });
        }
        Ok(())
    }

    fn impl_block(&mut self, imp: &syn::ItemImpl, scope: &Scope, file: &str) -> Result<(), Error> {
        let Some((bang, trait_path, _)) = &imp.trait_ else {
            return Ok(());
        };
        if bang.is_some() {
            return Ok(());
        }
        let trait_segments =
            scope.resolve(&segments_of(trait_path), trait_path.leading_colon.is_some());
        let groups: Vec<&str> = self
            .queries
            .iter()
            .filter(|q| trait_segments.ends_with(&q.segments))
            .map(|q| q.group.as_str())
            .collect();
        if groups.is_empty() {
            return Ok(());
        }
        let syn::Type::Path(self_ty) = &*imp.self_ty else {
            // `impl Trait for &T`, tuples, trait objects...: not a nameable registry entry.
            return Ok(());
        };
        if self_ty.qself.is_some() {
            return Ok(());
        }
        let type_segments = scope.resolve(
            &segments_of(&self_ty.path),
            self_ty.path.leading_colon.is_some(),
        );
        let line = imp.impl_token.span().start().line;
        let register = register_args(&imp.attrs, file)?;
        let generics = imp
            .generics
            .params
            .iter()
            .map(|p| match p {
                syn::GenericParam::Type(t) => t.ident.to_string(),
                syn::GenericParam::Lifetime(l) => l.lifetime.to_string(),
                syn::GenericParam::Const(c) => c.ident.to_string(),
            })
            .collect::<Vec<_>>();
        for group in groups {
            self.candidates.push(Candidate {
                group: group.to_owned(),
                type_name: type_segments.last().cloned().unwrap_or_default(),
                path: type_segments.join("::"),
                trait_path: trait_segments.join("::"),
                generics: generics.clone(),
                doc: doc_of(&imp.attrs),
                register: register.clone(),
                file: file.to_owned(),
                line,
            });
        }
        Ok(())
    }
}

fn segments_of(path: &syn::Path) -> Vec<String> {
    path.segments.iter().map(|s| s.ident.to_string()).collect()
}

/// Flattens a `use` tree into `(local name, path)` pairs. Globs are skipped.
fn flatten_use(tree: &UseTree, prefix: &mut Vec<String>, out: &mut Vec<(String, Vec<String>)>) {
    match tree {
        UseTree::Path(p) => {
            prefix.push(p.ident.to_string());
            flatten_use(&p.tree, prefix, out);
            prefix.pop();
        }
        UseTree::Name(n) => {
            let name = n.ident.to_string();
            if name == "self" {
                if let Some(last) = prefix.last() {
                    out.push((last.clone(), prefix.clone()));
                }
            } else {
                out.push((name.clone(), [prefix.as_slice(), &[name]].concat()));
            }
        }
        UseTree::Rename(r) => {
            let target = r.ident.to_string();
            let path = if target == "self" {
                prefix.clone()
            } else {
                [prefix.as_slice(), &[target]].concat()
            };
            if r.rename != "_" {
                out.push((r.rename.to_string(), path));
            }
        }
        UseTree::Glob(_) => {}
        UseTree::Group(g) => {
            for tree in &g.items {
                flatten_use(tree, prefix, out);
            }
        }
    }
}

fn doc_of(attrs: &[Attribute]) -> String {
    let lines: Vec<String> = attrs
        .iter()
        .filter(|a| a.path().is_ident("doc"))
        .filter_map(|a| match &a.meta {
            Meta::NameValue(MetaNameValue {
                value:
                    Expr::Lit(ExprLit {
                        lit: Lit::Str(s), ..
                    }),
                ..
            }) => Some(s.value()),
            _ => None,
        })
        .flat_map(|s| {
            // An empty `///` line is an empty string, which `lines()` would drop.
            let lines: Vec<String> = s
                .lines()
                .map(|l| l.strip_prefix(' ').unwrap_or(l).trim_end().to_owned())
                .collect();
            if lines.is_empty() {
                vec![String::new()]
            } else {
                lines
            }
        })
        .collect();
    lines.join("\n").trim().to_owned()
}

fn is_register(attr: &Attribute) -> bool {
    attr.path()
        .segments
        .last()
        .is_some_and(|s| s.ident == "register")
}

fn register_args(attrs: &[Attribute], file: &str) -> Result<Option<RegisterArgs>, Error> {
    let Some(attr) = attrs.iter().find(|a| is_register(a)) else {
        return Ok(None);
    };
    let line = attr.span().start().line;
    let fail = |msg: String| Error::new(msg).at(file, Some(line));
    let mut args = RegisterArgs::default();
    let pairs = match &attr.meta {
        Meta::Path(_) => return Ok(Some(args)),
        Meta::List(_) => attr
            .parse_args_with(Punctuated::<MetaNameValue, Token![,]>::parse_terminated)
            .map_err(|e| fail(format!("malformed #[register]: {e}")))?,
        Meta::NameValue(_) => return Err(fail("expected #[register(key = value, ...)]".into())),
    };
    for pair in pairs {
        let key = pair
            .path
            .get_ident()
            .map(|i| i.to_string())
            .ok_or_else(|| fail("expected a plain key".into()))?;
        let value = literal(&pair.value)
            .ok_or_else(|| fail(format!("`{key}` must be a literal or an array of literals")))?;
        match key.as_str() {
            "name" => {
                args.name = Some(
                    value
                        .as_str()
                        .ok_or_else(|| fail("`name` must be a string".into()))?
                        .to_owned(),
                )
            }
            "aliases" => {
                let list = value
                    .as_array()
                    .ok_or_else(|| fail("`aliases` must be an array of strings".into()))?;
                for alias in list {
                    let alias = alias
                        .as_str()
                        .ok_or_else(|| fail("`aliases` must be an array of strings".into()))?;
                    args.aliases.push(alias.to_owned());
                }
            }
            _ => {
                args.meta.insert(key, value);
            }
        }
    }
    Ok(Some(args))
}

fn literal(expr: &Expr) -> Option<Value> {
    match expr {
        Expr::Lit(ExprLit { lit, .. }) => match lit {
            Lit::Str(s) => Some(Value::String(s.value())),
            Lit::Bool(b) => Some(Value::Bool(b.value)),
            Lit::Int(i) => i
                .base10_parse::<i64>()
                .map(Value::from)
                .or_else(|_| i.base10_parse::<u64>().map(Value::from))
                .ok(),
            Lit::Float(f) => f.base10_parse::<f64>().ok().map(Value::from),
            _ => None,
        },
        Expr::Array(ExprArray { elems, .. }) => elems
            .iter()
            .map(literal)
            .collect::<Option<Vec<_>>>()
            .map(Value::Array),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_paths() {
        let root = Path::new("src");
        assert_eq!(module_of(root, Path::new("src/lib.rs")), ["crate"]);
        assert_eq!(module_of(root, Path::new("src/main.rs")), ["crate"]);
        assert_eq!(module_of(root, Path::new("src/a.rs")), ["crate", "a"]);
        assert_eq!(module_of(root, Path::new("src/a/mod.rs")), ["crate", "a"]);
        assert_eq!(
            module_of(root, Path::new("src/a/b.rs")),
            ["crate", "a", "b"]
        );
    }

    #[test]
    fn use_trees() {
        let item: syn::ItemUse =
            syn::parse_str("use a::{b::{C, D as E}, f, g::self, _h as _};").unwrap();
        let mut out = Vec::new();
        flatten_use(&item.tree, &mut Vec::new(), &mut out);
        let out: Vec<(String, String)> = out.into_iter().map(|(k, v)| (k, v.join("::"))).collect();
        assert_eq!(
            out,
            [
                ("C", "a::b::C"),
                ("E", "a::b::D"),
                ("f", "a::f"),
                ("g", "a::g")
            ]
            .map(|(a, b)| (a.to_owned(), b.to_owned()))
        );
    }
}
