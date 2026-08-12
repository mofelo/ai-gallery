//! AI Gallery CLI
//!
//! 命令行工具，用于：
//! 1. 上传图片到 CloudFlare-ImgBed
//! 2. 读取 PNG 元数据（prompt/seed/model）
//! 3. 创建 GitHub Issue
//!
//! ## 用法
//!
//! ```bash
//! # 上传图片（自动解析元数据 + 建 Issue）
//! cargo run -- upload path/to/image.png
//!
//! # 只读取元数据
//! cargo run -- read-meta path/to/image.png
//!
//! # 列出所有图片
//! cargo run -- list
//! ```

use clap::Parser;
use ai_gallery_core::types::ImageRecord;

mod metadata;
mod upload;
mod github;

#[derive(Parser)]
#[command(name = "ai-gallery-cli", about = "AI Gallery — 管理你的 AI 生成图片")]
enum Cli {
    /// 上传图片到 ImgBed 并自动创建 GitHub Issue
    Upload {
        /// 图片文件路径
        file: String,
        /// 提示词（可选，默认从 PNG 元数据读取）
        #[arg(long)]
        prompt: Option<String>,
        /// 随机种子（可选，默认从元数据读取）
        #[arg(long)]
        seed: Option<u64>,
        /// 模型名称（可选，默认从元数据读取）
        #[arg(long)]
        model: Option<String>,
        /// 标签（逗号分隔，可选）
        #[arg(long)]
        tags: Option<String>,
        /// ImgBed 地址（默认从 IMGBED_URL 环境变量读取）
        #[arg(long, env = "IMGBED_URL")]
        imgbed_url: Option<String>,
        /// 图片标题（可选，默认自动从 prompt 截取）
        #[arg(long)]
        title: Option<String>,
        /// 是否跳过上传（仅创建 Issue，使用已有 URL）
        #[arg(long)]
        png_url: Option<String>,
    },
    /// 从 PNG 文件读取元数据并显示
    ReadMeta {
        /// PNG 文件路径
        file: String,
        /// 以 JSON 格式输出
        #[arg(long)]
        json: bool,
    },
    /// 列出所有图片
    List {
        /// 搜索关键词
        #[arg(short, long)]
        search: Option<String>,
        /// 以 JSON 格式输出
        #[arg(long)]
        json: bool,
    },
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    match cli {
        Cli::Upload { file, prompt, seed, model, tags, imgbed_url, title, png_url } => {
            handle_upload(file, prompt, seed, model, tags, imgbed_url, title, png_url).await;
        }
        Cli::ReadMeta { file, json } => {
            handle_read_meta(file, json);
        }
        Cli::List { search, json } => {
            handle_list(search, json).await;
        }
    }
}

/// 处理 Upload 命令
async fn handle_upload(
    file: String, cli_prompt: Option<String>, cli_seed: Option<u64>,
    cli_model: Option<String>, cli_tags: Option<String>,
    imgbed_url: Option<String>, cli_title: Option<String>,
    cli_png_url: Option<String>,
) {
    println!("📤 处理图片: {}", file);

    // 1. 读取 PNG 元数据
    let meta = metadata::read_png_metadata(&file);
    let meta = match meta {
        Ok(m) => {
            println!("  ✅ 读取到元数据 (来源: {})", m.source);
            m
        }
        Err(e) => {
            eprintln!("  ⚠️  {}", e);
            println!("  ℹ️  继续使用命令行参数（如果有的话）");
            metadata::ParsedMetadata::default()
        }
    };

    // 合并命令行参数（优先级高于元数据）
    let prompt = cli_prompt.as_ref().cloned().unwrap_or_else(|| meta.prompt.clone());
    let seed = cli_seed.unwrap_or(meta.seed);
    let model = cli_model.clone().unwrap_or_else(|| meta.model.clone());
    let source = meta.source.clone();

    // 如果有命令行 prompt，覆盖 source
    let source = if cli_prompt.is_some() { "manual".to_string() } else { source };

    // 2. 上传到 ImgBed（或使用已有 URL）
    let cdn_url = if let Some(url) = cli_png_url {
        println!("  🔗 使用已有 URL: {}", url);
        url
    } else if let Some(ref imgbed) = imgbed_url {
        println!("  ☁️  上传到 ImgBed: {}", imgbed);
        match upload::upload_to_imgbed_simple(imgbed, &file, None).await {
            Ok(url) => {
                println!("  ✅ 上传成功: {}", url);
                url
            }
            Err(e) => {
                eprintln!("  ❌ 上传失败: {}", e);
                std::process::exit(1);
            }
        }
    } else {
        eprintln!("  ❌ 需要 ImgBed URL。请设置 IMGBED_URL 环境变量或使用 --imgbed-url 参数");
        std::process::exit(1);
    };

    // 3. 构建 Issue
    let tags: Vec<String> = cli_tags
        .as_ref()
        .map(|t| t.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect())
        .unwrap_or_default();

    // 自动生成标题
    let title = cli_title.unwrap_or_else(|| {
        let first_line = prompt.lines().next().unwrap_or(&prompt);
        // 取前 60 个字符作为标题
        let trimmed: String = first_line
            .chars()
            .filter(|c| !c.is_whitespace() || c == &' ')
            .collect::<String>()
            .trim()
            .chars()
            .take(60)
            .collect();
        if trimmed.is_empty() {
            format!("AI Image - seed {}", seed)
        } else {
            trimmed
        }
    });

    let body = github::build_issue_body(
        &prompt,
        if meta.negative.is_empty() { None } else { Some(&meta.negative) },
        seed,
        if model.is_empty() { None } else { Some(&model) },
        if meta.model_hash.is_empty() { None } else { Some(&meta.model_hash) },
        meta.cfg_scale,
        meta.steps,
        if meta.sampler.is_empty() { None } else { Some(&meta.sampler) },
        meta.width,
        meta.height,
        if meta.loras.is_empty() { None } else { Some(&meta.loras) },
        if source.is_empty() { None } else { Some(&source) },
        &cdn_url,
        &tags,
    );

    // 4. 创建 GitHub Issue
    println!("  📝 创建 GitHub Issue...");
    match github::create_issue(&title, &body, &tags).await {
        Ok(issue) => {
            let number = issue["number"].as_u64().unwrap_or(0);
            let html_url = issue["html_url"].as_str().unwrap_or("");
            println!("  ✅ Issue #{} 创建成功!", number);
            println!("  🔗 {}", html_url);
        }
        Err(e) => {
            eprintln!("  ❌ 创建 Issue 失败: {}", e);
            std::process::exit(1);
        }
    }
}

