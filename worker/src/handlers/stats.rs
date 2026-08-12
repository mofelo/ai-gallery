//! 统计处理器
//!
//! 端点: GET /api/stats
//! 聚合展示：模型使用频率、标签分布、prompt token 频率、seed 分布、每月趋势

use crate::github::{get_token, github_api};
use crate::handlers::cluster::extract_prompt_tokens;
use ai_gallery_core::error::ApiError;
use ai_gallery_core::response;
use ai_gallery_core::types::ImageRecord;
use std::collections::HashMap;
use worker::*;

/// GET /api/stats
pub async fn handle_stats(ctx: RouteContext<()>) -> Result<Response> {
    let token = get_token(&ctx).await;
    if token.is_empty() {
        return Response::from_json(&response::err(&ApiError::Unauthorized));
    }

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

    let records: Vec<ImageRecord> = issues.iter().filter_map(ImageRecord::from_issue).collect();

    // 1. 模型使用频率
    let mut model_freq: HashMap<String, usize> = HashMap::new();
    for r in &records {
        if let Some(ref m) = r.model {
            *model_freq.entry(m.clone()).or_insert(0) += 1;
        }
    }
    let mut top_models: Vec<(String, usize)> = model_freq.into_iter().collect();
    top_models.sort_by(|a, b| b.1.cmp(&a.1));
    top_models.truncate(20);

    // 2. 标签分布
    let mut tag_freq: HashMap<String, usize> = HashMap::new();
    for r in &records {
        for t in &r.tags {
            *tag_freq.entry(t.clone()).or_insert(0) += 1;
        }
    }
    let mut top_tags: Vec<(String, usize)> = tag_freq.into_iter().collect();
    top_tags.sort_by(|a, b| b.1.cmp(&a.1));
    top_tags.truncate(20);

    // 3. Prompt token 频率
    let mut token_freq: HashMap<String, usize> = HashMap::new();
    for r in &records {
        let tokens = extract_prompt_tokens(&r.prompt);
        for t in tokens {
            *token_freq.entry(t).or_insert(0) += 1;
        }
    }
    let mut top_tokens: Vec<(String, usize)> = token_freq.into_iter().collect();
    top_tokens.sort_by(|a, b| b.1.cmp(&a.1));
    top_tokens.truncate(30);

    // 4. Seed 范围分布
    let mut seed_ranges: HashMap<String, usize> = HashMap::new();
    for r in &records {
        let range = if r.seed == 0 {
            "seed:0".to_string()
        } else {
            let bucket = r.seed / 1000000 * 1000000;
            format!("{}-{}", bucket, bucket + 999999)
        };
        *seed_ranges.entry(range).or_insert(0) += 1;
    }
    let mut seed_dist: Vec<(String, usize)> = seed_ranges.into_iter().collect();
    seed_dist.sort_by(|a, b| a.0.cmp(&b.0));

    // 5. 每月趋势
    let mut month_freq: HashMap<String, usize> = HashMap::new();
    for r in &records {
        if r.created_at.len() >= 7 {
            let month = r.created_at.chars().take(7).collect::<String>();
            *month_freq.entry(month).or_insert(0) += 1;
        }
    }
    let mut by_month: Vec<(String, usize)> = month_freq.into_iter().collect();
    by_month.sort_by(|a, b| a.0.cmp(&b.0));

    // 6. 采样器分布
    let mut sampler_freq: HashMap<String, usize> = HashMap::new();
    for r in &records {
        if let Some(ref s) = r.sampler {
            *sampler_freq.entry(s.clone()).or_insert(0) += 1;
        }
    }
    let mut top_samplers: Vec<(String, usize)> = sampler_freq.into_iter().collect();
    top_samplers.sort_by(|a, b| b.1.cmp(&a.1));

    Response::from_json(&response::ok(serde_json::json!({
        "total_images": records.len(),
        "top_models": top_models,
        "top_tags": top_tags,
        "top_prompt_tokens": top_tokens,
        "seed_distribution": seed_dist,
        "by_month": by_month,
        "top_samplers": top_samplers,
    })))
}