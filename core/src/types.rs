//! AI 图片数据模型
//!
//! 一个 ImageRecord 对应一个 GitHub Issue，存储 AI 生成图片的完整元数据。
//! Issue body 使用 YAML frontmatter 格式，方便 Rust 和 GitHub 两端读写。

use serde::{Deserialize, Serialize};

/// AI 图片记录
///
/// 对应 GitHub Issue 的 frontmatter + 正文。
/// 字段名保持简短，利于 Issue 标题/标签的搜索命中。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageRecord {
    // ============ 核心字段 ============

    /// 图片 CDN URL（来自 CloudFlare-ImgBed）
    pub png_url: String,
    /// 生成提示词（正）
    pub prompt: String,
    /// 反向提示词
    pub negative: Option<String>,

    // ============ 生成参数 ============

    /// 随机种子
    pub seed: u64,
    /// 模型名称/CKPT 名
    pub model: Option<String>,
    /// 模型 Hash（用于 Civitai 查找）
    pub model_hash: Option<String>,
    /// CFG Scale
    pub cfg_scale: Option<f64>,
    /// 步数
    pub steps: Option<u32>,
    /// 采样器
    pub sampler: Option<String>,
    /// 宽度
    pub width: Option<u32>,
    /// 高度
    pub height: Option<u32>,

    // ============ 扩展 ============

    /// 使用的 LoRA（逗号分隔）
    pub loras: Option<String>,
    /// 图片来源平台（A1111 / ComfyUI / NovelAI / SD3）
    pub source: Option<String>,
    /// 标签（自动推荐 + 人工标注）
    pub tags: Vec<String>,

    // ============ 元数据 ============

    /// GitHub Issue 编号
    pub number: u64,
    /// 标题（简短描述）
    pub title: String,
    /// 创建时间
    pub created_at: String,
    /// 更新时间
    pub updated_at: Option<String>,
}

/// 从 GitHub Issue JSON 解析 ImageRecord
///
/// Issue body 格式：
/// ```text
/// ---
/// prompt: "..."
/// seed: 123456
/// model: "sd3.5_medium"
/// ---
/// 正文描述...
/// ```
impl ImageRecord {
    pub fn from_issue(issue: &serde_json::Value) -> Option<Self> {
        let number = issue["number"].as_u64()?;
        let title = issue["title"].as_str()?.to_string();
        let body = issue["body"].as_str().unwrap_or("");
        let created_at = issue["created_at"]
            .as_str()
            .unwrap_or("")
            .chars()
            .take(10)
            .collect::<String>();

        let labels: Vec<String> = issue["labels"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|l| l.as_str().map(|s| s.to_string()))
                    .filter(|l| !l.starts_with("state:"))
                    .collect()
            })
            .unwrap_or_default();

        // 解析 YAML frontmatter
        let (frontmatter, _description) = if let Some(rest) = body.strip_prefix("---") {
            if let Some(end) = rest.find("---") {
                let fm = &rest[..end];
                let desc = rest[end + 3..].trim().to_string();
                (fm.to_string(), desc)
            } else {
                ("".to_string(), body.to_string())
            }
        } else {
            ("".to_string(), body.to_string())
        };

        // 从 frontmatter 提取字段
        let prompt = extract_yaml_field(&frontmatter, "prompt").unwrap_or_default();
        let negative = extract_yaml_field(&frontmatter, "negative");
        let seed = extract_yaml_field(&frontmatter, "seed")
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let model = extract_yaml_field(&frontmatter, "model");
        let model_hash = extract_yaml_field(&frontmatter, "model_hash");
        let cfg_scale = extract_yaml_field(&frontmatter, "cfg_scale")
            .and_then(|s| s.parse().ok());
        let steps = extract_yaml_field(&frontmatter, "steps")
            .and_then(|s| s.parse().ok());
        let sampler = extract_yaml_field(&frontmatter, "sampler");
        let width = extract_yaml_field(&frontmatter, "width")
            .and_then(|s| s.parse().ok());
        let height = extract_yaml_field(&frontmatter, "height")
            .and_then(|s| s.parse().ok());
        let loras = extract_yaml_field(&frontmatter, "loras");
        let source = extract_yaml_field(&frontmatter, "source");
        let png_url = extract_yaml_field(&frontmatter, "png_url")
            .or_else(|| extract_yaml_field(&frontmatter, "img_url"))
            .unwrap_or_default();

        Some(ImageRecord {
            number,
            title,
            prompt,
            negative,
            seed,
            model,
            model_hash,
            cfg_scale,
            steps,
            sampler,
            width,
            height,
            loras,
            source,
            tags: labels,
            png_url,
            created_at,
            updated_at: issue["updated_at"].as_str().map(|s| s.chars().take(10).collect()),
        })
    }
}

/// 简单 YAML 字段提取器（处理 frontmatter 中的 key: value 行）
fn extract_yaml_field(frontmatter: &str, key: &str) -> Option<String> {
    for line in frontmatter.lines() {
        let trimmed = line.trim();
        if let Some(stripped) = trimmed.strip_prefix(&format!("{}:", key)) {
            let val = stripped.trim().trim_matches('"').trim().to_string();
            if !val.is_empty() {
                return Some(val);
            }
        }
    }
    None
}

/// 聚类结果节点
#[derive(Debug, Serialize, Deserialize)]
pub struct ClusterNode {
    pub id: String,
    pub title: String,
    pub prompt: String,
    pub png_url: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub tags: Vec<String>,
    pub model: Option<String>,
    pub seed: u64,
    pub created_at: String,
    pub weight: f64,
}

/// 聚类结果边
#[derive(Debug, Serialize, Deserialize)]
pub struct ClusterEdge {
    pub source: String,
    pub target: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub weight: f64,
}

/// 聚类结果簇
#[derive(Debug, Serialize, Deserialize)]
pub struct ClusterGroup {
    pub id: String,
    pub label: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub node_ids: Vec<String>,
    pub color: String,
}

/// 统计信息
#[derive(Debug, Serialize, Deserialize)]
pub struct GalleryStats {
    pub total_images: usize,
    pub top_models: Vec<(String, usize)>,
    pub top_tags: Vec<(String, usize)>,
    pub top_prompt_tokens: Vec<(String, usize)>,
    pub seed_distribution: Vec<(String, usize)>,
    pub by_month: Vec<(String, usize)>,
}

/// 推演结果 — 推荐 prompt 变体
#[derive(Debug, Serialize, Deserialize)]
pub struct DeduceResult {
    pub token: String,
    pub co_occurring: Vec<(String, f64)>,
    pub suggested_prompt: String,
    pub similar_images: Vec<ImageRecord>,
}