//! 笔记处理器 — 图片笔记 CRUD
//!
//! 端点:
//!   GET    /api/images/:number/notes               — 列出笔记
//!   POST   /api/images/:number/notes               — 创建笔记
//!   DELETE /api/images/:number/notes/:note_id      — 删除笔记（关闭 Issue）
//!
//! 数据存储: GitHub Issues (mofelo/ai-notes)

use ai_gallery_core::error::ApiError;
use ai_gallery_core::response;
use chrono::Utc;
use serde::Deserialize;
use serde_json::json;
use worker::*;

/// 创建笔记请求体
#[derive(Debug, Deserialize)]
pub struct CreateNoteRequest {
    pub content: String,
}

/// GET /api/images/:number/notes — 列出某张图片的所有笔记
pub async fn handle_notes_list(_req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let number: u64 = match ctx.param("number").and_then(|s| s.parse().ok()) {
        Some(n) => n,
        None => {
            return Response::from_json(&response::err(&ApiError::Other(
                "无效的图片编号".to_string(),
            )))
        }
    };

    match crate::github_notes::list_notes(&ctx.env, number).await {
        Ok(notes) => Response::from_json(&response::ok(notes)),
        Err(e) => Response::from_json(&response::err(&ApiError::Other(e.to_string()))),
    }
}

/// POST /api/images/:number/notes — 创建笔记
pub async fn handle_notes_create(req: &mut Request, ctx: RouteContext<()>) -> Result<Response> {
    let number: u64 = match ctx.param("number").and_then(|s| s.parse().ok()) {
        Some(n) => n,
        None => {
            return Response::from_json(&response::err(&ApiError::Other(
                "无效的图片编号".to_string(),
            )))
        }
    };

    let body: CreateNoteRequest = match req.json().await {
        Ok(b) => b,
        Err(_) => {
            return Response::from_json(&response::err(&ApiError::Other(
                "无效的 JSON 请求体".to_string(),
            )))
        }
    };

    if body.content.trim().is_empty() {
        return Response::from_json(&response::err(&ApiError::Other(
            "笔记内容不能为空".to_string(),
        )));
    }

    match crate::github_notes::create_note(&ctx.env, number, &body.content).await {
        Ok(id) => {
            let now = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
            Response::from_json(&response::ok(json!({
                "id": id,
                "created_at": now,
            })))
        }
        Err(e) => Response::from_json(&response::err(&ApiError::Other(e.to_string()))),
    }
}

/// DELETE /api/images/:number/notes/:note_id — 删除笔记（关闭 Issue）
pub async fn handle_notes_delete(_req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let note_id: u64 = match ctx.param("note_id").and_then(|s| s.parse().ok()) {
        Some(id) => id,
        None => {
            return Response::from_json(&response::err(&ApiError::Other(
                "无效的笔记编号".to_string(),
            )))
        }
    };

    match crate::github_notes::delete_note(&ctx.env, note_id).await {
        Ok(()) => Response::from_json(&response::ok_empty()),
        Err(e) => Response::from_json(&response::err(&ApiError::Other(e.to_string()))),
    }
}