//! Reads every `.rs` file under a directory with `syn` and records each function:
//! where it is, what kind it is, and what it calls. Nothing is compiled or run, so
//! any codebase that parses can be analyzed, even one that doesn't build.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::visit::{self, Visit};
use syn::{Attribute, Expr, Token, UseTree};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    Function,
    Method,
    Test,
    /// An HTTP handler (`#[get("/path")]` and friends): its callers are outside the codebase.
    Endpoint {
        method: String,
        route: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Call {
    /// `apply_discount(…)`, `pricing::apply_discount(…)`, `Cart::new(…)`
    Path(Vec<String>),
    /// `cart.total()`: the receiver's type is unknown without compiling.
    Method(String),
}

#[derive(Debug, Clone)]
pub struct FnDef {
    /// Qualified name: `pricing::apply_discount`, `cart::Cart::total`.
    pub name: String,
    pub module: Vec<String>,
    pub owner: Option<String>,
    pub ident: String,
    pub kind: Kind,
    pub public: bool,
    pub file: PathBuf,
    /// The line of `fn name`; `doc_line` includes the doc comment above it.
    pub line: usize,
    pub doc_line: usize,
    pub end_line: usize,
    pub calls: Vec<Call>,
}

/// What `use` items bring into one module: `apply_discount` → `crate::pricing::apply_discount`.
#[derive(Debug, Default, Clone)]
pub struct Scope {
    pub aliases: HashMap<String, Vec<String>>,
    pub globs: Vec<Vec<String>>,
}

pub struct Codebase {
    pub root: PathBuf,
    /// The crate's name: `shop::cart::Cart` in an integration test means `cart::Cart`.
    pub crate_name: String,
    pub functions: Vec<FnDef>,
    pub scopes: HashMap<Vec<String>, Scope>,
    pub sources: HashMap<PathBuf, String>,
}

impl Codebase {
    /// Lines `from..=to` (1-based) of a file, for showing code in reports.
    pub fn lines(&self, file: &Path, from: usize, to: usize) -> String {
        let source = self.sources.get(file).map(String::as_str).unwrap_or("");
        source
            .lines()
            .skip(from.saturating_sub(1))
            .take(to + 1 - from)
            .collect::<Vec<_>>()
            .join("\n")
    }
}

pub fn parse_dir(root: &Path) -> Result<Codebase> {
    let mut files = Vec::new();
    collect_rs_files(root, &mut files)?;
    files.sort();
    let crate_name = root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("crate")
        .replace('-', "_");
    let mut codebase = Codebase {
        root: root.to_path_buf(),
        crate_name,
        functions: Vec::new(),
        scopes: HashMap::new(),
        sources: HashMap::new(),
    };
    for path in files {
        let relative = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
        let source = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        let file =
            syn::parse_file(&source).with_context(|| format!("parsing {}", relative.display()))?;
        let mut collector = Collector {
            module: module_of(&relative),
            owner: None,
            file: &relative,
            codebase: &mut codebase,
        };
        collector.visit_file(&file);
        codebase.sources.insert(relative, source);
    }
    Ok(codebase)
}

fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let path = entry?.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if path.is_dir() {
            if name != "target" && !name.starts_with('.') {
                collect_rs_files(&path, out)?;
            }
        } else if name.ends_with(".rs") {
            out.push(path);
        }
    }
    Ok(())
}

/// `src/lib.rs` → crate root, `src/cart.rs` → `cart`, `src/a/mod.rs` → `a`,
/// `tests/checkout.rs` → `tests::checkout`.
fn module_of(relative: &Path) -> Vec<String> {
    let mut parts: Vec<String> = relative
        .with_extension("")
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    if parts.first().is_some_and(|p| p == "src") {
        parts.remove(0);
    }
    if parts
        .last()
        .is_some_and(|p| p == "mod" || p == "lib" || p == "main")
    {
        parts.pop();
    }
    parts
}

struct Collector<'a> {
    module: Vec<String>,
    owner: Option<String>,
    file: &'a Path,
    codebase: &'a mut Codebase,
}

impl Collector<'_> {
    fn record(
        &mut self,
        attrs: &[Attribute],
        public: bool,
        ident: &syn::Ident,
        item: &impl Spanned,
        body: &syn::Block,
    ) {
        let kind = kind_of(attrs).unwrap_or(if self.owner.is_some() {
            Kind::Method
        } else {
            Kind::Function
        });
        let mut name = self.module.clone();
        name.extend(self.owner.clone());
        name.push(ident.to_string());
        let mut calls = Calls(Vec::new());
        calls.visit_block(body);
        self.codebase.functions.push(FnDef {
            name: name.join("::"),
            module: self.module.clone(),
            owner: self.owner.clone(),
            ident: ident.to_string(),
            kind,
            public,
            file: self.file.to_path_buf(),
            line: ident.span().start().line,
            doc_line: item.span().start().line,
            end_line: item.span().end().line,
            calls: calls.0,
        });
    }
}

