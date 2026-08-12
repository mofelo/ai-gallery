//! 图片聚类引擎
//!
//! 端点: GET /api/cluster
//! 从 GitHub Issues (ai-images) 拉取数据，按多个维度聚类：
//!   1. Prompt token 共现
//!   2. 模型相同
//!   3. Seed 变体（相同 prompt + 不同 seed）
//!   4. 时间（月份）
//!
//! 移植自 portfolio 的 cluster.rs，将聚类维度从 "影视标签/关键词"
//! 改为 "AI 生图参数（prompt/模型/seed/风格）"。

use crate::github::{get_token, github_api};
use ai_gallery_core::error::ApiError;
use ai_gallery_core::response;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use worker::*;

// ==================== 颜色生成 ====================

const CLUSTER_COLORS: &[&str] = &[
    "#c0392b", "#e74c3c", "#e67e22", "#f39c12", "#27ae60", "#2ecc71", "#1abc9c", "#3498db",
    "#2980b9", "#9b59b6", "#8e44ad", "#34495e", "#16a085", "#d35400", "#c0392b", "#7f8c8d",
];

fn hash_color(label: &str) -> String {
    let mut hash: i32 = 0;
    for c in label.chars() {
        hash = hash.wrapping_mul(31).wrapping_add(c as i32);
    }
    let idx = (hash.unsigned_abs() as usize) % CLUSTER_COLORS.len();
    CLUSTER_COLORS[idx].to_string()
}

// ==================== Prompt Token 提取 ====================

/// 从 prompt 中提取关键词 token
///
/// 拆分 prompt 为英文单词，过滤短词和常见修饰词，
/// 提取能代表图片特征的核心 token（人物/风格/场景/画质词）。
pub(crate) fn extract_prompt_tokens(prompt: &str) -> Vec<String> {
    let cleaned: String = prompt
        .chars()
        .map(|c| {
            if matches!(c, '\\' | '/' | '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>' | ':' | ';' | '"' | '\'' | ',' | '!' | '?' | '.') {
                ' '
            } else {
                c
            }
        })
        .collect();

    let mut words: Vec<String> = Vec::new();
    let mut current = String::new();

    for c in cleaned.chars() {
        if c.is_ascii_alphabetic() || c == '-' {
            current.push(c);
        } else if !current.is_empty() {
            let lower = current.to_lowercase();
            // 过滤常见修饰词和短词
            if lower.len() >= 3 && !PROMPT_STOP_WORDS.contains(&lower.as_str()) {
                words.push(lower);
            }
            current.clear();
        }
    }
    if !current.is_empty() {
        let lower = current.to_lowercase();
        if lower.len() >= 3 && !PROMPT_STOP_WORDS.contains(&lower.as_str()) {
            words.push(lower);
        }
    }

    // 频率统计 -> 取 top 8
    let mut freq: HashMap<String, usize> = HashMap::new();
    for w in &words {
        *freq.entry(w.clone()).or_insert(0) += 1;
    }

    let mut freq_vec: Vec<(String, usize)> = freq.into_iter().collect();
    freq_vec.sort_by(|a, b| b.1.cmp(&a.1));
    freq_vec.truncate(8);
    freq_vec.into_iter().map(|(word, _)| word).collect()
}

/// Prompt 停用词（常见但无聚类意义的修饰词）
const PROMPT_STOP_WORDS: &[&str] = &[
    "the", "a", "an", "and", "or", "of", "in", "on", "at", "to", "for", "with", "by", "from",
    "is", "are", "was", "were", "be", "been", "being", "have", "has", "had", "do", "does", "did",
    "will", "would", "could", "should", "may", "might", "can", "shall", "this", "that", "these",
    "those", "it", "its", "you", "your", "we", "our", "they", "their", "he", "she", "him", "her",
    "his", "my", "me", "all", "some", "any", "no", "not", "only", "just", "so", "too", "very",
    "than", "then", "also", "if", "but", "more", "most", "much", "many", "such", "like", "as",
    "up", "down", "out", "off", "over", "under", "again", "each", "every", "both", "few", "own",
    "same", "thing", "things", "make", "made", "get", "got", "see", "use", "used", "using",
    "high", "low", "best", "better", "good", "great", "new", "well", "really", "very", "quite",
    "still", "even", "back", "way", "long", "big", "small", "large", "lot", "top", "quality",
    "masterpiece", "bestquality", "highres", "ultradetailed",
];

