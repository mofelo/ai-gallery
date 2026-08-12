//! Worker API 客户端
//!
//! 通过 HTTP 调用 Cloudflare Worker API 读写图片记录（数据存储在 D1 中）。
//!
//! 端点:
//!   POST `{api_base}/api/images` — 创建图片记录
//!   GET  `{api_base}/api/images?page=0&per_page=50` — 分页列出
//!   GET  `{api_base}/api/search?q=xxx` — 搜索

use serde_json::Value;

/// 发送请求并解析 Worker 统一响应包裹 `{ success, data, error }`。
async fn send(
    method: reqwest::Method,
    url: &str,
    body: Option<&Value>,
) -> Result<Value, String> {
    let client = reqwest::Client::new();
    let mut req = client.request(method, url);

    if let Some(b) = body {
        req = req.json(b);
    }

    let resp = req.send().await.map_err(|e| format!("请求失败: {}", e))?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| format!("读取响应失败: {}", e))?;

    if !status.is_success() {
        return Err(format!("API 错误 (HTTP {}): {}", status.as_u16(), text));
    }

    let json: Value = serde_json::from_str(&text)
        .map_err(|e| format!("响应解析失败: {} (body: {})", e, text))?;

    // 提取统一响应中的 data 字段
    json.get("data")
        .cloned()
        .ok_or_else(|| format!("响应缺少 data 字段: {}", text))
}

/// 创建图片记录
///
/// POST `{api_base}/api/images`，请求体匹配 `CreateImageRequest`。
/// 返回包含 `number` 和 `created_at` 的 JSON。
pub async fn create_record(api_base: &str, body: Value) -> Result<Value, String> {
    let url = format!("{}/api/images", api_base.trim_end_matches('/'));
    send(reqwest::Method::POST, &url, Some(&body)).await
}

/// 列出图片记录
///
/// GET `{api_base}/api/images?page={page}&per_page={per_page}`
/// 返回 `{ items, total, page, per_page }`。
pub async fn list_records(api_base: &str, page: usize, per_page: usize) -> Result<Value, String> {
    let url = format!(
        "{}/api/images?page={}&per_page={}",
        api_base.trim_end_matches('/'),
        page,
        per_page
    );
    send(reqwest::Method::GET, &url, None).await
}

/// 搜索图片记录
///
/// GET `{api_base}/api/search?q={query}`
/// 返回 `{ items, total }`。
pub async fn search_records(api_base: &str, query: &str) -> Result<Value, String> {
    let encoded = urlencoding::encode(query);
    let url = format!(
        "{}/api/search?q={}",
        api_base.trim_end_matches('/'),
        encoded
    );
    send(reqwest::Method::GET, &url, None).await
}