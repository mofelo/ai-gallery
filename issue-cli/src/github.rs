//! Native GitHub API 客户端
//!
//! 使用 reqwest 调用 GitHub REST API，用于 issue-cli 环境。
//! 与 worker_github.rs 功能相同但使用不同的 HTTP 客户端。

use ai_gallery_core::github_api::{OWNER, IMAGE_REPO};
use ai_gallery_core::error::ApiResult;
use serde_json::Value;

/// 从环境变量读取 GitHub Token
fn get_github_token() -> Result<String, String> {
    std::env::var("GITHUB_TOKEN")
        .ok()
        .filter(|t| !t.is_empty())
        .or_else(|| {
            // 也尝试从 .env 文件读取
            dotenv::dotenv().ok();
            std::env::var("GITHUB_TOKEN").ok()
        })
        .ok_or_else(|| {
            "缺少 GITHUB_TOKEN 环境变量。请在 issue-cli 目录创建 .env 文件或设置环境变量。\n\
            示例: echo 'GITHUB_TOKEN=your_github_pat_here' > .env".to_string()
        })
}

const MAX_RETRIES: u32 = 3;
const INITIAL_BACKOFF_MS: u64 = 200;

/// 调用 GitHub API
pub async fn github_api(
    method: &str,
    path: &str,
    body: Option<&Value>,
) -> ApiResult<Value> {
    let token = get_github_token().map_err(|e| {
        ai_gallery_core::error::ApiError::Other(e)
    })?;

    let url = format!("https://api.github.com/repos/{}/{}/{}", OWNER, IMAGE_REPO, path);
    let client = reqwest::Client::new();
    let mut last_error = ai_gallery_core::error::ApiError::Other("max retries exceeded".to_string());

    for attempt in 0..=MAX_RETRIES {
        if attempt > 0 {
            let backoff = INITIAL_BACKOFF_MS * (1 << (attempt - 1));
            tokio::time::sleep(std::time::Duration::from_millis(backoff)).await;
        }

        let mut req = client.request(
            reqwest::Method::from_bytes(method.as_bytes()).unwrap_or(reqwest::Method::GET),
            &url
        )
        .header("Authorization", format!("Bearer {}", token))
        .header("Accept", "application/vnd.github.v3+json")
        .header("User-Agent", "ai-gallery-cli");

        if let Some(b) = body {
            req = req.json(b);
        }

        let resp = match req.send().await {
            Ok(r) => r,
            Err(e) => {
                last_error = ai_gallery_core::error::ApiError::Timeout(e.to_string());
                if attempt < MAX_RETRIES { continue; }
                return Err(last_error);
            }
        };

        let status = resp.status().as_u16();
        let text = resp.text().await.unwrap_or_default();

        if status >= 400 {
            if should_retry(status) && attempt < MAX_RETRIES {
                last_error = ai_gallery_core::error::ApiError::Status(status, text.clone());
                continue;
            }
            return Err(match status {
                404 => ai_gallery_core::error::ApiError::NotFound(text),
                429 => ai_gallery_core::error::ApiError::RateLimit(text),
                401 | 403 => ai_gallery_core::error::ApiError::Unauthorized,
                _ => ai_gallery_core::error::ApiError::Status(status, text),
            });
        }

        return serde_json::from_str(&text)
            .map_err(|e| ai_gallery_core::error::ApiError::Json(e.to_string()));
    }

    Err(last_error)
}

fn should_retry(status: u16) -> bool {
    matches!(status, 429 | 500 | 502 | 503 | 504)
}

/// 创建 Issue
pub async fn create_issue(
    title: &str,
    body: &str,
    labels: &[String],
) -> ApiResult<Value> {
    let payload = serde_json::json!({
        "title": title,
        "body": body,
        "labels": labels,
    });
    github_api("POST", "issues", Some(&payload)).await
}

/// 列出所有 Issue
pub async fn list_issues() -> ApiResult<Vec<Value>> {
    let value = github_api("GET", "issues?state=open&per_page=100", None).await?;
    let issues = value.as_array().cloned().unwrap_or_default();
    // 过滤掉 Pull Request
    Ok(issues.into_iter()
        .filter(|i| i.get("pull_request").is_none())
        .collect())
}

/// 搜索 Issue
pub async fn search_issues(query: &str) -> ApiResult<Vec<Value>> {
    let encoded = urlencoding::encode(query);
    let value = github_api("GET", &format!("issues?state=open&per_page=100&q={}", encoded), None).await?;
    let issues = value.as_array().cloned().unwrap_or_default();
    Ok(issues.into_iter()
        .filter(|i| i.get("pull_request").is_none())
        .collect())
}

/// 生成 Issue body（YAML frontmatter 格式）
pub fn build_issue_body(
    prompt: &str,
    negative: Option<&str>,
    seed: u64,
    model: Option<&str>,
    model_hash: Option<&str>,
    cfg_scale: f64,
    steps: u32,
    sampler: Option<&str>,
    width: u32,
    height: u32,
    loras: Option<&str>,
    source: Option<&str>,
    png_url: &str,
    tags: &[String],
) -> String {
    let mut body = String::from("---\n");

    body.push_str(&format!("prompt: {:?}\n", prompt));
    if let Some(neg) = negative {
        if !neg.is_empty() {
            body.push_str(&format!("negative: {:?}\n", neg));
        }
    }
    body.push_str(&format!("seed: {}\n", seed));
    if let Some(m) = model {
        if !m.is_empty() {
            body.push_str(&format!("model: {:?}\n", m));
        }
    }
    if let Some(mh) = model_hash {
        if !mh.is_empty() {
            body.push_str(&format!("model_hash: {:?}\n", mh));
        }
    }
    if cfg_scale > 0.0 {
        body.push_str(&format!("cfg_scale: {}\n", cfg_scale));
    }
    if steps > 0 {
        body.push_str(&format!("steps: {}\n", steps));
    }
    if let Some(s) = sampler {
        if !s.is_empty() {
            body.push_str(&format!("sampler: {:?}\n", s));
        }
    }
    if width > 0 && height > 0 {
        body.push_str(&format!("width: {}\n", width));
        body.push_str(&format!("height: {}\n", height));
    }
    if let Some(l) = loras {
        if !l.is_empty() {
            body.push_str(&format!("loras: {:?}\n", l));
        }
    }
    if let Some(s) = source {
        if !s.is_empty() {
            body.push_str(&format!("source: {:?}\n", s));
        }
    }
    body.push_str(&format!("png_url: {:?}\n", png_url));
    body.push_str("---\n\n");

    // 添加标签行
    if !tags.is_empty() {
        body.push_str(&format!("Tags: {}\n", tags.join(", ")));
    }

    body
}