// ==================== 节点归一化 ====================

fn normalize_nodes(issues: &[Value]) -> Vec<Value> {
    let mut nodes = Vec::new();

    for issue in issues {
        let number = issue["number"].as_u64().unwrap_or(0);
        let title = issue["title"].as_str().unwrap_or("").to_string();
        let body = issue["body"].as_str().unwrap_or("");
        let created_at = issue["created_at"]
            .as_str()
            .unwrap_or("")
            .chars()
            .take(10)
            .collect::<String>();

        // 从 body 中提取 prompt / png_url / model / seed
        let prompt = extract_frontmatter(body, "prompt").unwrap_or_default();
        let png_url = extract_frontmatter(body, "png_url")
            .or_else(|| extract_frontmatter(body, "img_url"))
            .unwrap_or_default();
        let model = extract_frontmatter(body, "model");
        let seed: u64 = extract_frontmatter(body, "seed")
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);

        let tags: Vec<String> = issue["labels"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|l| l.as_str().map(|s| s.to_string()))
                    .filter(|l| !l.starts_with("state:"))
                    .collect()
            })
            .unwrap_or_default();

        nodes.push(serde_json::json!({
            "id": format!("image:{}", number),
            "title": title,
            "prompt": prompt,
            "png_url": png_url,
            "model": model,
            "seed": seed,
            "tags": tags,
            "created_at": created_at,
            "number": number,
            "weight": 1,
        }));
    }

    nodes
}

/// 从 frontmatter 中提取字段
fn extract_frontmatter(body: &str, key: &str) -> Option<String> {
    if let Some(rest) = body.strip_prefix("---") {
        if let Some(end) = rest.find("---") {
            let fm = &rest[..end];
            for line in fm.lines() {
                let trimmed = line.trim();
                if let Some(stripped) = trimmed.strip_prefix(&format!("{}:", key)) {
                    let val = stripped.trim().trim_matches('"').trim().to_string();
                    if !val.is_empty() {
                        return Some(val);
                    }
                }
            }
        }
    }
    None
}

// ==================== 主聚类算法 ====================

