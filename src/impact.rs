//! The blast radius of a change: everything that reaches the changed function
//! through the call graph, and a risk score computed from it. All deterministic:
//! the same code always gives the same radius and the same score.

use std::collections::{BTreeSet, VecDeque};

use crate::graph::{Confidence, Graph};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Affected {
    pub id: usize,
    /// 1 = calls the changed function directly, 2 = calls a direct caller, …
    pub depth: usize,
    /// The next function on the way to the changed one.
    pub next: usize,
    /// The weakest edge on that way.
    pub confidence: Confidence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone)]
pub struct Risk {
    pub score: u32,
    pub level: Level,
    /// Why: one sentence per thing that added to the score.
    pub reasons: Vec<String>,
}

pub struct Impact {
    pub target: usize,
    pub affected: Vec<Affected>,
    pub risk: Risk,
}

impl Impact {
    /// The chain of calls from `id` down to the changed function.
    pub fn path(&self, id: usize) -> Vec<usize> {
        let mut path = vec![id];
        let mut current = id;
        while current != self.target {
            current = self
                .affected
                .iter()
                .find(|a| a.id == current)
                .map(|a| a.next)
                .unwrap_or(self.target);
            path.push(current);
        }
        path
    }

    pub fn tests<'a>(&'a self, graph: &'a Graph) -> impl Iterator<Item = &'a Affected> {
        self.affected.iter().filter(|a| graph.is_test(a.id))
    }

    pub fn endpoints<'a>(&'a self, graph: &'a Graph) -> impl Iterator<Item = &'a Affected> {
        self.affected.iter().filter(|a| graph.is_endpoint(a.id))
    }
}

pub fn blast_radius(graph: &Graph, target: usize) -> Impact {
    let mut affected: Vec<Affected> = Vec::new();
    let found =
        |affected: &[Affected], id: usize| id == target || affected.iter().any(|a| a.id == id);
    // Most confident first: a function reached through certain calls is listed as
    // certain, even if a weaker chain reaches it too.
    for level in [
        Confidence::Certain,
        Confidence::Likely,
        Confidence::Possible,
    ] {
        let mut queue = VecDeque::from([(target, 0)]);
        let mut seen = BTreeSet::from([target]);
        while let Some((id, depth)) = queue.pop_front() {
            for caller in &graph.callers[id] {
                if caller.confidence < level || !seen.insert(caller.other) {
                    continue;
                }
                if !found(&affected, caller.other) {
                    affected.push(Affected {
                        id: caller.other,
                        depth: depth + 1,
                        next: id,
                        confidence: level,
                    });
                }
                queue.push_back((caller.other, depth + 1));
            }
        }
    }
    affected.sort_by(|a, b| {
        (a.depth, &graph.functions[a.id].name).cmp(&(b.depth, &graph.functions[b.id].name))
    });
    let risk = assess(graph, target, &affected);
    Impact {
        target,
        affected,
        risk,
    }
}

/// A transparent score: every point comes with a reason a person can check.
fn assess(graph: &Graph, target: usize, affected: &[Affected]) -> Risk {
    let solid: Vec<&Affected> = affected
        .iter()
        .filter(|a| a.confidence >= Confidence::Likely)
        .collect();
    let code: Vec<&&Affected> = solid.iter().filter(|a| !graph.is_test(a.id)).collect();
    let direct = code.iter().filter(|a| a.depth == 1).count();
    let endpoints = solid.iter().filter(|a| graph.is_endpoint(a.id)).count();
    let tests = affected.iter().filter(|a| graph.is_test(a.id)).count();
    let modules: BTreeSet<&str> = code
        .iter()
        .filter_map(|a| graph.functions[a.id].module.first().map(String::as_str))
        .collect();

    let mut score = 0;
    let mut reasons = Vec::new();
    if direct > 0 {
        score += 10 * direct.min(3) as u32;
        reasons.push(format!("{direct} function(s) call it directly"));
    }
    if !code.is_empty() {
        score += 3 * code.len().min(10) as u32;
        reasons.push(format!(
            "{} function(s) depend on it, across {} module(s)",
            code.len(),
            modules.len()
        ));
    }
    if endpoints > 0 {
        score += 15 * endpoints.min(2) as u32;
        reasons.push(format!(
            "{endpoints} HTTP endpoint(s) reach it: their clients are outside this codebase"
        ));
    }
    if graph.functions[target].public {
        score += 10;
        reasons.push("it is public: code outside this repository may call it too".into());
    }
    if tests == 0 {
        score += 20;
        reasons.push("no test reaches it: a mistake would not be caught".into());
    } else {
        reasons.push(format!("{tests} test(s) reach it"));
    }
    let score = score.min(100);
    let level = match score {
        60.. => Level::High,
        30.. => Level::Medium,
        _ => Level::Low,
    };
    Risk {
        score,
        level,
        reasons,
    }
}
