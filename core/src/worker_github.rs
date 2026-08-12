//! GitHub API 客户端（workers-rs 实现）
//!
//! 仅当 feature "worker" 启用时编译。用于 Cloudflare Worker 运行时。

use crate::error::{ApiError, ApiResult};
use crate::github_api::{GitHubApi, OWNER};
use serde_json::Value;
use std::time::Duration;
use worker::wasm_bindgen::prelude::JsValue;
use worker::*;

const MAX_RETRIES: u32 = 3;
const INITIAL_BACKOFF_MS: u64 = 200;

/// 从环境变量获取 GitHub Token
pub async fn get_token(ctx: &RouteContext<()>) -> String {
    ctx.var("GITHUB_TOKEN")
        .ok()
        .map(|v| v.to_string())
        .unwrap_or_default()
}

/// Worker GitHub API 客户端
pub struct WorkerGitHubClient {
    token: String,
}

impl WorkerGitHubClient {
    pub async fn from_ctx(ctx: &RouteContext<()>) -> Option<Self> {
        let token = get_token(ctx).await;
        Some(Self { token })
    }

    pub fn token(&self) -> &str {
        &self.token
    }
}

impl GitHubApi for WorkerGitHubClient {
    async fn fetch_issues(&self, repo: &str) -> ApiResult<Vec<Value>> {
        let value = github_api(&self.token, repo, "GET", "issues?state=open&per_page=100", None).await?;
        let issues = value.as_array().cloned().unwrap_or_default();
        Ok(issues.into_iter().filter(|i| i.get("pull_request").is_none()).collect())
    }

    async fn fetch_issue(&self, repo: &str, number: u64) -> ApiResult<Value> {
        github_api(&self.token, repo, "GET", &format!("issues/{}", number), None).await
    }

    async fn create_issue(&self, repo: &str, title: &str, body: &str, labels: &[String]) -> ApiResult<Value> {
        let payload = serde_json::json!({ "title": title, "body": body, "labels": labels });
        github_api(&self.token, repo, "POST", "issues", Some(&payload)).await
    }

    async fn update_issue(&self, repo: &str, number: u64, title: Option<&str>, body: Option<&str>, state: Option<&str>) -> ApiResult<Value> {
        let mut payload = serde_json::json!({});
        if let Some(t) = title { payload["title"] = serde_json::json!(t); }
        if let Some(b) = body { payload["body"] = serde_json::json!(b); }
        if let Some(s) = state { payload["state"] = serde_json::json!(s); }
        github_api(&self.token, repo, "PATCH", &format!("issues/{}", number), Some(&payload)).await
    }
}

fn should_retry(status: u16) -> bool {
    matches!(status, 429 | 500 | 502 | 503 | 504)
}

/// 使用 workers-rs Fetch API 调用 GitHub REST API
pub async fn github_api(
    token: &str,
    repo: &str,
    method: &str,
    path: &str,
    body: Option<&Value>,
) -> ApiResult<Value> {
    let url = format!("https://api.github.com/repos/{}/{}/{}", OWNER, repo, path);
    let mut last_error = ApiError::Other("max retries exceeded".to_string());

    for attempt in 0..=MAX_RETRIES {
        if attempt > 0 {
            let backoff = INITIAL_BACKOFF_MS * (1 << (attempt - 1));
            console_log!("github_api retry {}/{} for {} (backoff: {}ms)", attempt, MAX_RETRIES, path, backoff);
            Delay::from(Duration::from_millis(backoff)).await;
        }

        let mut init = RequestInit::new();
        init.method = match method {
            "POST" => Method::Post,
            "PATCH" => Method::Patch,
            "PUT" => Method::Put,
            _ => Method::Get,
        };

        let headers = Headers::new();
        if !token.is_empty() {
            headers.set("Authorization", &format!("Bearer {}", token)).map_err(|e| ApiError::Other(e.to_string()))?;
        }
        headers.set("Accept", "application/vnd.github.v3+json").map_err(|e| ApiError::Other(e.to_string()))?;
        headers.set("User-Agent", "ai-gallery-worker").map_err(|e| ApiError::Other(e.to_string()))?;
        if body.is_some() {
            headers.set("Content-Type", "application/json").map_err(|e| ApiError::Other(e.to_string()))?;
        }
        init.headers = headers;

        if let Some(b) = body {
            let body_str = serde_json::to_string(b).map_err(|e| ApiError::Json(e.to_string()))?;
            init.body = Some(JsValue::from_str(&body_str));
        }

        let req = Request::new_with_init(&url, &init).map_err(|e| ApiError::Other(e.to_string()))?;
        let mut resp = match Fetch::Request(req).send().await {
            Ok(r) => r,
            Err(e) => {
                if attempt < MAX_RETRIES { last_error = ApiError::Timeout(e.to_string()); continue; }
                return Err(ApiError::Timeout(e.to_string()));
            }
        };

        let status = resp.status_code();
        let text = resp.text().await.map_err(|e| ApiError::Other(e.to_string()))?;

        if status >= 400 {
            if should_retry(status) && attempt < MAX_RETRIES {
                last_error = ApiError::Status(status, text.clone());
                continue;
            }
            return Err(match status {
                404 => ApiError::NotFound(text),
                429 => ApiError::RateLimit(text),
                401 | 403 => ApiError::Unauthorized,
                _ => ApiError::Status(status, text),
            });
        }

        return serde_json::from_str(&text).map_err(|e| ApiError::Json(e.to_string()));
    }

    Err(last_error)
}