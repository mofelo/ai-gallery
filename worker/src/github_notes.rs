//! GitHub Issues API 客户端 — 私有笔记存储
//!
//! 每条笔记 = 一条 GitHub Issue，标题带 `[#N]` 前缀关联图片。
//! 仓库: mofelo/ai-notes（可通过 GITHUB_NOTES_REPO 环境变量覆盖）

use ai_gallery_core::types::NoteRecord;
use serde_json::Value;
use worker::*;

/// 获取 GitHub Token
pub fn get_token(env: &Env) -> Result<String> {
    env.secret("GITHUB_TOKEN")
        .map(|s| s.to_string())
        .map_err(|e| worker::Error::RustError(format!("GITHUB_TOKEN 未配置: {}", e)))
}

/// 获取笔记仓库名
pub fn get_repo(env: &Env) -> String {
    env.var("GITHUB_NOTES_REPO")
        .map(|s| s.to_string())
        .unwrap_or_else(|_| "mofelo/ai-notes".to_string())
}

/// 构造 GitHub API 基础 URL
fn api_base(env: &Env) -> String {
    format!("https://api.github.com/repos/{}", get_repo(env))
}

/// 构造通用请求头
fn build_headers(token: &str) -> Result<Headers> {
    let headers = Headers::new();
    headers.set("Authorization", &format!("Bearer {}", token))?;
    headers.set("Accept", "application/vnd.github+json")?;
    headers.set("User-Agent", "ai-gallery")?;
    headers.set("Content-Type", "application/json")?;
    Ok(headers)
}

/// 发送 GitHub API 请求并返回 JSON 响应体
async fn github_request(env: &Env, method: Method, path: &str, body: Option<Vec<u8>>) -> Result<String> {
    let token = get_token(env)?;
    let headers = build_headers(&token)?;
    let url = format!("{}{}", api_base(env), path);

    let mut init = RequestInit::new();
    init.method = method;
    init.headers = headers;
    if let Some(b) = body {
        init.body = Some(b.into());
    }

    let req = Request::new_with_init(&url, &init)?;
    let mut resp = Fetch::Request(req).send().await?;
    let status = resp.status_code();
    let text = resp.text().await?;

    if status < 200 || status >= 300 {
        return Err(worker::Error::RustError(format!(
            "GitHub API 返回 {}: {}",
            status, text
        )));
    }

    Ok(text)
}

/// 从 Issue 标题解析图片编号 (#N)
fn extract_number(title: &str) -> Option<u64> {
    if title.starts_with("[#") {
        if let Some(end) = title.find(']') {
            let num_str = &title[2..end];
            return num_str.parse::<u64>().ok();
        }
    }
    None
}

/// 从 Issue 标题取前 40 字作为 Issue 标题（不含 [#N] 前缀）
fn make_issue_title(image_number: u64, content: &str) -> String {
    let first_line = content.lines().next().unwrap_or(content);
    let truncated = if first_line.len() > 40 {
        &first_line[..40]
    } else {
        first_line
    };
    format!("[#{}] {}", image_number, truncated)
}

/// 列出某张图片的所有笔记
///
/// 从 GitHub Issues 获取所有带有 `note` 标签的 issue，
/// 过滤 title 以 `[#N]` 开头的条目，按 created_at 降序返回。
pub async fn list_notes(env: &Env, image_number: u64) -> Result<Vec<NoteRecord>> {
    let text = github_request(
        env,
        Method::Get,
        "/issues?state=open&labels=note&per_page=100",
        None,
    )
    .await?;

    let items: Vec<Value> = serde_json::from_str(&text)
        .map_err(|e| worker::Error::RustError(format!("解析 GitHub 响应失败: {}", e)))?;

    let mut notes: Vec<NoteRecord> = items
        .into_iter()
        .filter_map(|item| {
            let title = item.get("title")?.as_str()?;
            let n = extract_number(title)?;
            if n != image_number {
                return None;
            }
            let id = item.get("number")?.as_u64()?;
            let content = item.get("body")?.as_str().unwrap_or("").to_string();
            let created_at = item.get("created_at")?.as_str()?.to_string();
            let updated_at = item.get("updated_at")?.as_str().unwrap_or("").to_string();
            Some(NoteRecord {
                id,
                number: n,
                content,
                created_at,
                updated_at,
            })
        })
        .collect();

    // 按 created_at 降序
    notes.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    Ok(notes)
}

/// 创建笔记
///
/// 创建一条 GitHub Issue，标题 `[#N] <前40字>`，body 为完整内容。
/// 返回创建的 Issue 编号。
pub async fn create_note(env: &Env, image_number: u64, content: &str) -> Result<u64> {
    let title = make_issue_title(image_number, content);
    let body = serde_json::json!({
        "title": title,
        "body": content,
        "labels": ["note"],
    });

    let body_bytes = serde_json::to_vec(&body)
        .map_err(|e| worker::Error::RustError(format!("序列化失败: {}", e)))?;

    let text = github_request(env, Method::Post, "/issues", Some(body_bytes)).await?;

    let resp: Value = serde_json::from_str(&text)
        .map_err(|e| worker::Error::RustError(format!("解析 GitHub 响应失败: {}", e)))?;

    let id = resp
        .get("number")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| worker::Error::RustError("无法获取 Issue 编号".to_string()))?;

    Ok(id)
}

/// 删除笔记（关闭 Issue）
///
/// GitHub API 不支持真删除，改为关闭 Issue。
pub async fn delete_note(env: &Env, note_id: u64) -> Result<()> {
    let body = serde_json::json!({"state": "closed"});
    let body_bytes = serde_json::to_vec(&body)
        .map_err(|e| worker::Error::RustError(format!("序列化失败: {}", e)))?;

    let path = format!("/issues/{}", note_id);
    github_request(env, Method::Patch, &path, Some(body_bytes)).await?;

    Ok(())
}