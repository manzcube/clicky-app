//! Everything that knows what a language model is lives here.
//!
//! To run clicky against something other than Ollama — a cloud API, llama.cpp,
//! LM Studio — rewrite `stream` and leave the rest of the app alone. The
//! frontend only knows about the `ask_model` command and three events:
//! `token`, `done`, `error`.

use futures_util::StreamExt;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};

pub async fn stream(app: AppHandle, host: String, model: String, system: String, user: String) {
    let body = json!({
        "model": model,
        "stream": true,
        // Keeps the model resident between summons. Without this, the first
        // request after a few minutes pays the whole load time again and the
        // app feels broken rather than slow.
        "keep_alive": "30m",
        "options": { "temperature": 0.3, "num_predict": 800 },
        "messages": [
            { "role": "system", "content": system },
            { "role": "user",   "content": user }
        ]
    });

    let url = format!("{host}/api/chat");
    let resp = match reqwest::Client::new().post(&url).json(&body).send().await {
        Ok(r) => r,
        Err(e) => {
            let _ = app.emit("error", friendly(&e.to_string(), &model));
            return;
        }
    };

    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        let _ = app.emit(
            "error",
            format!("No model called {model}. Pull it with: ollama pull {model}"),
        );
        return;
    }
    if !resp.status().is_success() {
        let _ = app.emit("error", format!("Ollama answered {}", resp.status()));
        return;
    }

    // Ollama streams newline-delimited JSON, and chunks split mid-line.
    let mut stream = resp.bytes_stream();
    let mut buf = String::new();

    while let Some(chunk) = stream.next().await {
        let Ok(bytes) = chunk else { break };
        buf.push_str(&String::from_utf8_lossy(&bytes));

        while let Some(nl) = buf.find('\n') {
            let line: String = buf.drain(..=nl).collect();
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let Ok(v) = serde_json::from_str::<Value>(line) else {
                continue;
            };

            if let Some(err) = v["error"].as_str() {
                let _ = app.emit("error", friendly(err, &model));
                return;
            }
            if let Some(t) = v["message"]["content"].as_str() {
                if !t.is_empty() {
                    let _ = app.emit("token", t);
                }
            }
            if v["done"].as_bool().unwrap_or(false) {
                let _ = app.emit("done", ());
                return;
            }
        }
    }
    let _ = app.emit("done", ());
}

/// Turn transport noise into something a person can act on.
fn friendly(raw: &str, model: &str) -> String {
    let low = raw.to_lowercase();
    if low.contains("connect") || low.contains("refused") || low.contains("dns") {
        "Ollama isn't running. Start it, then try again.".into()
    } else if low.contains("not found") || low.contains("no such model") {
        format!("No model called {model}. Pull it with: ollama pull {model}")
    } else {
        raw.to_string()
    }
}

/// Models you've actually pulled, for the tray menu.
pub async fn list(host: &str) -> Vec<String> {
    let url = format!("{host}/api/tags");
    let Ok(resp) = reqwest::get(&url).await else {
        return vec![];
    };
    let Ok(v) = resp.json::<Value>().await else {
        return vec![];
    };
    v["models"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|m| m["name"].as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

/// Is Ollama even running? Used to drive the first-run setup card — a user
/// with nothing installed yet gets a "download Ollama" prompt instead of a
/// silent, confusing failure the first time they highlight text.
pub async fn ping(host: &str) -> bool {
    reqwest::Client::new()
        .get(format!("{host}/api/tags"))
        .timeout(std::time::Duration::from_secs(2))
        .send()
        .await
        .is_ok()
}

pub async fn has_model(host: &str, model: &str) -> bool {
    list(host).await.iter().any(|m| m == model)
}

/// Pulls a model, streaming progress the same way `stream()` streams tokens.
pub async fn pull(app: AppHandle, host: String, model: String) {
    let url = format!("{host}/api/pull");
    let body = json!({ "model": model, "stream": true });

    let resp = match reqwest::Client::new().post(&url).json(&body).send().await {
        Ok(r) => r,
        Err(e) => {
            let _ = app.emit("pull-error", e.to_string());
            return;
        }
    };
    if !resp.status().is_success() {
        let _ = app.emit("pull-error", format!("Ollama answered {}", resp.status()));
        return;
    }

    let mut stream = resp.bytes_stream();
    let mut buf = String::new();

    while let Some(chunk) = stream.next().await {
        let Ok(bytes) = chunk else { break };
        buf.push_str(&String::from_utf8_lossy(&bytes));

        while let Some(nl) = buf.find('\n') {
            let line: String = buf.drain(..=nl).collect();
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let Ok(v) = serde_json::from_str::<Value>(line) else {
                continue;
            };

            if let Some(err) = v["error"].as_str() {
                let _ = app.emit("pull-error", err.to_string());
                return;
            }
            let _ = app.emit(
                "pull-progress",
                json!({
                    "status": v["status"].as_str().unwrap_or(""),
                    "completed": v["completed"].as_u64(),
                    "total": v["total"].as_u64(),
                }),
            );
            if v["status"].as_str() == Some("success") {
                let _ = app.emit("pull-done", ());
                return;
            }
        }
    }
    let _ = app.emit("pull-done", ());
}
