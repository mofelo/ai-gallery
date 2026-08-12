//! 图片列表 & 搜索处理器
//!
//! 端点:
//!   GET  /api/images — 列出所有图片（支持分页: ?page=0&per_page=50）
//!   GET  /api/images/:number — 单张图片详情
//!   POST /api/images — 创建新图片记录
//!   GET  /api/search?q=xxx&model=yyy&tag=zzz&seed=123 — 搜索
//!
//! 数据源: D1 (ai_images 表)

use crate::db;
use ai_gallery_core::error::ApiError;
use ai_gallery_core::response;
use ai_gallery_core::types::ImageRecord;
use chrono::Utc;
use serde::Deserialize;
use serde_json::Value;
use worker::*;

/// 创建图片请求体（不含 number/created_at/updated_at）
#[derive(Debug, Deserialize)]
pub struct CreateImageRequest {
    pub png_url: String,
    pub prompt: String,
    pub negative: Option<String>,
    pub seed: Option<u64>,
    pub model: Option<String>,
    pub model_hash: Option<String>,
    pub cfg_scale: Option<f64>,
    pub steps: Option<u32>,
    pub sampler: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub loras: Option<String>,
    pub source: Option<String>,
    pub tags: Option<Vec<String>>,
    pub title: Option<String>,
}

/// GET /api/images — 列出所有 AI 图片
pub async fn handle_images(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let db = match db::get_db(&ctx.env) {
        Ok(d) => d,
        Err(e) => return Response::from_json(&response::err(&ApiError::Other(e.to_string()))),
    };

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

    let records = match db::fetch_all(&db).await {
        Ok(r) => r,
        Err(e) => return Response::from_json(&response::err(&ApiError::Other(e.to_string()))),
    };

    let total = records.len();
    let start = page * per_page;
    let paged: Vec<&ImageRecord> = records.iter().skip(start).take(per_page).collect();

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
    let db = match db::get_db(&ctx.env) {
        Ok(d) => d,
        Err(e) => return Response::from_json(&response::err(&ApiError::Other(e.to_string()))),
    };

    match db::fetch_one(&db, number).await {
        Ok(Some(record)) => Response::from_json(&response::ok(serde_json::json!({
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
        Ok(None) => Response::from_json(&response::err(&ApiError::NotFound(
            format!("image #{}", number),
        ))),
        Err(e) => Response::from_json(&response::err(&ApiError::Other(e.to_string()))),
    }
}

/// POST /api/images — 创建新图片记录
pub async fn handle_create_image(req: &mut Request, ctx: RouteContext<()>) -> Result<Response> {
    let db = match db::get_db(&ctx.env) {
        Ok(d) => d,
        Err(e) => return Response::from_json(&response::err(&ApiError::Other(e.to_string()))),
    };

    let body: CreateImageRequest = match req.json().await {
        Ok(b) => b,
        Err(_) => {
            return Response::from_json(&response::err(&ApiError::Other(
                "Invalid JSON body".to_string(),
            )))
        }
    };

    let created_at = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    let rec = ImageRecord {
        number: 0, // D1 自增
        png_url: body.png_url,
        prompt: body.prompt,
        negative: body.negative,
        seed: body.seed.unwrap_or(0),
        model: body.model,
        model_hash: body.model_hash,
        cfg_scale: body.cfg_scale,
        steps: body.steps,
        sampler: body.sampler,
        width: body.width,
        height: body.height,
        loras: body.loras,
        source: body.source,
        tags: body.tags.unwrap_or_default(),
        title: body.title.unwrap_or_default(),
        created_at: created_at.clone(),
        updated_at: Some(created_at),
    };

    match db::insert(&db, &rec).await {
        Ok(id) => Response::from_json(&response::ok(serde_json::json!({
            "number": id,
            "created_at": rec.created_at,
        }))),
        Err(e) => Response::from_json(&response::err(&ApiError::Other(e.to_string()))),
    }
}

/// GET /api/search — 搜索图片
pub async fn handle_search(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let db = match db::get_db(&ctx.env) {
        Ok(d) => d,
        Err(e) => return Response::from_json(&response::err(&ApiError::Other(e.to_string()))),
    };

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

    let records = match db::search(
        &db,
        q.as_deref(),
        model_filter.as_deref(),
        tag_filter.as_deref(),
        seed_filter,
    )
    .await
    {
        Ok(r) => r,
        Err(e) => return Response::from_json(&response::err(&ApiError::Other(e.to_string()))),
    };

    let items: Vec<Value> = records
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