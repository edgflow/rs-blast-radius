//! Turning an analysis into text for people.

use std::fmt::Write;

use crate::graph::{Confidence, Graph};
use crate::impact::{Impact, Level};
use crate::parse::Kind;

pub fn level_name(level: Level) -> &'static str {
    match level {
        Level::Low => "LOW",
        Level::Medium => "MEDIUM",
        Level::High => "HIGH",
    }
}

pub fn confidence_name(confidence: Confidence) -> &'static str {
    match confidence {
        Confidence::Certain => "certain",
        Confidence::Likely => "likely",
        Confidence::Possible => "possible",
    }
}

/// "GET /cart/quote" for an endpoint, the qualified name for anything else.
pub fn label(graph: &Graph, id: usize) -> String {
    match &graph.functions[id].kind {
        Kind::Endpoint { method, route } => format!("{method} {route}"),
        _ => graph.functions[id].name.clone(),
    }
}

/// The static analysis on its own: what reaches the function, and the risk score.
pub fn impact_text(graph: &Graph, impact: &Impact) -> String {
    let target = &graph.functions[impact.target];
    let mut out = String::new();
    writeln!(
        out,
        "{}  ({}:{})",
        target.name,
        target.file.display(),
        target.line
    )
    .unwrap();
    writeln!(
        out,
        "Risk: {} ({}/100)",
        level_name(impact.risk.level),
        impact.risk.score
    )
    .unwrap();
    for reason in &impact.risk.reasons {
        writeln!(out, "  - {reason}").unwrap();
    }
    writeln!(out, "\nWhat reaches it ({}):", impact.affected.len()).unwrap();
    for a in &impact.affected {
        let path: Vec<&str> = impact
            .path(a.id)
            .iter()
            .map(|&i| graph.functions[i].name.as_str())
            .collect();
        writeln!(
            out,
            "  {}  {:9} {}",
            a.depth,
            confidence_name(a.confidence),
            path.join(" → ")
        )
        .unwrap();
    }
    let tests: Vec<String> = impact
        .tests(graph)
        .map(|a| graph.functions[a.id].name.clone())
        .collect();
    let endpoints: Vec<String> = impact
        .endpoints(graph)
        .map(|a| label(graph, a.id))
        .collect();
    writeln!(
        out,
        "\nTests to run: {}",
        if tests.is_empty() {
            "none reach it".into()
        } else {
            tests.join(", ")
        }
    )
    .unwrap();
    writeln!(
        out,
        "API endpoints: {}",
        if endpoints.is_empty() {
            "none".into()
        } else {
            endpoints.join(", ")
        }
    )
    .unwrap();
    out
}
