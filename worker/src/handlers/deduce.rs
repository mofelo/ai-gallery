//! 推演处理器 — Prompt 共现推荐
//!
//! 端点: GET /api/deduce/:token
//! 给定一个 prompt token，分析其与其他 token 的共现关系，
//! 推荐最佳搭配词，并给出示例 prompt 建议。

use crate::github::{get_token, github_api};
use crate::handlers::cluster::extract_prompt_tokens;
use ai_gallery_core::error::ApiError;
use ai_gallery_core::response;
use ai_gallery_core::types::ImageRecord;
use serde_json::Value;
use std::collections::HashMap;
use worker::*;

/// GET /api/deduce/:token
pub async fn handle_deduce(ctx: RouteContext<()>, token: &str) -> Result<Response> {
    let t = get_token(&ctx).await;
    if t.is_empty() {
        return Response::from_json(&response::err(&ApiError::Unauthorized));
    }

    if token.is_empty() {
        return Response::from_json(&response::err(&ApiError::NotFound("token".to_string())));
    }

    let lower_token = token.to_lowercase();

    let value = match github_api(
        &t,
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

    let records: Vec<ImageRecord> = issues.iter().filter_map(ImageRecord::from_issue).collect();

    // 1. 找到包含该 token 的图片
    let matched: Vec<&ImageRecord> = records
        .iter()
        .filter(|r| {
            r.prompt.to_lowercase().contains(&lower_token)
                || r.title.to_lowercase().contains(&lower_token)
        })
        .collect();

    if matched.is_empty() {
        return Response::from_json(&response::ok(serde_json::json!({
            "token": token,
            "co_occurring": [],
            "suggested_prompt": "",
            "similar_images": [],
        })));
    }

    // 2. 计算共现 token 频率
    let mut co_occurrence: HashMap<String, usize> = HashMap::new();
    let mut total = 0usize;

    for r in &matched {
        let tokens = extract_prompt_tokens(&r.prompt);
        for tk in tokens {
            if tk != lower_token {
                *co_occurrence.entry(tk).or_insert(0) += 1;
            }
        }
        total += 1;
    }

    let mut co_occurring: Vec<(String, f64)> = co_occurrence
        .into_iter()
        .map(|(tk, count)| {
            let confidence = count as f64 / total as f64;
            (tk, confidence)
        })
        .collect();

    co_occurring.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    co_occurring.truncate(10);

    // 3. 构建建议 prompt
    let suggested_prompt = if co_occurring.len() >= 2 {
        let top = co_occurring[0].0.clone();
        let second = if co_occurring.len() > 1 {
            format!(", {}", co_occurring[1].0)
        } else {
            String::new()
        };
        format!("{}, {}, {}{}", token, top, second, ", masterpiece, best quality")
    } else {
        format!("{}, masterpiece, best quality", token)
    };

    // 4. 返回相似图片（含该 token 的最近几张）
    let mut sorted = matched.clone();
    sorted.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    let similar_images: Vec<Value> = sorted
        .iter()
        .take(6)
        .map(|r| {
            serde_json::json!({
                "number": r.number,
                "title": r.title,
                "prompt": r.prompt,
                "png_url": r.png_url,
                "seed": r.seed,
                "model": r.model,
                "created_at": r.created_at,
            })
        })
        .collect();

    Response::from_json(&response::ok(serde_json::json!({
        "token": token,
        "match_count": matched.len(),
        "co_occurring": co_occurring,
        "suggested_prompt": suggested_prompt,
        "similar_images": similar_images,
    })))
}