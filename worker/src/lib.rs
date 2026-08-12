//! AI Gallery API Worker
//!
//! 路由入口，所有处理器按功能拆分到 handlers/ 模块。
//! 数据源: Cloudflare D1 (ai_images 表)

use worker::*;

mod db;
mod github_notes;
mod handlers;
mod sync;

pub(crate) use handlers::images::*;
pub(crate) use handlers::cluster::*;
pub(crate) use handlers::stats::*;
pub(crate) use handlers::tags::*;
pub(crate) use handlers::deduce::*;
pub(crate) use handlers::notes::*;
pub(crate) use handlers::upload::*;

fn add_cors(res: Response) -> Result<Response> {
    let headers = Headers::new();
    headers.set("Access-Control-Allow-Origin", "*")?;
    headers.set("Access-Control-Allow-Methods", "GET, POST, DELETE, OPTIONS")?;
    headers.set("Access-Control-Allow-Headers", "*")?;
    Ok(res.with_headers(headers))
}

#[event(fetch)]
pub async fn main(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    if req.method() == Method::Options {
        return add_cors(Response::empty()?);
    }

    let router = Router::new();

    let res = router
        // ========== 图片列表与搜索 ==========
        .get_async("/api/images", |req, ctx| async move {
            handle_images(req, ctx).await
        })
        .get_async("/api/images/:number", |_, ctx| async move {
            let number: u64 = ctx.param("number")
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            handle_image_detail(ctx, number).await
        })

        // ========== 创建图片记录 ==========
        .post_async("/api/images", |mut req, ctx| async move {
            handle_create_image(&mut req, ctx).await
        })

        // ========== 图片上传代理（转发 ImgBed） ==========
        .post_async("/api/upload", |req, ctx| async move {
            handle_upload(req, ctx).await
        })

        // ========== 搜索 ==========
        .get_async("/api/search", |req, ctx| async move {
            handle_search(req, ctx).await
        })

        // ========== 聚类 ==========
        .get_async("/api/cluster", |req, ctx| async move {
            handle_cluster(req, ctx).await
        })

        // ========== 标签推荐 ==========
        .post_async("/api/tags/extract", |mut req, ctx| async move {
            handle_tags_extract(&mut req, ctx).await
        })

        // ========== 统计 ==========
        .get_async("/api/stats", |_, ctx| async move {
            handle_stats(ctx).await
        })

        // ========== 推演（prompt 共现推荐） ==========
        .get_async("/api/deduce/:token", |_, ctx| async move {
            let token = ctx.param("token").cloned().unwrap_or_default();
            handle_deduce(ctx, &token).await
        })

        // ========== 图片笔记（GitHub Issues） ==========
        .get_async("/api/images/:number/notes", |req, ctx| async move {
            handle_notes_list(req, ctx).await
        })
        .post_async("/api/images/:number/notes", |mut req, ctx| async move {
            handle_notes_create(&mut req, ctx).await
        })
        .delete_async("/api/images/:number/notes/:note_id", |req, ctx| async move {
            handle_notes_delete(req, ctx).await
        })

        // ========== ImgBed 自动同步 ==========
        .post_async("/api/sync-webhook", |req, ctx| async move {
            sync::handle_sync_webhook(req, &ctx.env).await
        })

        // ========== 健康检查 ==========
        .get_async("/health", |_, _| async move { Response::ok("ok") })
        .run(req, env)
        .await?;

    add_cors(res)
}

/// 定时任务：每 10 分钟自动同步 ImgBed 新增图片
#[event(scheduled)]
pub async fn scheduled(_scheduled: ScheduledEvent, env: Env, _ctx: ScheduleContext) {
    if let Err(e) = sync::handle_sync_cron(&env).await {
        console_log!("sync cron error: {}", e);
    }
}