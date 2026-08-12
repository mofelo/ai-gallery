//! 图片列表 & 搜索处理器
//!
//! 端点:
//!   GET /api/images — 列出所有图片（支持分页: ?page=0&per_page=50）
//!   GET /api/images/:number — 单张图片详情
//!   GET /api/search?q=xxx — 搜索（prompt/模型/标签）
//!   GET /api/search?model=xxx&tag=yyy — 按条件筛选
//!
//! 数据源: GitHub Issues (ai-images 仓库)

use crate::github::{get_token, github_api};
use ai_gallery_core::error::ApiError;
use ai_gallery_core::response;
use ai_gallery_core::types::ImageRecord;
use serde_json::Value;
use worker::*;

/// 从 GitHub Issue 原始数据中提取 AI 图片字段
/// 支持 frontmatter 格式和旧格式
fn extract_image_fields(issue: &Value) -> Option<ImageRecord> {
    ImageRecord::from_issue(issue)
}

/// GET /api/images — 列出所有 AI 图片
pub async fn handle_images(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let token = get_token(&ctx).await;
    if token.is_empty() {
        return Response::from_json(&response::err(&ApiError::Unauthorized));
    }

    // 分页参数
    let url = req.url()?;
    let query = url.query_pairs().collect::<Vec<_>>();
    let page: usize = query
        .iter()
        .find(|(k, _)| k == "page")
        .and_then(|(_, v)| v.parse().ok())
        .unwrap_or(0);
    let per_page: usize = query
        .iter()
        .find(|(k, _)| k == "per_page")
        .and_then(|(_, v)| v.parse().ok())
        .unwrap_or(50);

    // 拉取 Issues
    let value = match github_api(
        &token,
        "ai-images",
        "GET",
        "issues?state=open&per_page=100",
        None,
    )
    .await
    {
        Ok(v) => v,
        Err(e) => return Response::from_json(&response::err(&e)),
    };

    let issues = value
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|i| i.get("pull_request").is_none())
        .collect::<Vec<_>>();

    // 解析为 ImageRecord
    let mut records: Vec<ImageRecord> = issues
        .iter()
        .filter_map(extract_image_fields)
        .collect();

    // 按创建时间降序排列
    records.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    // 分页
    let total = records.len();
    let start = page * per_page;
    let paged: Vec<&ImageRecord> = records
        .iter()
        .skip(start)
        .take(per_page)
        .collect();

    // 转换为 JSON 响应
    let items: Vec<Value> = paged
        .iter()
        .map(|r| {
            serde_json::json!({
                "number": r.number,
                "title": r.title,
                "prompt": r.prompt,
                "seed": r.seed,
                "model": r.model,
                "png_url": r.png_url,
                "tags": r.tags,
                "created_at": r.created_at,
                "cfg_scale": r.cfg_scale,
                "steps": r.steps,
                "sampler": r.sampler,
                "width": r.width,
                "height": r.height,
                "source": r.source,
            })
        })
        .collect();

    Response::from_json(&response::ok(serde_json::json!({
        "items": items,
        "total": total,
        "page": page,
        "per_page": per_page,
    })))
}

/// GET /api/images/:number — 单张图片详情
pub async fn handle_image_detail(ctx: RouteContext<()>, number: u64) -> Result<Response> {
    let token = get_token(&ctx).await;
    if token.is_empty() {
        return Response::from_json(&response::err(&ApiError::Unauthorized));
    }

    let value = match github_api(&token, "ai-images", "GET", &format!("issues/{}", number), None).await {
        Ok(v) => v,
        Err(e) => return Response::from_json(&response::err(&e)),
    };

    match extract_image_fields(&value) {
        Some(record) => Response::from_json(&response::ok(serde_json::json!({
            "number": record.number,
            "title": record.title,
            "prompt": record.prompt,
            "negative": record.negative,
            "seed": record.seed,
            "model": record.model,
            "model_hash": record.model_hash,
            "png_url": record.png_url,
            "tags": record.tags,
            "created_at": record.created_at,
            "cfg_scale": record.cfg_scale,
            "steps": record.steps,
            "sampler": record.sampler,
            "width": record.width,
            "height": record.height,
            "loras": record.loras,
            "source": record.source,
        }))),
        None => Response::from_json(&response::err(&ApiError::NotFound(
            format!("image #{}", number),
        ))),
    }
}

/// GET /api/search — 搜索图片
pub async fn handle_search(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let token = get_token(&ctx).await;
    if token.is_empty() {
        return Response::from_json(&response::err(&ApiError::Unauthorized));
    }

    // 解析搜索参数
    let url = req.url()?;
    let query = url.query_pairs().collect::<Vec<_>>();
    let q = query
        .iter()
        .find(|(k, _)| k == "q")
        .map(|(_, v)| v.to_lowercase());
    let model_filter = query
        .iter()
        .find(|(k, _)| k == "model")
        .map(|(_, v)| v.to_lowercase());
    let tag_filter = query
        .iter()
        .find(|(k, _)| k == "tag")
        .map(|(_, v)| v.to_lowercase());
    let seed_filter = query
        .iter()
        .find(|(k, _)| k == "seed")
        .and_then(|(_, v)| v.parse::<u64>().ok());

    // 拉取所有 Issues
    let value = match github_api(
        &token,
        "ai-images",
        "GET",
        "issues?state=open&per_page=100",
        None,
    )
    .await
    {
        Ok(v) => v,
        Err(e) => return Response::from_json(&response::err(&e)),
    };

    let issues = value
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|i| i.get("pull_request").is_none())
        .collect::<Vec<_>>();

    let records: Vec<ImageRecord> = issues.iter().filter_map(extract_image_fields).collect();

    // 筛选
    let mut filtered: Vec<&ImageRecord> = records
        .iter()
        .filter(|r| {
            // 全文搜索（prompt/标题/标签）
            if let Some(ref search_q) = q {
                if !r.prompt.to_lowercase().contains(search_q)
                    && !r.title.to_lowercase().contains(search_q)
                    && !r.tags.iter().any(|t| t.to_lowercase().contains(search_q))
                    && !r.model.as_ref().map(|m| m.to_lowercase().contains(search_q)).unwrap_or(false)
                {
                    return false;
                }
            }
            // 按模型筛选
            if let Some(ref m) = model_filter {
                if !r.model.as_ref().map(|v| v.to_lowercase().contains(m)).unwrap_or(false) {
                    return false;
                }
            }
            // 按标签筛选
            if let Some(ref t) = tag_filter {
                if !r.tags.iter().any(|tag| tag.to_lowercase().contains(t)) {
                    return false;
                }
            }
            // 按种子筛选
            if let Some(s) = seed_filter {
                if r.seed != s {
                    return false;
                }
            }
            true
        })
        .collect();

    // 按创建时间降序排列
    filtered.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    let items: Vec<Value> = filtered
        .iter()
        .map(|r| {
            serde_json::json!({
                "number": r.number,
                "title": r.title,
                "prompt": r.prompt,
                "seed": r.seed,
                "model": r.model,
                "png_url": r.png_url,
                "tags": r.tags,
                "created_at": r.created_at,
            })
        })
        .collect();

    Response::from_json(&response::ok(serde_json::json!({
        "items": items,
        "total": items.len(),
    })))
}