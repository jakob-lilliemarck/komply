use std::collections::HashMap;

use serde::Deserialize;

use super::errors::Error;

/// The validated result of one decisions request.
#[derive(Debug, Clone, PartialEq)]
pub struct Decision {
    pub answers: HashMap<String, f64>,
    pub cost_usd: Option<f64>,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct WireResponse {
    #[serde(default)]
    pub(crate) answers: HashMap<String, WireAnswer>,
    #[serde(default)]
    pub(crate) usage: Option<WireUsage>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct WireAnswer {
    #[serde(rename = "type", default)]
    pub(crate) kind: Option<String>,
    #[serde(default)]
    pub(crate) noul: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct WireUsage {
    #[serde(default)]
    pub(crate) cost: Option<f64>,
    #[serde(default)]
    pub(crate) input_tokens: Option<u64>,
    #[serde(default)]
    pub(crate) output_tokens: Option<u64>,
}

/// Validates the wire response against the requested keys.
pub(crate) fn decision_from_response(
    response: WireResponse,
    keys: &[String],
) -> Result<Decision, Error> {
    let mut answers = HashMap::new();

    for key in keys {
        let answer = response
            .answers
            .get(key)
            .ok_or_else(|| Error::MissingAnswer(key.clone()))?;
        if answer.kind.as_deref() != Some("noul") {
            return Err(Error::MissingAnswer(key.clone()));
        }
        let value = answer
            .noul
            .ok_or_else(|| Error::MissingAnswer(key.clone()))?;
        if !value.is_finite() || !(0.0..=1.0).contains(&value) {
            return Err(Error::InvalidProbability {
                key: key.clone(),
                value,
            });
        }
        answers.insert(key.clone(), value);
    }

    let (cost_usd, input_tokens, output_tokens) = match response.usage {
        Some(usage) => (usage.cost, usage.input_tokens, usage.output_tokens),
        None => (None, None, None),
    };

    Ok(Decision {
        answers,
        cost_usd,
        input_tokens,
        output_tokens,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answer(kind: Option<&str>, noul: Option<f64>) -> WireAnswer {
        WireAnswer {
            kind: kind.map(str::to_owned),
            noul,
        }
    }

    fn response(answers: Vec<(&str, WireAnswer)>, usage: Option<WireUsage>) -> WireResponse {
        WireResponse {
            answers: answers
                .into_iter()
                .map(|(key, answer)| (key.to_owned(), answer))
                .collect(),
            usage,
        }
    }

    #[test]
    fn it_maps_answers_and_usage() {
        let response = response(
            vec![
                ("r0", answer(Some("noul"), Some(0.1))),
                ("r1", answer(Some("noul"), Some(0.9))),
            ],
            Some(WireUsage {
                cost: Some(0.25),
                input_tokens: Some(100),
                output_tokens: Some(20),
            }),
        );

        let decision =
            decision_from_response(response, &["r0".to_owned(), "r1".to_owned()]).unwrap();

        assert_eq!(decision.answers["r0"], 0.1);
        assert_eq!(decision.answers["r1"], 0.9);
        assert_eq!(decision.cost_usd, Some(0.25));
        assert_eq!(decision.input_tokens, Some(100));
        assert_eq!(decision.output_tokens, Some(20));
    }

    #[test]
    fn it_allows_a_missing_usage() {
        let response = response(vec![("r0", answer(Some("noul"), Some(0.1)))], None);

        let decision = decision_from_response(response, &["r0".to_owned()]).unwrap();

        assert_eq!(decision.cost_usd, None);
        assert_eq!(decision.input_tokens, None);
        assert_eq!(decision.output_tokens, None);
    }

    #[test]
    fn it_ignores_extra_answers() {
        let response = response(
            vec![
                ("r0", answer(Some("noul"), Some(0.1))),
                ("r1", answer(Some("noul"), Some(0.2))),
            ],
            None,
        );

        let decision = decision_from_response(response, &["r0".to_owned()]).unwrap();

        assert_eq!(decision.answers.len(), 1);
        assert_eq!(decision.answers["r0"], 0.1);
    }

    #[test]
    fn it_rejects_a_missing_answer() {
        let response = response(Vec::new(), None);

        let error = decision_from_response(response, &["r0".to_owned()]).unwrap_err();

        assert!(matches!(error, Error::MissingAnswer(key) if key == "r0"));
    }

    #[test]
    fn it_rejects_a_missing_type() {
        let response = response(vec![("r0", answer(None, Some(0.1)))], None);

        let error = decision_from_response(response, &["r0".to_owned()]).unwrap_err();

        assert!(matches!(error, Error::MissingAnswer(key) if key == "r0"));
    }

    #[test]
    fn it_rejects_a_wrong_type() {
        let response = response(vec![("r0", answer(Some("bool"), Some(0.1)))], None);

        let error = decision_from_response(response, &["r0".to_owned()]).unwrap_err();

        assert!(matches!(error, Error::MissingAnswer(key) if key == "r0"));
    }

    #[test]
    fn it_rejects_a_probability_outside_the_unit_interval() {
        let response = response(vec![("r0", answer(Some("noul"), Some(1.5)))], None);

        let error = decision_from_response(response, &["r0".to_owned()]).unwrap_err();

        assert!(matches!(
            error,
            Error::InvalidProbability { key, value }
                if key == "r0" && value == 1.5
        ));
    }

    #[test]
    fn it_accepts_the_boundaries() {
        let response = response(
            vec![
                ("r0", answer(Some("noul"), Some(0.0))),
                ("r1", answer(Some("noul"), Some(1.0))),
            ],
            None,
        );

        let decision =
            decision_from_response(response, &["r0".to_owned(), "r1".to_owned()]).unwrap();

        assert_eq!(decision.answers["r0"], 0.0);
        assert_eq!(decision.answers["r1"], 1.0);
    }
}