impl<'ast> Visit<'ast> for Collector<'_> {
    fn visit_item_mod(&mut self, m: &'ast syn::ItemMod) {
        if let Some((_, items)) = &m.content {
            self.module.push(m.ident.to_string());
            for item in items {
                self.visit_item(item);
            }
            self.module.pop();
        }
    }

    fn visit_item_impl(&mut self, i: &'ast syn::ItemImpl) {
        if let syn::Type::Path(tp) = &*i.self_ty {
            self.owner = tp.path.segments.last().map(|s| s.ident.to_string());
            visit::visit_item_impl(self, i);
            self.owner = None;
        }
    }

    fn visit_item_fn(&mut self, f: &'ast syn::ItemFn) {
        let public = matches!(f.vis, syn::Visibility::Public(_));
        self.record(&f.attrs, public, &f.sig.ident, f, &f.block);
    }

    fn visit_impl_item_fn(&mut self, f: &'ast syn::ImplItemFn) {
        let public = matches!(f.vis, syn::Visibility::Public(_));
        self.record(&f.attrs, public, &f.sig.ident, f, &f.block);
    }

    fn visit_item_use(&mut self, u: &'ast syn::ItemUse) {
        let scope = self.codebase.scopes.entry(self.module.clone()).or_default();
        add_use(&u.tree, Vec::new(), scope);
    }
}

/// `use crate::pricing::{self, apply_discount as discount};` → aliases `pricing`, `discount`.
fn add_use(tree: &UseTree, prefix: Vec<String>, scope: &mut Scope) {
    let with = |ident: &syn::Ident| {
        let mut path = prefix.clone();
        path.push(ident.to_string());
        path
    };
    match tree {
        UseTree::Path(p) => add_use(&p.tree, with(&p.ident), scope),
        UseTree::Name(n) if n.ident == "self" => {
            if let Some(last) = prefix.last() {
                scope.aliases.insert(last.clone(), prefix.clone());
            }
        }
        UseTree::Name(n) => {
            scope.aliases.insert(n.ident.to_string(), with(&n.ident));
        }
        UseTree::Rename(r) => {
            scope.aliases.insert(r.rename.to_string(), with(&r.ident));
        }
        UseTree::Glob(_) => scope.globs.push(prefix),
        UseTree::Group(g) => g
            .items
            .iter()
            .for_each(|t| add_use(t, prefix.clone(), scope)),
    }
}

fn kind_of(attrs: &[Attribute]) -> Option<Kind> {
    for attr in attrs {
        let Some(last) = attr.path().segments.last() else {
            continue;
        };
        let name = last.ident.to_string();
        if name == "test" {
            return Some(Kind::Test);
        }
        if ["get", "post", "put", "patch", "delete"].contains(&name.as_str()) {
            if let Ok(route) = attr.parse_args::<syn::LitStr>() {
                return Some(Kind::Endpoint {
                    method: name.to_uppercase(),
                    route: route.value(),
                });
            }
        }
    }
    None
}

/// Every call in a function body, including the ones inside macros like `assert_eq!`.
struct Calls(Vec<Call>);

impl<'ast> Visit<'ast> for Calls {
    fn visit_expr_call(&mut self, c: &'ast syn::ExprCall) {
        if let Expr::Path(p) = &*c.func {
            self.0.push(Call::Path(
                p.path
                    .segments
                    .iter()
                    .map(|s| s.ident.to_string())
                    .collect(),
            ));
        }
        visit::visit_expr_call(self, c);
    }

    fn visit_expr_method_call(&mut self, m: &'ast syn::ExprMethodCall) {
        self.0.push(Call::Method(m.method.to_string()));
        visit::visit_expr_method_call(self, m);
    }

    // `syn` leaves macro bodies as tokens. Most macros that matter here
    // (assert_eq!, format!, println!) take comma-separated expressions.
    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        if let Ok(args) = mac.parse_body_with(Punctuated::<Expr, Token![,]>::parse_terminated) {
            args.iter().for_each(|e| self.visit_expr(e));
        }
    }

    // A function defined inside another one is recorded on its own, not as a call.
    fn visit_item_fn(&mut self, _: &'ast syn::ItemFn) {}
}
