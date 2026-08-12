# AI Gallery — 项目记忆

## 架构

- **数据存储**: GitHub Issues (`mofelo/ai-images`) — 每个 Issue 一张图，body 用 YAML frontmatter 存元数据
- **图床**: CloudFlare-ImgBed（纯存储/CDN，不做搜索）
- **API Worker**: Rust + workers-rs，读取 Issues 提供 REST API
- **前端**: Astro + Cloudflare Pages

## 账户

- GitHub 部署账户: **mofelo**（token 有 `repo` 权限，存在 keyring 中）
- 本地 `gh` 默认是 mkafw，部署/推送用 mofelo 的 token 需显式切换或设置 `GITHUB_TOKEN`
- Cloudflare 账户: baobaolong12

## 仓库

- 代码仓库: `github.com/mofelo/ai-gallery`
- 数据仓库: `github.com/mofelo/ai-images`

## 部署

- API Worker: `https://ai-gallery-api.baobaolong12.workers.dev`
- 前端: `https://ai-gallery.baobaolong12.workers.dev`
- Worker 构建: `worker-build --release`（在 worker/ 目录执行）
- 前端构建: `npm run build`（根目录）
- GITHUB_TOKEN 需作为 secret 设置到 `ai-gallery-api` Worker

## 项目结构

```
ai-gallery/
├── core/          — 数据模型、错误处理、GitHub API 契约
├── worker/        — Rust Worker API（搜索/聚类/统计/推演）
├── issue-cli/     — CLI 工具（PNG 解析/ImgBed 上传/建 Issue）
├── src/           — Astro 前端（画廊/搜索/统计/聚类/详情）
└── CLAUDE.md      — 本文件
```

## 关键约定

- `OWNER = "mofelo"`（在 `core/src/github_api.rs` 中）
- 图片数据始终走 GitHub Issues，不存数据库
- issue-cli 用法: `cargo run -p ai-gallery-cli -- upload <file>`
- issue-cli 需要 `GITHUB_TOKEN` 和 `IMGBED_URL` 环境变量