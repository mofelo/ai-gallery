//! D1 数据库访问层
//!
//! 封装 ai_images 表的所有 D1 读写操作，替代旧 GitHub Issues 数据源。
//!
//! 数据流:
//! ```text
//! ImgBed (存储层)  ←─ CDN URL ─→  ai_images (详细层，本模块)
//! ```

use ai_gallery_core::types::ImageRecord;
use serde::Deserialize;
use worker::{D1Type, Env, Result};

// ============ 行结构 ============

/// D1 行反序列化结构
///
/// 从 D1 读出的原始行，字段与 ai_images 表一一对应。
/// tags 存 JSON 字符串，读入时反序列化为 Vec<String>。
#[derive(Debug, Clone, Deserialize)]
struct D1ImageRow {
    id: i64,
    png_url: String,
    prompt: String,
    negative: Option<String>,
    seed: i64,
    model: Option<String>,
    model_hash: Option<String>,
    cfg_scale: Option<f64>,
    steps: Option<i32>,
    sampler: Option<String>,
    width: Option<i32>,
    height: Option<i32>,
    loras: Option<String>,
    source: Option<String>,
    tags: String,
    title: String,
    created_at: String,
    updated_at: Option<String>,
}

/// D1 行 → ImageRecord
fn row_to_record(row: D1ImageRow) -> Option<ImageRecord> {
    let tags: Vec<String> = serde_json::from_str(&row.tags).ok().unwrap_or_default();

    Some(ImageRecord {
        number: row.id as u64,
        png_url: row.png_url,
        prompt: row.prompt,
        negative: row.negative,
        seed: row.seed as u64,
        model: row.model,
        model_hash: row.model_hash,
        cfg_scale: row.cfg_scale,
        steps: row.steps.map(|s| s as u32),
        sampler: row.sampler,
        width: row.width.map(|w| w as u32),
        height: row.height.map(|h| h as u32),
        loras: row.loras,
        source: row.source,
        tags,
        title: row.title,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

// ============ 辅助 ============

/// 从 Env 获取 D1 数据库绑定
pub fn get_db(env: &Env) -> Result<worker::D1Database> {
    env.d1("ai_gallery")
}

/// 构造 bind_refs 用的 D1Type 引用列表
fn bind_opt_text(val: Option<&String>) -> D1Type<'_> {
    match val {
        Some(s) => D1Type::Text(s),
        None => D1Type::Null,
    }
}

// ============ 读取 ============

/// 查询全部记录（按 id 降序）
pub async fn fetch_all(db: &worker::D1Database) -> Result<Vec<ImageRecord>> {
    let stmt = db.prepare("SELECT * FROM ai_images ORDER BY id DESC");
    let result = stmt.all().await?;
    let rows = result.results::<D1ImageRow>()?;
    Ok(rows.into_iter().filter_map(row_to_record).collect())
}

/// 查询单条记录
pub async fn fetch_one(db: &worker::D1Database, number: u64) -> Result<Option<ImageRecord>> {
    let stmt = db.prepare("SELECT * FROM ai_images WHERE id = ?1");
    let params = vec![D1Type::Real(number as f64)];
    let stmt = stmt.bind_refs(&params)?;
    let result = stmt.all().await?;
    let rows = result.results::<D1ImageRow>()?;
    match rows.into_iter().next() {
        Some(row) => Ok(row_to_record(row)),
        None => Ok(None),
    }
}

/// 查询所有已存在的 png_url 值
///
/// 用于自动同步去重（webhook / cron 幂等检查）。
pub async fn list_all_png_urls(db: &worker::D1Database) -> Result<Vec<String>> {
    let stmt = db.prepare("SELECT png_url FROM ai_images");
    let result = stmt.all().await?;
    let rows = result.results::<PngUrlRow>()?;
    Ok(rows.into_iter().filter_map(|r| r.png_url).collect())
}

/// png_url 行结构（png_url 可能为 NULL，用 Option 过滤）
#[derive(Debug, Clone, Deserialize)]
struct PngUrlRow {
    png_url: Option<String>,
}

/// 查询数量
#[allow(dead_code)]
pub async fn count_all(db: &worker::D1Database) -> Result<usize> {
    let stmt = db.prepare("SELECT COUNT(*) as cnt FROM ai_images");
    let result = stmt.first::<i64>(Some("cnt")).await?;
    Ok(result.unwrap_or(0) as usize)
}

/// 条件搜索
///
/// 支持按全文(prompt/title)、模型、标签、种子筛选。
/// 结果按 id 降序。
pub async fn search(
    db: &worker::D1Database,
    q: Option<&str>,
    model: Option<&str>,
    tag: Option<&str>,
    seed: Option<u64>,
) -> Result<Vec<ImageRecord>> {
    let mut conditions = Vec::new();
    let mut params: Vec<D1Type> = Vec::new();
    let mut idx = 0usize;

    // 预计算 like 字符串，使其在函数作用域存活，以供 D1Type::Text 借用
    let q_like = q.map(|query| format!("%{}%", query));
    let m_like = model.map(|m| format!("%{}%", m));
    let t_like = tag.map(|t| format!("%{}%", t));

    if let Some(ref like) = q_like {
        idx += 1;
        conditions.push(format!(
            "LOWER(prompt) LIKE ?{} OR LOWER(title) LIKE ?{} OR LOWER(model) LIKE ?{}",
            idx, idx, idx
        ));
        params.push(D1Type::Text(like));
    }

    if let Some(ref like) = m_like {
        idx += 1;
        conditions.push(format!("LOWER(model) LIKE ?{}", idx));
        params.push(D1Type::Text(like));
    }

    if let Some(ref like) = t_like {
        idx += 1;
        conditions.push(format!("LOWER(tags) LIKE ?{}", idx));
        params.push(D1Type::Text(like));
    }

    if let Some(s) = seed {
        idx += 1;
        conditions.push(format!("seed = ?{}", idx));
        params.push(D1Type::Real(s as f64));
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    let sql = format!("SELECT * FROM ai_images {} ORDER BY id DESC", where_clause);

    let stmt = db.prepare(&sql);
    let stmt = stmt.bind_refs(&params)?;
    let result = stmt.all().await?;
    let rows = result.results::<D1ImageRow>()?;
    Ok(rows.into_iter().filter_map(row_to_record).collect())
}

// ============ 写入 ============

/// 插入新记录，返回自增 ID
pub async fn insert(db: &worker::D1Database, rec: &ImageRecord) -> Result<i64> {
    let tags_json = serde_json::to_string(&rec.tags).unwrap_or_else(|_| "[]".to_string());

    let params: Vec<D1Type> = vec![
        D1Type::Text(&rec.png_url),
        D1Type::Text(&rec.prompt),
        bind_opt_text(rec.negative.as_ref()),
        D1Type::Real(rec.seed as f64),
        bind_opt_text(rec.model.as_ref()),
        bind_opt_text(rec.model_hash.as_ref()),
        rec.cfg_scale.map_or(D1Type::Null, |v| D1Type::Real(v)),
        rec.steps.map_or(D1Type::Null, |v| D1Type::Integer(v as i32)),
        bind_opt_text(rec.sampler.as_ref()),
        rec.width.map_or(D1Type::Null, |v| D1Type::Integer(v as i32)),
        rec.height.map_or(D1Type::Null, |v| D1Type::Integer(v as i32)),
        bind_opt_text(rec.loras.as_ref()),
        bind_opt_text(rec.source.as_ref()),
        D1Type::Text(&tags_json),
        D1Type::Text(&rec.title),
        D1Type::Text(&rec.created_at),
        bind_opt_text(rec.updated_at.as_ref()),
    ];

    let stmt = db.prepare(
        "INSERT INTO ai_images \
         (png_url, prompt, negative, seed, model, model_hash, \
          cfg_scale, steps, sampler, width, height, loras, source, \
          tags, title, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
    );

    let stmt = stmt.bind_refs(&params)?;
    let result = stmt.run().await?;
    let last_id = result.meta()?.and_then(|m| m.last_row_id).unwrap_or(-1);
    Ok(last_id)
}