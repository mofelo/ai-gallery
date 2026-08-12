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

/// 图片笔记记录（一条笔记 = 一条 GitHub Issue）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteRecord {
    /// GitHub Issue 编号
    pub id: u64,
    /// 关联的图片 number（从标题 [#N] 解析）
    pub number: u64,
    /// 笔记内容
    pub content: String,
    /// 创建时间
    pub created_at: String,
    /// 更新时间
    pub updated_at: String,
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