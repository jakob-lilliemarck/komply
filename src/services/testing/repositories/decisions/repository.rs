use std::time::Duration;

use crate::services::testing::model::EvaluationRequest;

use super::errors::Error;
use super::models::{Decision, WireResponse, decision_from_response};

const DEFAULT_URL: &str = "https://openrouter.ai/api/alpha/decisions";
const MAX_RETRIES: u32 = 3;

#[derive(Clone, Copy)]
struct RetryDelays {
    initial: Duration,
    max: Duration,
    max_jitter: Duration,
}

#[cfg(not(test))]
const RETRY_DELAYS: RetryDelays = RetryDelays {
    initial: Duration::from_secs(1),
    max: Duration::from_secs(30),
    max_jitter: Duration::from_millis(250),
};

#[cfg(test)]
const RETRY_DELAYS: RetryDelays = RetryDelays {
    initial: Duration::ZERO,
    max: Duration::ZERO,
    max_jitter: Duration::ZERO,
};

/// Sends one decisions request and returns the validated `Decision`.
pub struct DecisionsRepository {
    api_key: String,
    base_url: String,
    agent: ureq::Agent,
}

impl DecisionsRepository {
    /// Creates a repository from an injected key, endpoint, and timeout.
    ///
    /// It reads no environment.
    pub fn new(api_key: impl Into<String>, base_url: impl Into<String>, timeout: Duration) -> Self {
        let agent = ureq::AgentBuilder::new().timeout(timeout).build();
        Self {
            api_key: api_key.into(),
            base_url: base_url.into(),
            agent,
        }
    }

    /// Creates a repository from `OPENROUTER_API_KEY` and an optional
    /// `COMPLY_DECISIONS_URL` override.
    ///
    /// A missing or empty key is [`Error::MissingApiKey`]. A missing or empty
    /// URL means the production endpoint.
    pub fn from_env(timeout: Duration) -> Result<Self, Error> {
        Self::from_lookup(timeout, |key| std::env::var(key).ok())
    }

    fn from_lookup<F>(timeout: Duration, lookup: F) -> Result<Self, Error>
    where
        F: Fn(&str) -> Option<String>,
    {
        let api_key = lookup("OPENROUTER_API_KEY")
            .filter(|value| !value.is_empty())
            .ok_or(Error::MissingApiKey)?;
        let base_url = lookup("COMPLY_DECISIONS_URL")
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| DEFAULT_URL.to_owned());
        Ok(Self::new(api_key, base_url, timeout))
    }

    /// Sends one request to the decisions endpoint.
    ///
    /// Transient failures are retried at most three times. The returned
    /// `Decision` contains only the requested keys, each validated as a finite
    /// `noul` probability in `[0, 1]`.
    pub fn evaluate(&self, request: &EvaluationRequest) -> Result<Decision, Error> {
        let keys: Vec<String> = request
            .questions
            .iter()
            .map(|question| question.key.clone())
            .collect();
        let body = request.to_json(None);

        for attempt in 0..=MAX_RETRIES {
            match self.send_once(&body, &keys) {
                Ok(decision) => return Ok(decision),
                Err(failure) if failure.retryable && attempt < MAX_RETRIES => {
                    let wait = failure
                        .retry_after
                        .unwrap_or_else(|| retry_delay(attempt + 1));
                    std::thread::sleep(wait);
                }
                Err(failure) => return Err(failure.error),
            }
        }

        Err(Error::Transport(
            "retry loop ended without a result".to_owned(),
        ))
    }

    fn send_once(&self, body: &serde_json::Value, keys: &[String]) -> Result<Decision, Failure> {
        let response = self
            .agent
            .post(&self.base_url)
            .set("Authorization", &format!("Bearer {}", self.api_key))
            .set("Content-Type", "application/json")
            .send_json(body);

        let response = match response {
            Ok(response) => response,
            Err(ureq::Error::Status(status, response)) => {
                let retry_after = response.header("Retry-After").and_then(parse_retry_after);
                let body = response.into_string().unwrap_or_default();
                let error = Error::Status {
                    status,
                    body: body.clone(),
                };
                return Err(Failure {
                    error,
                    retryable: is_retryable(status, &body),
                    retry_after,
                });
            }
            Err(ureq::Error::Transport(error)) => {
                return Err(Failure {
                    error: Error::Transport(error.to_string()),
                    retryable: false,
                    retry_after: None,
                });
            }
        };

        let text = response.into_string().map_err(|error| Failure {
            error: Error::Decode(error.to_string()),
            retryable: false,
            retry_after: None,
        })?;
        let wire: WireResponse = serde_json::from_str(&text).map_err(|error| Failure {
            error: Error::Decode(error.to_string()),
            retryable: false,
            retry_after: None,
        })?;

        decision_from_response(wire, keys).map_err(|error| Failure {
            error,
            retryable: false,
            retry_after: None,
        })
    }
}

