use serde::Serialize;
use serde_json::Value;

use crate::MODEL_NAME;

/// D1 tokenizes the JSON inside `instructions`, so field order is part of the
/// prompt, even though ordinary JSON consumers treat object order as immaterial.
/// Keep these typed values all the way to reqwest: a serde_json::Value roundtrip
/// sorts the keys and changes decisions (verified against the actual Q4 model).
#[derive(Serialize)]
pub(crate) struct DecisionRequest<'a> {
    model: &'static str,
    keep_alive: u16,
    state: &'a Value,
    questions: Questions<'a>,
}

#[derive(Serialize)]
struct Questions<'a> {
    relevant: Question<'a>,
}

#[derive(Serialize)]
struct Question<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    instructions: Instructions<'a>,
}

#[derive(Serialize)]
struct Instructions<'a> {
    task: &'static str,
    candidate_window_title: &'a str,
}

pub(crate) fn relevance_request<'a>(state: &'a Value, title: &'a str) -> DecisionRequest<'a> {
    DecisionRequest {
        model: MODEL_NAME,
        keep_alive: 300,
        state,
        questions: Questions {
            relevant: Question {
                kind: "noul",
                instructions: Instructions {
                    task: "Does this candidate relate to at least one selected window through a shared project, topic, or task? Shared application names alone do not count.",
                    candidate_window_title: title,
                },
            },
        },
    }
}