/// 处理 ReadMeta 命令
fn handle_read_meta(file: String, json_output: bool) {
    println!("📖 读取元数据: {}", file);

    match metadata::read_png_metadata(&file) {
        Ok(meta) => {
            if json_output {
                println!("{}", serde_json::to_string_pretty(&serde_json::json!({
                    "prompt": meta.prompt,
                    "negative": meta.negative,
                    "seed": meta.seed,
                    "model": meta.model,
                    "model_hash": meta.model_hash,
                    "cfg_scale": meta.cfg_scale,
                    "steps": meta.steps,
                    "sampler": meta.sampler,
                    "width": meta.width,
                    "height": meta.height,
                    "loras": meta.loras,
                    "source": meta.source,
                })).unwrap());
            } else {
                println!("  来源:    {}", meta.source);
                println!("  Prompt:  {}", meta.prompt);
                if !meta.negative.is_empty() {
                    println!("  Neg:     {}", meta.negative);
                }
                println!("  Seed:    {}", meta.seed);
                println!("  Model:   {}", meta.model);
                if !meta.model_hash.is_empty() {
                    println!("  Hash:    {}", meta.model_hash);
                }
                if meta.cfg_scale > 0.0 {
                    println!("  CFG:     {}", meta.cfg_scale);
                }
                if meta.steps > 0 {
                    println!("  Steps:   {}", meta.steps);
                }
                if !meta.sampler.is_empty() {
                    println!("  Sampler: {}", meta.sampler);
                }
                if meta.width > 0 && meta.height > 0 {
                    println!("  Size:    {}x{}", meta.width, meta.height);
                }
                if !meta.loras.is_empty() {
                    println!("  LoRAs:   {}", meta.loras);
                }
            }
        }
        Err(e) => {
            eprintln!("❌ {}", e);
            std::process::exit(1);
        }
    }
}

/// 处理 List 命令
async fn handle_list(search: Option<String>, json_output: bool) {
    println!("📋 获取图片列表...");

    let issues = if let Some(q) = &search {
        match github::search_issues(q).await {
            Ok(issues) => {
                println!("  搜索: \"{}\"", q);
                issues
            }
            Err(e) => {
                eprintln!("❌ 搜索失败: {}", e);
                std::process::exit(1);
            }
        }
    } else {
        match github::list_issues().await {
            Ok(issues) => issues,
            Err(e) => {
                eprintln!("❌ 获取列表失败: {}", e);
                std::process::exit(1);
            }
        }
    };

    if issues.is_empty() {
        println!("  (没有图片)");
        return;
    }

    // 解析为 ImageRecord
    let records: Vec<ImageRecord> = issues.iter()
        .filter_map(|issue| ImageRecord::from_issue(issue))
        .collect();

    if json_output {
        println!("{}", serde_json::to_string_pretty(&records).unwrap());
    } else {
        println!("  共 {} 张图片\n", records.len());
        for record in &records {
            let title = if record.title.len() > 50 {
                format!("{}...", &record.title[..47])
            } else {
                record.title.clone()
            };
            println!("  #{:<5} {} ({} | {})",
                record.number,
                title,
                record.model.as_deref().unwrap_or("unknown"),
                record.created_at,
            );
        }
    }
}