struct Failure {
    error: Error,
    retryable: bool,
    retry_after: Option<Duration>,
}

fn is_retryable(status: u16, body: &str) -> bool {
    status == 429
        || (500..=599).contains(&status)
        || (status == 402 && body.contains("openrouter_in_flight_budget"))
}

fn parse_retry_after(value: &str) -> Option<Duration> {
    value.trim().parse::<u64>().ok().map(Duration::from_secs)
}

fn retry_delay(attempt: u32) -> Duration {
    retry_delay_for(attempt, RETRY_DELAYS)
}

fn retry_delay_for(attempt: u32, delays: RetryDelays) -> Duration {
    let exponent = attempt.saturating_sub(1);
    let base = delays
        .initial
        .saturating_mul(2u32.saturating_pow(exponent))
        .min(delays.max);
    base + jitter(attempt, delays.max_jitter)
}

fn jitter(attempt: u32, max_jitter: Duration) -> Duration {
    let max_millis = max_jitter.as_millis() as u64;
    if max_millis == 0 {
        return Duration::ZERO;
    }
    Duration::from_millis((u64::from(attempt) * 37) % max_millis)
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::{Arc, Mutex};
    use std::thread::{self, JoinHandle};

    use crate::services::testing::model::EvaluationQuestion;

    use super::*;

    const OK_BODY: &str = r#"{"answers":{"r0":{"type":"noul","noul":0.1}},"usage":{"cost":0.25,"input_tokens":100,"output_tokens":20}}"#;

    struct StubResponse {
        status: u16,
        headers: Vec<(String, String)>,
        body: String,
    }

    impl StubResponse {
        fn ok(body: &str) -> Self {
            Self {
                status: 200,
                headers: Vec::new(),
                body: body.to_owned(),
            }
        }

        fn status(status: u16, body: &str) -> Self {
            Self {
                status,
                headers: Vec::new(),
                body: body.to_owned(),
            }
        }

        fn with_header(mut self, name: &str, value: &str) -> Self {
            self.headers.push((name.to_owned(), value.to_owned()));
            self
        }
    }

    struct StubServer {
        url: String,
        requests: Arc<Mutex<Vec<String>>>,
        handle: Option<JoinHandle<()>>,
    }

    impl StubServer {
        fn new(responses: Vec<StubResponse>) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let url = format!("http://{address}/decisions");
            let requests = Arc::new(Mutex::new(Vec::new()));
            let captured = Arc::clone(&requests);

            let handle = thread::spawn(move || {
                for response in responses {
                    let (mut stream, _) = listener.accept().unwrap();
                    let body = read_request(&mut stream);
                    captured
                        .lock()
                        .unwrap()
                        .push(String::from_utf8_lossy(&body).into_owned());
                    write_response(&mut stream, &response);
                }
            });

            Self {
                url,
                requests,
                handle: Some(handle),
            }
        }

        fn url(&self) -> &str {
            &self.url
        }

        fn finish(mut self) -> Vec<serde_json::Value> {
            if let Some(handle) = self.handle.take() {
                handle.join().unwrap();
            }
            let requests = self.requests.lock().unwrap();
            requests
                .iter()
                .map(|body| serde_json::from_str(body).unwrap())
                .collect()
        }
    }

    fn read_request(stream: &mut TcpStream) -> Vec<u8> {
        let mut buffer = Vec::new();
        let mut chunk = [0u8; 1024];

        loop {
            let count = stream.read(&mut chunk).unwrap();
            if count == 0 {
                return buffer;
            }
            buffer.extend_from_slice(&chunk[..count]);

            if let Some(position) = find_header_end(&buffer) {
                let header = String::from_utf8_lossy(&buffer[..position]).to_lowercase();
                let content_length = header
                    .lines()
                    .find_map(|line| {
                        line.trim()
                            .strip_prefix("content-length:")
                            .and_then(|value| value.trim().parse::<usize>().ok())
                    })
                    .unwrap_or(0);
                let total = position + 4 + content_length;
                while buffer.len() < total {
                    let count = stream.read(&mut chunk).unwrap();
                    if count == 0 {
                        break;
                    }
                    buffer.extend_from_slice(&chunk[..count]);
                }
                return buffer[position + 4..].to_vec();
            }
        }
    }

    fn find_header_end(buffer: &[u8]) -> Option<usize> {
        buffer.windows(4).position(|window| window == b"\r\n\r\n")
    }

    fn write_response(stream: &mut TcpStream, response: &StubResponse) {
        let reason = match response.status {
            200 => "OK",
            400 => "Bad Request",
            402 => "Payment Required",
            429 => "Too Many Requests",
            500 => "Internal Server Error",
            _ => "Status",
        };
        let mut head = format!(
            "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n",
            response.status,
            reason,
            response.body.len()
        );
        for (name, value) in &response.headers {
            head.push_str(&format!("{name}: {value}\r\n"));
        }
        head.push_str("\r\n");

        stream.write_all(head.as_bytes()).unwrap();
        stream.write_all(response.body.as_bytes()).unwrap();
        stream.flush().unwrap();
    }

    fn request() -> EvaluationRequest {
        EvaluationRequest {
            model: "test/model".to_owned(),
            state: "STATE".to_owned(),
            questions: vec![EvaluationQuestion {
                key: "r0".to_owned(),
                instruction: "Is it true?".to_owned(),
            }],
        }
    }

    fn repository(url: &str) -> DecisionsRepository {
        DecisionsRepository::new("test-key", url, Duration::from_secs(5))
    }

    #[test]
    fn it_sends_the_request_and_maps_the_decision() {
        let server = StubServer::new(vec![StubResponse::ok(OK_BODY)]);
        let repository = repository(server.url());

        let decision = repository.evaluate(&request()).unwrap();

        assert_eq!(decision.answers["r0"], 0.1);
        assert_eq!(decision.cost_usd, Some(0.25));
        assert_eq!(decision.input_tokens, Some(100));
        assert_eq!(decision.output_tokens, Some(20));

        let requests = server.finish();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0]["model"], "test/model");
        assert_eq!(requests[0]["state"], "STATE");
        assert_eq!(requests[0]["questions"]["r0"]["type"], "noul");
        assert_eq!(
            requests[0]["questions"]["r0"]["instructions"],
            "Is it true?"
        );
    }

    #[test]
    fn it_rejects_a_missing_answer() {
        let server = StubServer::new(vec![StubResponse::ok(r#"{"answers":{}}"#)]);
        let repository = repository(server.url());

        let error = repository.evaluate(&request()).unwrap_err();

        assert!(matches!(error, Error::MissingAnswer(key) if key == "r0"));
        server.finish();
    }

    #[test]
    fn it_rejects_a_wrong_type() {
        let server = StubServer::new(vec![StubResponse::ok(
            r#"{"answers":{"r0":{"type":"bool","noul":0.1}}}"#,
        )]);
        let repository = repository(server.url());

        let error = repository.evaluate(&request()).unwrap_err();

        assert!(matches!(error, Error::MissingAnswer(key) if key == "r0"));
        server.finish();
    }

    #[test]
    fn it_rejects_an_invalid_probability() {
        let server = StubServer::new(vec![StubResponse::ok(
            r#"{"answers":{"r0":{"type":"noul","noul":1.5}}}"#,
        )]);
        let repository = repository(server.url());

        let error = repository.evaluate(&request()).unwrap_err();

        assert!(matches!(
            error,
            Error::InvalidProbability { key, value }
                if key == "r0" && value == 1.5
        ));
        server.finish();
    }

    #[test]
    fn it_rejects_a_malformed_response() {
        let server = StubServer::new(vec![StubResponse::ok("not json")]);
        let repository = repository(server.url());

        let error = repository.evaluate(&request()).unwrap_err();

        assert!(matches!(error, Error::Decode(_)));
        server.finish();
    }

    #[test]
    fn it_retries_a_429_then_succeeds() {
        let server = StubServer::new(vec![
            StubResponse::status(429, r#"{"error":"rate"}"#),
            StubResponse::ok(OK_BODY),
        ]);
        let repository = repository(server.url());

        let decision = repository.evaluate(&request()).unwrap();

        assert_eq!(decision.answers["r0"], 0.1);
        assert_eq!(server.finish().len(), 2);
    }

    #[test]
    fn it_retries_a_500_then_succeeds() {
        let server = StubServer::new(vec![
            StubResponse::status(500, r#"{"error":"boom"}"#),
            StubResponse::ok(OK_BODY),
        ]);
        let repository = repository(server.url());

        let decision = repository.evaluate(&request()).unwrap();

        assert_eq!(decision.answers["r0"], 0.1);
        assert_eq!(server.finish().len(), 2);
    }

    #[test]
    fn it_does_not_retry_a_400() {
        let server = StubServer::new(vec![StubResponse::status(400, r#"{"error":"bad"}"#)]);
        let repository = repository(server.url());

        let error = repository.evaluate(&request()).unwrap_err();

        match error {
            Error::Status { status, body } => {
                assert_eq!(status, 400);
                assert_eq!(body, r#"{"error":"bad"}"#);
            }
            other => panic!("unexpected error: {other}"),
        }
        assert_eq!(server.finish().len(), 1);
    }

    #[test]
    fn it_retries_an_in_flight_budget_402() {
        let server = StubServer::new(vec![
            StubResponse::status(
                402,
                r#"{"error":{"metadata":{"limit_source":"openrouter_in_flight_budget"}}}"#,
            ),
            StubResponse::ok(OK_BODY),
        ]);
        let repository = repository(server.url());

        let decision = repository.evaluate(&request()).unwrap();

        assert_eq!(decision.answers["r0"], 0.1);
        assert_eq!(server.finish().len(), 2);
    }

    #[test]
    fn it_does_not_retry_an_ordinary_402() {
        let server = StubServer::new(vec![StubResponse::status(
            402,
            r#"{"error":"payment required"}"#,
        )]);
        let repository = repository(server.url());

        let error = repository.evaluate(&request()).unwrap_err();

        assert!(matches!(error, Error::Status { status: 402, .. }));
        assert_eq!(server.finish().len(), 1);
    }

    #[test]
    fn it_returns_the_last_status_after_exhausting_retries() {
        let server = StubServer::new(vec![
            StubResponse::status(500, "one"),
            StubResponse::status(500, "two"),
            StubResponse::status(500, "three"),
            StubResponse::status(500, "four"),
        ]);
        let repository = repository(server.url());

        let error = repository.evaluate(&request()).unwrap_err();

        match error {
            Error::Status { status, body } => {
                assert_eq!(status, 500);
                assert_eq!(body, "four");
            }
            other => panic!("unexpected error: {other}"),
        }
        assert_eq!(server.finish().len(), 4);
    }

    #[test]
    fn it_honours_a_numeric_retry_after() {
        let server = StubServer::new(vec![
            StubResponse::status(429, r#"{"error":"rate"}"#).with_header("Retry-After", "0"),
            StubResponse::ok(OK_BODY),
        ]);
        let repository = repository(server.url());

        let decision = repository.evaluate(&request()).unwrap();

        assert_eq!(decision.answers["r0"], 0.1);
        assert_eq!(server.finish().len(), 2);
    }

    #[test]
    fn it_parses_a_numeric_retry_after_and_ignores_a_date() {
        assert_eq!(parse_retry_after("0"), Some(Duration::ZERO));
        assert_eq!(parse_retry_after(" 7 "), Some(Duration::from_secs(7)));
        assert_eq!(parse_retry_after("Wed, 21 Oct 2015 07:28:00 GMT"), None);
    }

    #[test]
    fn it_calculates_the_backoff() {
        let delays = RetryDelays {
            initial: Duration::from_secs(1),
            max: Duration::from_secs(30),
            max_jitter: Duration::from_millis(250),
        };

        assert_eq!(retry_delay_for(1, delays), Duration::from_millis(1_037));
        assert_eq!(retry_delay_for(2, delays), Duration::from_millis(2_074));
        assert_eq!(retry_delay_for(3, delays), Duration::from_millis(4_111));
        assert_eq!(retry_delay_for(4, delays), Duration::from_millis(8_148));
        assert_eq!(retry_delay_for(6, delays), Duration::from_millis(30_222));
    }

    #[test]
    fn it_reads_the_key_and_default_url() {
        let repository = DecisionsRepository::from_lookup(Duration::from_secs(5), |key| {
            (key == "OPENROUTER_API_KEY").then(|| "key".to_owned())
        })
        .unwrap();

        assert_eq!(repository.api_key, "key");
        assert_eq!(repository.base_url, DEFAULT_URL);
    }

    #[test]
    fn it_uses_the_url_override() {
        let repository =
            DecisionsRepository::from_lookup(Duration::from_secs(5), |key| match key {
                "OPENROUTER_API_KEY" => Some("key".to_owned()),
                "COMPLY_DECISIONS_URL" => Some("http://example.test/decisions".to_owned()),
                _ => None,
            })
            .unwrap();

        assert_eq!(repository.base_url, "http://example.test/decisions");
    }

    #[test]
    fn it_treats_an_empty_key_as_missing() {
        let error = match DecisionsRepository::from_lookup(Duration::from_secs(5), |key| {
            (key == "OPENROUTER_API_KEY").then(String::new)
        }) {
            Ok(_) => panic!("unexpected repository"),
            Err(error) => error,
        };

        assert!(matches!(error, Error::MissingApiKey));
    }

    #[test]
    fn it_treats_an_empty_url_as_absent() {
        let repository =
            DecisionsRepository::from_lookup(Duration::from_secs(5), |key| match key {
                "OPENROUTER_API_KEY" => Some("key".to_owned()),
                "COMPLY_DECISIONS_URL" => Some(String::new()),
                _ => None,
            })
            .unwrap();

        assert_eq!(repository.base_url, DEFAULT_URL);
    }

    #[test]
    fn it_uses_the_values_given_to_new() {
        let repository =
            DecisionsRepository::new("key", "http://example.test", Duration::from_secs(5));

        assert_eq!(repository.api_key, "key");
        assert_eq!(repository.base_url, "http://example.test");
    }

    #[test]
    fn it_returns_transport_when_the_connection_fails() {
        let repository = DecisionsRepository::new(
            "key",
            "http://127.0.0.1:1/decisions",
            Duration::from_millis(100),
        );

        let error = repository.evaluate(&request()).unwrap_err();

        assert!(matches!(error, Error::Transport(_)));
    }
}
