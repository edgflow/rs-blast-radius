//! The call graph: one node per function, one edge per call we can resolve.
//!
//! Without compiling there is no type information, so each edge says how sure we
//! are: a path call that resolves through `use` items and modules is certain; a
//! method call is matched by name only (`cart.total()` → some `total` method).

use std::collections::HashMap;

use crate::parse::{Call, Codebase, FnDef, Kind, Scope};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Confidence {
    /// A method name that several of our types share.
    Possible,
    /// A method name only one of our types has, or a path matched by its ending.
    Likely,
    /// A path that resolves exactly, through modules and `use` items.
    Certain,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Edge {
    pub other: usize,
    pub confidence: Confidence,
}

pub struct Graph {
    pub functions: Vec<FnDef>,
    /// `callees[f]`: what f calls. `callers[f]`: who calls f.
    pub callees: Vec<Vec<Edge>>,
    pub callers: Vec<Vec<Edge>>,
    /// Calls to code outside the codebase: std, other crates, `Some(…)`, `Ok(…)`.
    pub external_calls: usize,
}

/// Method names std types share: `.map()` or `.len()` says nothing about our code.
const STD_METHODS: &[&str] = &[
    "as_ref",
    "as_str",
    "clone",
    "collect",
    "contains",
    "err",
    "expect",
    "filter",
    "get",
    "insert",
    "into",
    "into_iter",
    "is_empty",
    "is_err",
    "is_ok",
    "iter",
    "len",
    "map",
    "ok",
    "pop",
    "push",
    "remove",
    "sum",
    "to_owned",
    "to_string",
    "unwrap",
    "unwrap_or",
];

impl Graph {
    pub fn build(codebase: &Codebase) -> Graph {
        let functions = codebase.functions.clone();
        let index: HashMap<&str, usize> = functions
            .iter()
            .enumerate()
            .map(|(i, f)| (f.name.as_str(), i))
            .collect();
        let n = functions.len();
        let mut callees: Vec<Vec<Edge>> = vec![Vec::new(); n];
        let mut external_calls = 0;
        let empty = Scope::default();
        for (from, f) in functions.iter().enumerate() {
            let scope = codebase.scopes.get(&f.module).unwrap_or(&empty);
            for call in &f.calls {
                let targets = match call {
                    Call::Path(segments) => {
                        resolve_path(segments, f, scope, &codebase.crate_name, &functions, &index)
                    }
                    Call::Method(name) => resolve_method(name, &functions),
                };
                if targets.is_empty() {
                    external_calls += 1;
                }
                for edge in targets {
                    add_edge(&mut callees[from], edge);
                }
            }
        }
        let mut callers: Vec<Vec<Edge>> = vec![Vec::new(); n];
        for (from, edges) in callees.iter().enumerate() {
            for e in edges {
                add_edge(
                    &mut callers[e.other],
                    Edge {
                        other: from,
                        confidence: e.confidence,
                    },
                );
            }
        }
        Graph {
            functions,
            callees,
            callers,
            external_calls,
        }
    }

    /// A function by its qualified name, or by a unique ending: `apply_discount`.
    pub fn find(&self, name: &str) -> Result<usize, String> {
        if let Some(i) = self.functions.iter().position(|f| f.name == name) {
            return Ok(i);
        }
        let suffix = format!("::{name}");
        let matches: Vec<usize> = (0..self.functions.len())
            .filter(|&i| self.functions[i].name.ends_with(&suffix))
            .collect();
        match matches.as_slice() {
            [one] => Ok(*one),
            [] => Err(format!("no function named `{name}`")),
            many => Err(format!(
                "`{name}` is ambiguous: {}",
                many.iter()
                    .map(|&i| self.functions[i].name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
        }
    }

    pub fn is_test(&self, id: usize) -> bool {
        self.functions[id].kind == Kind::Test
    }

    pub fn is_endpoint(&self, id: usize) -> bool {
        matches!(self.functions[id].kind, Kind::Endpoint { .. })
    }
}

/// Keeps one edge per pair, with the highest confidence seen.
fn add_edge(edges: &mut Vec<Edge>, edge: Edge) {
    match edges.iter_mut().find(|e| e.other == edge.other) {
        Some(e) => e.confidence = e.confidence.max(edge.confidence),
        None => edges.push(edge),
    }
}

/// `crate::x`, `self::x`, `super::x`, `Self::x` → a path from the crate root.
fn normalize(path: &[String], f: &FnDef, crate_name: &str) -> Option<Vec<String>> {
    let first = path.first()?.as_str();
    let rest = &path[1..];
    let mut out = match first {
        "crate" => Vec::new(),
        _ if first == crate_name => Vec::new(),
        "self" => f.module.clone(),
        "Self" => {
            let mut p = f.module.clone();
            p.push(f.owner.clone()?);
            p
        }
        "super" => {
            let mut p = f.module.clone();
            p.pop();
            let mut rest = rest;
            while rest.first().is_some_and(|s| s == "super") {
                p.pop();
                rest = &rest[1..];
            }
            p.extend(rest.iter().cloned());
            return Some(p);
        }
        _ => return None,
    };
    out.extend(rest.iter().cloned());
    Some(out)
}

fn resolve_path(
    segments: &[String],
    f: &FnDef,
    scope: &Scope,
    crate_name: &str,
    functions: &[FnDef],
    index: &HashMap<&str, usize>,
) -> Vec<Edge> {
    // Expand a `use` alias: `apply_discount` → `crate::pricing::apply_discount`.
    let mut path = segments.to_vec();
    if let Some(alias) = scope.aliases.get(&path[0]) {
        path = alias.iter().chain(&path[1..]).cloned().collect();
    }
    // Where the path could point, most specific first.
    let candidates: Vec<Vec<String>> = match normalize(&path, f, crate_name) {
        Some(absolute) => vec![absolute],
        None => {
            let mut c = vec![f.module.iter().chain(&path).cloned().collect()];
            for glob in &scope.globs {
                if let Some(base) = normalize(glob, f, crate_name).or_else(|| Some(glob.clone())) {
                    c.push(base.into_iter().chain(path.iter().cloned()).collect());
                }
            }
            c.push(path.clone());
            c
        }
    };
    for candidate in &candidates {
        if let Some(&i) = index.get(candidate.join("::").as_str()) {
            return vec![Edge {
                other: i,
                confidence: Confidence::Certain,
            }];
        }
    }
    // No exact match: a function whose name ends with the path, if there is just one.
    let tail = candidates.last().map(|c| c.join("::")).unwrap_or_default();
    let suffix = format!("::{tail}");
    let matches: Vec<usize> = (0..functions.len())
        .filter(|&i| functions[i].name == tail || functions[i].name.ends_with(&suffix))
        .collect();
    match matches.as_slice() {
        [one] => vec![Edge {
            other: *one,
            confidence: Confidence::Likely,
        }],
        _ => Vec::new(),
    }
}

fn resolve_method(name: &str, functions: &[FnDef]) -> Vec<Edge> {
    if STD_METHODS.contains(&name) {
        return Vec::new();
    }
    let matches: Vec<usize> = (0..functions.len())
        .filter(|&i| functions[i].owner.is_some() && functions[i].ident == name)
        .collect();
    let confidence = if matches.len() == 1 {
        Confidence::Likely
    } else {
        Confidence::Possible
    };
    matches
        .into_iter()
        .map(|i| Edge {
            other: i,
            confidence,
        })
        .collect()
}
