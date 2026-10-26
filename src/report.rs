//! Turning an analysis into text for people.

use std::fmt::Write;

use crate::graph::{Confidence, Graph};
use crate::impact::{Impact, Level};
use crate::parse::Kind;
use crate::predict::Prediction;

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

/// The full report: the model's prediction, checked against the graph, then the
/// static analysis it was based on.
pub fn prediction_markdown(
    graph: &Graph,
    impact: &Impact,
    prediction: &Prediction,
    change: &str,
) -> String {
    let target = &graph.functions[impact.target];
    let r = &prediction.report;
    let mut out = String::new();
    writeln!(out, "# Blast radius: `{}`\n", target.name).unwrap();
    writeln!(out, "**Change:** {}\n", change.trim()).unwrap();
    writeln!(
        out,
        "**Risk: {}** · call graph {} ({}/100) · model {}\n",
        level_name(prediction.level),
        level_name(impact.risk.level).to_lowercase(),
        impact.risk.score,
        level_name(prediction.model_level).to_lowercase(),
    )
    .unwrap();
    writeln!(out, "> {}\n", r.summary.trim()).unwrap();
    writeln!(out, "## What may break\n").unwrap();
    if r.breaking.is_empty() {
        writeln!(out, "Nothing the model could name.\n").unwrap();
    } else {
        writeln!(out, "| Function | Likelihood | Why |\n|---|---|---|").unwrap();
        for f in &r.breaking {
            writeln!(
                out,
                "| `{}` | {} | {} |",
                f.function,
                f.likelihood,
                f.why.replace('|', "\\|")
            )
            .unwrap();
        }
        writeln!(out).unwrap();
    }
    if !r.api_impact.is_empty() {
        writeln!(out, "## API consumers\n").unwrap();
        for a in &r.api_impact {
            writeln!(out, "- **{}**: {}", a.endpoint, a.impact).unwrap();
        }
        writeln!(out).unwrap();
    }
    writeln!(out, "## Tests to run\n").unwrap();
    for t in &r.tests_to_run {
        writeln!(out, "- `{t}`").unwrap();
    }
    if r.tests_to_run.is_empty() {
        writeln!(
            out,
            "- none reach this function: add one before changing it"
        )
        .unwrap();
    }
    writeln!(out, "\n## Recommendations\n").unwrap();
    for (i, rec) in r.recommendations.iter().enumerate() {
        writeln!(out, "{}. {rec}", i + 1).unwrap();
    }
    writeln!(out, "\n## Static analysis\n").unwrap();
    for reason in &impact.risk.reasons {
        writeln!(out, "- {reason}").unwrap();
    }
    writeln!(out, "\n| Depth | Confidence | Path |\n|---|---|---|").unwrap();
    for a in &impact.affected {
        let path: Vec<&str> = impact
            .path(a.id)
            .iter()
            .map(|&i| graph.functions[i].name.as_str())
            .collect();
        writeln!(
            out,
            "| {} | {} | {} |",
            a.depth,
            confidence_name(a.confidence),
            path.join(" → ")
        )
        .unwrap();
    }
    if !prediction.rejected.is_empty() {
        writeln!(out, "\n## Removed from the model's answer\n").unwrap();
        for r in &prediction.rejected {
            writeln!(out, "- {r}").unwrap();
        }
    }
    out
}
