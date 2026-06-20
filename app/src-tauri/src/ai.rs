//! AI classification via OpenAI chat completions (gpt-4o-mini). One batched call
//! tags many items at once; verbs are normalized through the canonical taxonomy.

use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Debug, Clone)]
pub struct Classification {
    pub object: String,
    pub sub_object: String,
    pub verb: String,
    pub qualifier: String,
}

/// OpenAI API key from the environment (the app's chosen provider).
pub fn api_key() -> Option<String> {
    std::env::var("OPENAI_API_KEY")
        .ok()
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty())
}

/// Build the chat-completions request body for a batch of items.
pub fn build_request_body(items: &[(i64, String, String)], verbs: &[&str]) -> Value {
    let list: Vec<Value> = items
        .iter()
        .map(|(id, name, desc)| json!({ "id": id, "name": name, "description": desc }))
        .collect();
    let system = format!(
        "You classify developer \"skills\" and \"agents\" into a canonical naming scheme so \
         duplicates can be found. For EACH item return: object (the primary noun/domain it acts \
         on, Title Case, e.g. Ax, Twilio, Code, Data), sub_object (a finer noun or empty string, \
         e.g. Form, Enum, Table), verb (EXACTLY ONE of: {verbs}), and qualifier (a role/scope or \
         empty string: Expert, Specialist, Reviewer, CRUD, Deep). Reply ONLY as JSON of the form \
         {{\"items\":[{{\"id\":<id>,\"object\":\"\",\"sub_object\":\"\",\"verb\":\"\",\"qualifier\":\"\"}}]}}.",
        verbs = verbs.join(", ")
    );
    let user = json!({ "items": list }).to_string();
    json!({
        "model": "gpt-4o-mini",
        "temperature": 0,
        "response_format": { "type": "json_object" },
        "messages": [
            { "role": "system", "content": system },
            { "role": "user", "content": user }
        ]
    })
}

#[derive(Deserialize)]
struct RawItem {
    id: i64,
    #[serde(default)]
    object: String,
    #[serde(default)]
    sub_object: String,
    #[serde(default)]
    verb: String,
    #[serde(default)]
    qualifier: String,
}

#[derive(Deserialize)]
struct RawResp {
    items: Vec<RawItem>,
}

/// Parse the model's JSON content into classifications, normalizing the verb
/// through the canonical taxonomy (unknown verbs are kept as-is).
pub fn parse_response(content: &str) -> Result<Vec<(i64, Classification)>, String> {
    let resp: RawResp = serde_json::from_str(content).map_err(|e| format!("classify parse: {e}"))?;
    Ok(resp
        .items
        .into_iter()
        .map(|r| {
            let verb = crate::taxonomy::canonical_verb(&r.verb)
                .map(str::to_string)
                .unwrap_or(r.verb);
            (
                r.id,
                Classification {
                    object: r.object.trim().to_string(),
                    sub_object: r.sub_object.trim().to_string(),
                    verb,
                    qualifier: r.qualifier.trim().to_string(),
                },
            )
        })
        .collect())
}

/// Classify a batch of items with one API call.
pub async fn classify_batch(
    client: &reqwest::Client,
    api_key: &str,
    items: &[(i64, String, String)],
    verbs: &[&str],
) -> Result<Vec<(i64, Classification)>, String> {
    let body = build_request_body(items, verbs);
    let resp = client
        .post("https://api.openai.com/v1/chat/completions")
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        let status = resp.status();
        return Err(format!("OpenAI {status}: {}", resp.text().await.unwrap_or_default()));
    }
    let v: Value = resp.json().await.map_err(|e| e.to_string())?;
    let content = v["choices"][0]["message"]["content"]
        .as_str()
        .ok_or("OpenAI response missing message content")?;
    parse_response(content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_body_has_model_and_json_format() {
        let b = build_request_body(&[(1, "x".into(), "d".into())], &["Create", "Review"]);
        assert_eq!(b["model"], "gpt-4o-mini");
        assert_eq!(b["response_format"]["type"], "json_object");
        assert!(b["messages"][0]["content"].as_str().unwrap().contains("Create, Review"));
    }

    #[test]
    fn parse_normalizes_verbs_and_trims() {
        let content = r#"{"items":[
            {"id":1,"object":"Ax","sub_object":"Form","verb":"generate","qualifier":"Expert"},
            {"id":2,"object":" Code ","sub_object":"","verb":"Review","qualifier":""}
        ]}"#;
        let out = parse_response(content).unwrap();
        assert_eq!(out[0].0, 1);
        assert_eq!(out[0].1.verb, "Create"); // generate -> Create
        assert_eq!(out[0].1.object, "Ax");
        assert_eq!(out[1].1.object, "Code"); // trimmed
        assert_eq!(out[1].1.verb, "Review");
    }

    #[test]
    fn parse_keeps_unknown_verb() {
        let out = parse_response(r#"{"items":[{"id":9,"object":"X","verb":"frobnicate"}]}"#).unwrap();
        assert_eq!(out[0].1.verb, "frobnicate");
    }

    /// Opt-in live check against OpenAI (needs OPENAI_API_KEY).
    /// Run: cargo test classify_live -- --ignored --nocapture
    #[test]
    #[ignore]
    fn classify_live() {
        let key = match api_key() {
            Some(k) => k,
            None => {
                eprintln!("no OPENAI_API_KEY; skipping");
                return;
            }
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        let items = vec![
            (1i64, "ax-form-builder".to_string(), "Generate a new D365 F&O form".to_string()),
            (2i64, "code-reviewer".to_string(), "Review code for quality and bugs".to_string()),
        ];
        let out = rt
            .block_on(classify_batch(&reqwest::Client::new(), &key, &items, crate::taxonomy::CANONICAL_VERBS))
            .unwrap();
        println!("live classify: {out:?}");
        assert_eq!(out.len(), 2);
        for (_, c) in &out {
            assert!(crate::taxonomy::CANONICAL_VERBS.contains(&c.verb.as_str()), "verb {} not canonical", c.verb);
        }
    }
}
