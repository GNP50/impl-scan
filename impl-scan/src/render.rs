//! Rendering Handlebars templates against a [`Registry`].

use std::path::{Path, PathBuf};

use handlebars::{handlebars_helper, Handlebars};
use serde_json::{Map, Value};

use crate::case;
use crate::error::Error;
use crate::scan::Registry;

/// The built-in partials, by name. See the README for their parameters.
pub const BUILTIN_PARTIALS: &[(&str, &str)] = &[
    ("registry_enum", include_str!("partials/registry_enum.hbs")),
    ("store", include_str!("partials/store.hbs")),
    ("dispatch", include_str!("partials/dispatch.hbs")),
];

handlebars_helper!(snake: |s: str| case::snake(s));
handlebars_helper!(pascal: |s: str| case::pascal(s));
handlebars_helper!(camel: |s: str| case::camel(s));
handlebars_helper!(screaming: |s: str| case::screaming(s));
handlebars_helper!(lower: |s: str| s.to_lowercase());
handlebars_helper!(upper: |s: str| s.to_uppercase());
// `{:?}` of a `str` is a valid Rust string literal, escapes included.
handlebars_helper!(quote: |s: str| format!("{s:?}"));
handlebars_helper!(join: |items: array, sep: str| {
    items.iter().map(|v| match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }).collect::<Vec<_>>().join(sep)
});
// `{{doc text fallback="..." indent=4}}`: one `///` line per line of `text`, the first
// one unindented (the template places it), the following ones indented.
handlebars_helper!(doc: |text: str, {fallback: str = "", indent: u64 = 0}| {
    let text = if text.trim().is_empty() { fallback } else { text };
    let pad = " ".repeat(indent as usize);
    text.lines()
        .map(|l| if l.is_empty() { "///".to_owned() } else { format!("/// {l}") })
        .collect::<Vec<_>>()
        .join(&format!("\n{pad}"))
});

/// Renders templates against a registry, with case helpers and the built-in partials.
///
/// Helpers: `snake`, `pascal`, `camel`, `screaming`, `lower`, `upper`, `quote`
/// (a Rust string literal), `join array sep`, `doc text fallback=".." indent=N`.
/// HTML escaping is off and strict mode is on: a misspelled field is an error, not an
/// empty string.
pub struct Renderer {
    handlebars: Handlebars<'static>,
    data: Map<String, Value>,
    emit_cargo: bool,
}

impl Default for Renderer {
    fn default() -> Self {
        Self::new()
    }
}

impl Renderer {
    /// A renderer with the helpers and built-in partials registered.
    pub fn new() -> Self {
        let mut handlebars = Handlebars::new();
        handlebars.register_escape_fn(handlebars::no_escape);
        handlebars.set_strict_mode(true);
        handlebars.register_helper("snake", Box::new(snake));
        handlebars.register_helper("pascal", Box::new(pascal));
        handlebars.register_helper("camel", Box::new(camel));
        handlebars.register_helper("screaming", Box::new(screaming));
        handlebars.register_helper("lower", Box::new(lower));
        handlebars.register_helper("upper", Box::new(upper));
        handlebars.register_helper("quote", Box::new(quote));
        handlebars.register_helper("join", Box::new(join));
        handlebars.register_helper("doc", Box::new(doc));
        for (name, source) in BUILTIN_PARTIALS {
            handlebars
                .register_partial(name, *source)
                .expect("built-in partial parses");
        }
        Self {
            handlebars,
            data: Map::new(),
            emit_cargo: std::env::var_os("OUT_DIR").is_some(),
        }
    }

    /// Whether to print `cargo:rerun-if-changed` for templates and partials. Defaults to
    /// true inside a build script.
    pub fn emit_cargo(mut self, emit: bool) -> Self {
        self.emit_cargo = emit;
        self
    }

    /// Turns strict mode off (it is on by default).
    pub fn strict(mut self, strict: bool) -> Self {
        self.handlebars.set_strict_mode(strict);
        self
    }

    /// Registers a partial from a string. Overrides a built-in partial of the same name.
    pub fn partial(mut self, name: &str, source: &str) -> Result<Self, Error> {
        self.handlebars.register_partial(name, source)?;
        Ok(self)
    }

    /// Registers every `*.hbs` file of `dir` as a partial named after the file stem.
    pub fn partials_dir(mut self, dir: impl AsRef<Path>) -> Result<Self, Error> {
        let dir = dir.as_ref();
        if self.emit_cargo {
            println!("cargo:rerun-if-changed={}", dir.display());
        }
        let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
            .map_err(|e| Error::new(format!("cannot read partials: {e}")).at(dir, None))?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "hbs"))
            .collect();
        entries.sort();
        for path in entries {
            let name = path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            let source = std::fs::read_to_string(&path)
                .map_err(|e| Error::new(format!("cannot read partial: {e}")).at(&path, None))?;
            self.handlebars
                .register_partial(&name, source)
                .map_err(|e| Error::from(e).at(&path, None))?;
        }
        Ok(self)
    }

    /// Adds a top-level value to the template context, next to the groups.
    pub fn data(mut self, key: &str, value: impl Into<Value>) -> Self {
        self.data.insert(key.to_owned(), value.into());
        self
    }

    fn context(&self, registry: &Registry) -> Value {
        let mut context = match registry.to_json() {
            Value::Object(map) => map,
            _ => Map::new(),
        };
        for (key, value) in &self.data {
            context.insert(key.clone(), value.clone());
        }
        Value::Object(context)
    }

    /// Renders a template given as a string.
    pub fn render_str(&self, template: &str, registry: &Registry) -> Result<String, Error> {
        Ok(self
            .handlebars
            .render_template(template, &self.context(registry))?)
    }

    /// Renders the template file `template` to `output`, behind a `@generated` header.
    /// The file is written only when its content changes, so an unchanged output keeps
    /// its mtime and does not trigger a rebuild. Returns whether it was written.
    pub fn render_to_file(
        &self,
        template: impl AsRef<Path>,
        registry: &Registry,
        output: impl AsRef<Path>,
    ) -> Result<bool, Error> {
        let (template, output) = (template.as_ref(), output.as_ref());
        if self.emit_cargo {
            println!("cargo:rerun-if-changed={}", template.display());
        }
        let source = std::fs::read_to_string(template)
            .map_err(|e| Error::new(format!("cannot read template: {e}")).at(template, None))?;
        let body = self
            .handlebars
            .render_template(&source, &self.context(registry))
            .map_err(|e| Error::from(e).at(template, None))?;
        let name = template.file_name().unwrap_or_default().to_string_lossy();
        let rendered =
            format!("// @generated by impl-scan from {name}. Do not edit by hand.\n\n{body}");
        if std::fs::read_to_string(output).ok().as_deref() == Some(rendered.as_str()) {
            return Ok(false);
        }
        if let Some(parent) = output.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).map_err(|e| {
                Error::new(format!("cannot create output directory: {e}")).at(parent, None)
            })?;
        }
        std::fs::write(output, rendered)
            .map_err(|e| Error::new(format!("cannot write output: {e}")).at(output, None))?;
        Ok(true)
    }
}