fn compute_clusters(nodes: &[Value]) -> (Vec<Value>, Vec<Value>, Vec<Value>) {
    let mut edges: Vec<Value> = Vec::new();
    let mut clusters: Vec<Value> = Vec::new();
    let mut edge_set: HashSet<String> = HashSet::new();

    let mut add_edge = |source: &str, target: &str, etype: &str, weight: f64| {
        let mut key_parts = vec![source.to_string(), target.to_string(), etype.to_string()];
        key_parts.sort();
        let key = key_parts.join("::");
        if edge_set.contains(&key) {
            return;
        }
        edge_set.insert(key);
        edges.push(serde_json::json!({
            "source": source,
            "target": target,
            "type": etype,
            "weight": weight,
        }));
    };

    // ========== 1. 按模型聚类 ==========
    let mut model_to_nodes: HashMap<String, Vec<String>> = HashMap::new();
    for node in nodes {
        let id = node["id"].as_str().unwrap_or("").to_string();
        if let Some(model) = node["model"].as_str().filter(|m| !m.is_empty()) {
            model_to_nodes
                .entry(model.to_string())
                .or_default()
                .push(id);
        }
    }

    for (model, node_ids) in &model_to_nodes {
        if node_ids.len() < 2 {
            continue;
        }
        clusters.push(serde_json::json!({
            "id": format!("model:{}", model),
            "label": format!("模型: {}", model),
            "type": "model",
            "nodeIds": node_ids,
            "color": hash_color(model),
        }));
        for i in 0..node_ids.len() {
            for j in (i + 1)..node_ids.len() {
                add_edge(&node_ids[i], &node_ids[j], "model", 1.0);
            }
        }
    }

    // ========== 2. 按 Prompt Token 共现聚类 ==========
    let mut node_tokens: HashMap<String, Vec<String>> = HashMap::new();
    for node in nodes {
        let id = node["id"].as_str().unwrap_or("").to_string();
        let prompt = node["prompt"].as_str().unwrap_or("");
        let tokens = extract_prompt_tokens(prompt);
        node_tokens.insert(id, tokens);
    }

    let node_ids: Vec<String> = nodes
        .iter()
        .map(|n| n["id"].as_str().unwrap_or("").to_string())
        .collect();

    for i in 0..node_ids.len() {
        for j in (i + 1)..node_ids.len() {
            let a = &node_ids[i];
            let b = &node_ids[j];
            let ta = node_tokens.get(a).cloned().unwrap_or_default();
            let tb = node_tokens.get(b).cloned().unwrap_or_default();
            let shared: Vec<&String> = ta.iter().filter(|k| tb.contains(k)).collect();

            if shared.len() >= 2 {
                let cluster_key = format!("token:{}", shared[0]);
                let existing = clusters
                    .iter()
                    .position(|c| c["id"].as_str() == Some(&cluster_key));
                if let Some(idx) = existing {
                    let node_ids_arr = clusters[idx]["nodeIds"].as_array_mut().unwrap();
                    if !node_ids_arr.iter().any(|v| v.as_str() == Some(a)) {
                        node_ids_arr.push(serde_json::json!(a));
                    }
                    if !node_ids_arr.iter().any(|v| v.as_str() == Some(b)) {
                        node_ids_arr.push(serde_json::json!(b));
                    }
                } else {
                    clusters.push(serde_json::json!({
                        "id": cluster_key,
                        "label": format!("Prompt: {}", shared[0]),
                        "type": "prompt_token",
                        "nodeIds": [a, b],
                        "color": hash_color(shared[0]),
                    }));
                }
                let weight = (shared.len() as f64 / 3.0).min(1.0);
                add_edge(a, b, "prompt_token", weight);
            }
        }
    }

    // ========== 3. 按 Seed 变体聚类 ==========
    // 相同 prompt + 不同 seed 的图片
    let mut prompt_seed_map: HashMap<String, Vec<String>> = HashMap::new();
    for node in nodes {
        let id = node["id"].as_str().unwrap_or("").to_string();
        let prompt = node["prompt"].as_str().unwrap_or("").to_string();
        // 取 prompt 前 50 个字符作为 key（忽略 seed 差异）
        let key: String = prompt.chars().take(50).collect();
        if !key.is_empty() {
            prompt_seed_map.entry(key).or_default().push(id);
        }
    }

    for (prompt_prefix, ids) in &prompt_seed_map {
        if ids.len() < 2 {
            continue;
        }
        // 只有存在不同 seed 时才形成变体簇
        let seeds: HashSet<u64> = ids
            .iter()
            .filter_map(|id| {
                nodes
                    .iter()
                    .find(|n| n["id"].as_str() == Some(id))
                    .and_then(|n| n["seed"].as_u64())
            })
            .collect();
        if seeds.len() < 2 {
            continue;
        }

        clusters.push(serde_json::json!({
            "id": format!("seed_variant:{}", ids[0]),
            "label": format!("变体: {}...", prompt_prefix.chars().take(20).collect::<String>()),
            "type": "seed_variant",
            "nodeIds": ids,
            "color": hash_color(&format!("variants:{}", prompt_prefix)),
        }));

        for i in 0..ids.len() {
            for j in (i + 1)..ids.len() {
                add_edge(&ids[i], &ids[j], "seed_variant", 0.7);
            }
        }
    }

    // ========== 4. 按标签聚类 ==========
    let mut tag_to_nodes: HashMap<String, Vec<String>> = HashMap::new();
    for node in nodes {
        let id = node["id"].as_str().unwrap_or("").to_string();
        if let Some(tags) = node["tags"].as_array() {
            for tag_val in tags {
                if let Some(tag) = tag_val.as_str() {
                    tag_to_nodes
                        .entry(tag.to_string())
                        .or_default()
                        .push(id.clone());
                }
            }
        }
    }

    for (tag, ids) in &tag_to_nodes {
        if ids.len() < 2 {
            continue;
        }
        clusters.push(serde_json::json!({
            "id": format!("tag:{}", tag),
            "label": format!("标签: {}", tag),
            "type": "tag",
            "nodeIds": ids,
            "color": hash_color(tag),
        }));
        for i in 0..ids.len() {
            for j in (i + 1)..ids.len() {
                add_edge(&ids[i], &ids[j], "tag", 0.8);
            }
        }
    }

    // ========== 5. 按时间聚类 ==========
    let mut time_to_nodes: HashMap<String, Vec<String>> = HashMap::new();
    for node in nodes {
        let id = node["id"].as_str().unwrap_or("").to_string();
        let created_at = node["created_at"].as_str().unwrap_or("");
        if created_at.len() < 7 {
            continue;
        }
        let month: String = created_at.chars().take(7).collect();
        time_to_nodes.entry(month).or_default().push(id);
    }

    for (month, ids) in &time_to_nodes {
        if ids.len() < 2 {
            continue;
        }
        clusters.push(serde_json::json!({
            "id": format!("time:{}", month),
            "label": format!("{} 月", month),
            "type": "time",
            "nodeIds": ids,
            "color": hash_color(month),
        }));
        for i in 0..ids.len() {
            for j in (i + 1)..ids.len() {
                add_edge(&ids[i], &ids[j], "time", 0.5);
            }
        }
    }

    // ========== 6. 计算节点权重 ==========
    let mut weighted_nodes: Vec<Value> = nodes.to_vec();
    for node in &mut weighted_nodes {
        let id = node["id"].as_str().unwrap_or("");
        let edge_count = edges
            .iter()
            .filter(|e| e["source"].as_str() == Some(id) || e["target"].as_str() == Some(id))
            .count()
            .min(10) as f64;
        let cluster_count = clusters
            .iter()
            .filter(|c| {
                c["nodeIds"]
                    .as_array()
                    .map(|arr| arr.iter().any(|v| v.as_str() == Some(id)))
                    .unwrap_or(false)
            })
            .count() as f64;

        let weight = 1.0 + edge_count * 0.15 + cluster_count * 0.25;
        node["weight"] = serde_json::json!(weight);
    }

    (weighted_nodes, edges, clusters)
}

// ==================== 处理器 ====================

/// GET /api/cluster
///
/// 拉取 ai-images 仓库的所有 Issues，执行聚类，返回结果。
///
/// 响应:
/// ```json
/// {
///   "nodes": [...],
///   "edges": [...],
///   "clusters": [
///     { "id": "model:sd3.5", "label": "模型: sd3.5", "type": "model", "nodeIds": [...], "color": "..." },
///     { "id": "token:cyberpunk", "label": "Prompt: cyberpunk", "type": "prompt_token", "nodeIds": [...], "color": "..." },
///     { "id": "tag:portrait", "label": "标签: portrait", "type": "tag", "nodeIds": [...], "color": "..." },
///     ...
///   ]
/// }
/// ```
pub async fn handle_cluster(_req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let token = get_token(&ctx).await;
    if token.is_empty() {
        return Response::from_json(&response::err(&ApiError::Unauthorized));
    }

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

    // 归一化节点
    let nodes = normalize_nodes(&issues);

    // 计算聚类
    let (weighted_nodes, edges, clusters) = compute_clusters(&nodes);

    Response::from_json(&response::ok(serde_json::json!({
        "nodes": weighted_nodes,
        "edges": edges,
        "clusters": clusters,
    })))
}