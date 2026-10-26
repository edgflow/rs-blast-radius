//! The AI report, against a local stand-in for the OpenAI API: what is sent, and
//! how the answer is checked against the call graph. No network, no API key.

use std::path::Path;

use blast_radius::graph::Graph;
use blast_radius::impact::{Level, blast_radius};
use blast_radius::openai::OpenAi;
use blast_radius::parse::parse_dir;
use blast_radius::predict::predict;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const CHANGE: &str = "Return Result<u64, PricingError> and reject percentages above 100.";

/// What the model "answers": one real finding, plus things the graph doesn't support.
fn model_answer() -> serde_json::Value {
    json!({
        "summary": "Every caller must now handle an error.",
        "risk": "medium",
        "breaking": [
            { "function": "cart::Cart::total", "likelihood": "high", "why": "It adds the result to VAT as a u64." },
            { "function": "inventory::reserve", "likelihood": "low", "why": "Invented: stock has nothing to do with discounts." },
            { "function": "billing::invoice", "likelihood": "medium", "why": "Invented: there is no billing module." }
        ],
        "tests_to_run": ["tests::checkout::quote_includes_vat", "tests::discounts_still_work"],
        "api_impact": [
            { "endpoint": "GET /cart/quote", "impact": "Must map the error to a 400 response." },
            { "endpoint": "POST /inventory/{sku}/restock", "impact": "Invented." }
        ],
        "recommendations": ["Add a test for 101%."]
    })
}

async fn stand_in() -> MockServer {
    let server = MockServer::start().await;
    let body = json!({
        "status": "completed",
        "output": [
            { "type": "reasoning", "summary": [] },
            { "type": "message", "content": [{ "type": "output_text", "text": model_answer().to_string() }] }
        ]
    });
    Mock::given(method("POST"))
        .and(path("/responses"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .expect(1)
        .mount(&server)
        .await;
    server
}

#[tokio::test]
async fn the_answer_is_checked_against_the_call_graph() {
    let server = stand_in().await;
    let codebase = parse_dir(Path::new("examples/shop")).unwrap();
    let graph = Graph::build(&codebase);
    let impact = blast_radius(&graph, graph.find("apply_discount").unwrap());
    let ai = OpenAi::new("test-key", server.uri());

    let prediction = predict(&ai, "test-model", &codebase, &graph, &impact, CHANGE)
        .await
        .unwrap();

    let kept: Vec<&str> = prediction
        .report
        .breaking
        .iter()
        .map(|f| f.function.as_str())
        .collect();
    assert_eq!(kept, ["cart::Cart::total"]);
    assert_eq!(
        prediction.report.tests_to_run,
        ["tests::checkout::quote_includes_vat"]
    );
    assert_eq!(prediction.report.api_impact.len(), 1);
    assert_eq!(prediction.rejected.len(), 4);
    assert!(
        prediction
            .rejected
            .iter()
            .any(|r| r.contains("inventory::reserve") && r.contains("doesn't reach it"))
    );
    assert!(
        prediction
            .rejected
            .iter()
            .any(|r| r.contains("billing::invoice") && r.contains("no such function"))
    );
    // The model said medium; the graph says high. The report keeps the higher.
    assert_eq!(
        (prediction.model_level, prediction.level),
        (Level::Medium, Level::High)
    );
}

#[tokio::test]
async fn only_the_blast_radius_is_sent_and_nothing_is_stored() {
    let server = stand_in().await;
    let codebase = parse_dir(Path::new("examples/shop")).unwrap();
    let graph = Graph::build(&codebase);
    let impact = blast_radius(&graph, graph.find("apply_discount").unwrap());
    let ai = OpenAi::new("test-key", server.uri());
    predict(&ai, "test-model", &codebase, &graph, &impact, CHANGE)
        .await
        .unwrap();

    let requests = server.received_requests().await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    let input = body["input"].as_str().unwrap();
    assert!(input.contains(CHANGE));
    assert!(input.contains("pub fn apply_discount"));
    assert!(input.contains("api::post_checkout → checkout::place_order"));
    assert!(
        !input.contains("fn restock"),
        "code outside the blast radius was sent"
    );
    assert_eq!(body["store"], false);
    assert_eq!(body["text"]["format"]["strict"], true);
}
