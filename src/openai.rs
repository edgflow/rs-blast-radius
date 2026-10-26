//! A small client for the one OpenAI endpoint this project needs: model
//! responses (`POST /responses`) with structured JSON output.
//!
//! Plain `reqwest` + `serde`: the request and response types below are the
//! subset of the API we use, so every field on the wire is visible here.

use std::time::Duration;

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use thiserror::Error;

pub const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";

/// Attempts per request when OpenAI is rate limiting or briefly failing.
const MAX_ATTEMPTS: u32 = 3;

#[derive(Debug, Error)]
pub enum OpenAiError {
    /// The key is missing, wrong or revoked. The key itself is never echoed.
    #[error("OpenAI rejected the API key (check OPENAI_API_KEY)")]
    Unauthorized,

    #[error("OpenAI request failed ({status}): {message}")]
    Api { status: u16, message: String },

    #[error("cannot reach OpenAI: {0}")]
    Network(#[from] reqwest::Error),

    #[error("the model refused to answer: {0}")]
    Refused(String),

    #[error("unexpected response from OpenAI: {0}")]
    BadResponse(String),
}

#[derive(Clone)]
pub struct OpenAi {
    http: reqwest::Client,
    api_key: String,
    base_url: String,
    retry_delay: Duration,
}

/// A request for a JSON answer that must match `schema` (structured output).
pub struct JsonRequest<'a> {
    pub model: &'a str,
    pub instructions: &'a str,
    pub input: &'a str,
    pub schema_name: &'a str,
    pub schema: serde_json::Value,
}

impl OpenAi {
    pub fn new(api_key: impl Into<String>, base_url: impl Into<String>) -> Self {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(90))
            .build()
            .expect("the HTTP client configuration is valid");
        Self {
            http,
            api_key: api_key.into(),
            base_url: base_url.into().trim_end_matches('/').to_string(),
            retry_delay: Duration::from_millis(500),
        }
    }

    /// The first wait between attempts (doubled each time). Tests shorten it.
    pub fn with_retry_delay(mut self, delay: Duration) -> Self {
        self.retry_delay = delay;
        self
    }

    /// Asks a model for an answer as JSON matching `request.schema`, parsed into `T`.
    pub async fn respond_json<T: DeserializeOwned>(
        &self,
        request: &JsonRequest<'_>,
    ) -> Result<T, OpenAiError> {
        let body = ResponseRequest {
            model: request.model,
            instructions: request.instructions,
            input: request.input,
            text: TextOptions {
                format: Format {
                    kind: "json_schema",
                    name: request.schema_name,
                    schema: &request.schema,
                    strict: true,
                },
            },
            reasoning: Reasoning { effort: "low" },
            max_output_tokens: 4000,
            // Don't keep the response (and the code in it) on OpenAI's side.
            store: false,
        };
        let response: ResponseBody = self.post("responses", &body).await?;
        if response.status != "completed" {
            return Err(OpenAiError::BadResponse(format!(
                "response {}",
                response.status
            )));
        }
        let mut text = String::new();
        for item in response.output {
            // Reasoning and other item types are skipped; only messages carry the answer.
            if let OutputItem::Message { content } = item {
                for part in content {
                    match part {
                        ContentPart::OutputText { text: t } => text.push_str(&t),
                        ContentPart::Refusal { refusal } => {
                            return Err(OpenAiError::Refused(refusal));
                        }
                        ContentPart::Other => {}
                    }
                }
            }
        }
        serde_json::from_str(&text)
            .map_err(|e| OpenAiError::BadResponse(format!("answer is not the expected JSON: {e}")))
    }

    /// POSTs JSON, retrying rate limits (429), server errors (5xx) and timeouts.
    async fn post<B: Serialize, R: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<R, OpenAiError> {
        let url = format!("{}/{path}", self.base_url);
        let mut attempt = 1;
        loop {
            match self
                .http
                .post(&url)
                .bearer_auth(&self.api_key)
                .json(body)
                .send()
                .await
            {
                Ok(response) if response.status().is_success() => {
                    return response
                        .json()
                        .await
                        .map_err(|e| OpenAiError::BadResponse(e.to_string()));
                }
                Ok(response) => {
                    let status = response.status();
                    if status == reqwest::StatusCode::UNAUTHORIZED {
                        return Err(OpenAiError::Unauthorized);
                    }
                    let retryable = status == reqwest::StatusCode::TOO_MANY_REQUESTS
                        || status.is_server_error();
                    if !retryable || attempt == MAX_ATTEMPTS {
                        return Err(api_error(response).await);
                    }
                    eprintln!("OpenAI request failed ({status}); retrying (attempt {attempt})");
                }
                Err(error)
                    if (error.is_timeout() || error.is_connect()) && attempt < MAX_ATTEMPTS =>
                {
                    eprintln!("OpenAI unreachable ({error}); retrying (attempt {attempt})");
                }
                Err(error) => return Err(OpenAiError::Network(error)),
            }
            tokio::time::sleep(self.retry_delay * 2u32.pow(attempt - 1)).await;
            attempt += 1;
        }
    }
}

/// The API's error message (`{"error": {"message": ...}}`), or the status alone.
async fn api_error(response: reqwest::Response) -> OpenAiError {
    let status = response.status().as_u16();
    let message = response
        .json::<ErrorBody>()
        .await
        .map(|body| body.error.message)
        .unwrap_or_else(|_| "no error details".into());
    OpenAiError::Api { status, message }
}

// ---- wire types ----------------------------------------------------------

#[derive(Serialize)]
struct ResponseRequest<'a> {
    model: &'a str,
    instructions: &'a str,
    input: &'a str,
    text: TextOptions<'a>,
    reasoning: Reasoning,
    max_output_tokens: u32,
    store: bool,
}

#[derive(Serialize)]
struct TextOptions<'a> {
    format: Format<'a>,
}

#[derive(Serialize)]
struct Format<'a> {
    #[serde(rename = "type")]
    kind: &'a str,
    name: &'a str,
    schema: &'a serde_json::Value,
    strict: bool,
}

#[derive(Serialize)]
struct Reasoning {
    effort: &'static str,
}

#[derive(Deserialize)]
struct ResponseBody {
    status: String,
    output: Vec<OutputItem>,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum OutputItem {
    Message {
        content: Vec<ContentPart>,
    },
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ContentPart {
    OutputText {
        text: String,
    },
    Refusal {
        refusal: String,
    },
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
struct ErrorBody {
    error: ErrorDetail,
}

#[derive(Deserialize)]
struct ErrorDetail {
    message: String,
}
