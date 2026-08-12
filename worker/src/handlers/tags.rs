//! 自动标签推荐处理器
//!
//! 端点: POST /api/tags/extract
//! 从 prompt + 正文中提取关键词、推荐标签、发现新标签候选。
//!
//! 移植自 portfolio 的 tags.rs，面向 AI 生图场景。
//! 重用中文分词逻辑，增加英文 prompt 专用的 token 提取。

use ai_gallery_core::error::ApiError;
use ai_gallery_core::response;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use worker::*;

#[derive(Deserialize)]
pub struct TagsRequest {
    pub prompt: String,
    #[serde(default)]
    pub body: String,
    #[serde(rename = "tagLibrary")]
    pub tag_library: Vec<String>,
}

// ==================== 停用词 ====================

const PROMPT_STOP_WORDS: &[&str] = &[
    "the", "a", "an", "and", "or", "of", "in", "on", "at", "to", "for", "with", "by", "from",
    "is", "are", "was", "were", "be", "been", "being", "have", "has", "had", "do", "does", "did",
    "will", "would", "could", "should", "may", "might", "can", "shall", "this", "that", "these",
    "those", "it", "its", "you", "your", "we", "our", "they", "their", "he", "she", "him", "her",
    "his", "my", "me", "all", "some", "any", "no", "not", "only", "just", "so", "too", "very",
    "than", "then", "also", "if", "but", "more", "most", "much", "many", "such", "like", "as",
    "up", "down", "out", "off", "over", "under", "again", "each", "every", "both", "few", "own",
    "same", "thing", "things", "make", "made", "get", "got", "see", "use", "used", "using",
    "high", "low", "best", "better", "good", "great", "new", "well", "really", "quite",
    "still", "even", "back", "way", "long", "big", "small", "large", "lot", "top", "quality",
    "masterpiece", "bestquality", "highres", "ultradetailed", "highlydetailed",
    "intricate", "beautiful", "stunning", "amazing", "gorgeous", "magnificent",
    "artstation", "deviantart", "pixiv", "pinterest",
];

// ==================== Token 提取 ====================

fn extract_prompt_tokens(text: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();

    for c in text.chars() {
        if c.is_ascii_alphabetic() || c == '-' {
            current.push(c);
        } else {
            if current.len() >= 3 {
                let lower = current.to_lowercase();
                if !PROMPT_STOP_WORDS.contains(&lower.as_str()) {
                    words.push(lower);
                }
            }
            current.clear();
        }
    }
    if current.len() >= 3 {
        let lower = current.to_lowercase();
        if !PROMPT_STOP_WORDS.contains(&lower.as_str()) {
            words.push(lower);
        }
    }

    // 频率统计
    let mut freq: HashMap<String, usize> = HashMap::new();
    for w in &words {
        *freq.entry(w.clone()).or_insert(0) += 1;
    }
    let mut freq_vec: Vec<(String, usize)> = freq.into_iter().collect();
    freq_vec.sort_by(|a, b| b.1.cmp(&a.1));
    freq_vec.into_iter().map(|(w, _)| w).collect()
}

// ==================== 核心算法 ====================

/// 从 prompt + 正文中提取关键词
fn extract_keywords(prompt: &str, body: &str, top_n: usize) -> Vec<Value> {
    let text = format!("{} {}", prompt, body);
    let tokens = extract_prompt_tokens(&text);

    let mut scored: Vec<(String, usize)> = tokens
        .into_iter()
        .enumerate()
        .map(|(i, t)| (t, 10 - (i as usize).min(9))) // 越靠前权重越高
        .collect();

    scored.sort_by(|a, b| b.1.cmp(&a.1));
    scored.truncate(top_n);

    let max_score = scored.first().map(|(_, s)| *s).unwrap_or(1).max(1);
    scored
        .into_iter()
        .map(|(text, score)| {
            serde_json::json!({
                "text": text,
                "score": ((score as f64 / max_score as f64) * 100.0).round() as i64,
            })
        })
        .collect()
}

/// 从现有标签库中推荐匹配标签
fn recommend_tags(prompt: &str, body: &str, tag_library: &[String]) -> Vec<Value> {
    let combined = format!("{} {}", prompt, body).to_lowercase();
    let mut results: Vec<(String, i64, String)> = Vec::new();

    for tag in tag_library {
        let lower_tag = tag.to_lowercase();
        let (score, reason) = if combined.contains(&lower_tag) {
            (100, "prompt 匹配".to_string())
        } else if tag.len() >= 3 {
            let partial = &lower_tag[..lower_tag
                .char_indices()
                .nth(3)
                .map(|(i, _)| i)
                .unwrap_or(lower_tag.len())];
            if combined.contains(partial) {
                (50, "部分匹配".to_string())
            } else {
                continue;
            }
        } else {
            continue;
        };
        results.push((tag.clone(), score, reason));
    }

    results.sort_by(|a, b| b.1.cmp(&a.1));
    results
        .into_iter()
        .map(|(tag, score, reason)| {
            serde_json::json!({ "tag": tag, "score": score, "reason": reason })
        })
        .collect()
}

/// 发现新标签候选
fn discover_new_tags(prompt: &str, body: &str, tag_library: &[String]) -> Vec<Value> {
    let keywords = extract_keywords(prompt, body, 30);
    let lower_library: Vec<String> = tag_library.iter().map(|t| t.to_lowercase()).collect();

    keywords
        .into_iter()
        .filter(|k| {
            let lower = k["text"].as_str().unwrap_or("").to_lowercase();
            !lower_library.contains(&lower)
                && !lower_library.iter().any(|l| l.contains(&lower))
        })
        .take(10)
        .collect()
}

// ==================== 处理器 ====================

/// POST /api/tags/extract
pub async fn handle_tags_extract(req: &mut Request, _ctx: RouteContext<()>) -> Result<Response> {
    let body: TagsRequest = match req.json().await {
        Ok(v) => v,
        Err(_) => {
            return Response::from_json(&response::err(&ApiError::Other(
                "Invalid JSON body".into(),
            )))
        }
    };

    let keywords = extract_keywords(&body.prompt, &body.body, 15);
    let recommendations = recommend_tags(&body.prompt, &body.body, &body.tag_library);
    let new_candidates = discover_new_tags(&body.prompt, &body.body, &body.tag_library);

    Response::from_json(&response::ok(serde_json::json!({
        "keywords": keywords,
        "recommendations": recommendations,
        "newCandidates": new_candidates,
    })))
}