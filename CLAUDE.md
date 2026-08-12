# AI Gallery — 项目记忆

## 架构

- **数据存储**: Cloudflare D1 数据库 (`ai_images` 表) — 详细元数据（prompt/seed/model/cfg/采样器等）
- **私有笔记**: GitHub Issues（私有仓库 `mofelo/ai-notes`，每条笔记 = 一条 Issue，标题带 `[#N]` 前缀）
- **图床**: CloudFlare-ImgBed（纯存储/CDN，存图片文件）— **不改**
- **API Worker**: Rust + workers-rs，读取 D1 提供 REST API（搜索/聚类/统计/推演），笔记通过 GitHub API 读写
- **前端**: Astro + Cloudflare Pages（API 响应格式不变）

### 三层架构

```
CloudFlare-ImgBed (存储层/"管子+阀门控制器")
  └─ 已有完整 CRUD: upload/directoryTree/manage/delete|rename|move|tags
  └─ 不改一行代码

ai-gallery D1 (详细层)
  └─ 表 ai_images: id/prompt/seed/model/cfg_scale/steps/sampler/width/height/tags
  └─ 提供搜索/聚类/统计/推演
  └─ POST /api/images 写入，GET /api/images 读取

GitHub Issues (笔记层)
  └─ 私有仓库 mofelo/ai-notes
  └─ 每条笔记 = 一条 Issue（标题 `[#N]` 关联图片）
  └─ 私有性由私有仓库 + GITHUB_TOKEN 保证
```

## 账户

- GitHub 部署账户: **mofelo**
- 本地 `gh` 默认是 mkafw，部署/推送用 mofelo 的 token 需显式切换或设置 `GITHUB_TOKEN`
- Cloudflare 账户: baobaolong12

## 仓库

- 代码仓库: `github.com/mofelo/ai-gallery`
- 笔记仓库: `github.com/mofelo/ai-notes`（私有）

## 部署

- API Worker: `https://ai-gallery-api.baobaolong12.workers.dev`
- 前端: `https://ai-gallery.baobaolong12.workers.dev`
- Worker 构建: `worker-build --release`（在 worker/ 目录执行）
- Worker 部署: `cd worker && npx wrangler deploy --config wrangler.toml`
- 前端构建: `npm run build`（根目录）
- 前端部署: `npx wrangler deploy --config wrangler.jsonc`
- Worker 环境变量: `IMGBED_URL`, `GITHUB_NOTES_REPO`
- Worker Secret: `GITHUB_TOKEN`（mofelo 的 token，`repo` 权限）

## 项目结构

```
ai-gallery/
├── core/          — 数据模型、错误处理、响应格式
├── worker/        — Rust Worker API + D1 数据库访问层
│   ├── src/db.rs              — D1 读写封装（fetch_all/insert/search）
│   ├── src/github_notes.rs    — GitHub Issues API 客户端（笔记 CRUD）
│   ├── src/handlers/          — 搜索/聚类/统计/推演/标签/笔记/上传
│   └── migrations/            — D1 建表 SQL
├── issue-cli/     — CLI 工具（PNG 解析/ImgBed 上传/POST Worker API）
├── src/           — Astro 前端（画廊/搜索/统计/聚类/详情）
└── CLAUDE.md      — 本文件
```

## 自动记忆规则

此项目配置了 hooks 自动记忆系统（`.claude/settings.json`）：
- **SessionStart**: 自动加载 `.claude/memory/MEMORY.md` 索引
- **SessionStop**: 自动创建今日记忆文件 `.claude/memory/<日期>.md`
- **压缩**: 运行 `bash .claude/hooks/compress.sh` 归档旧记录

**作为 AI，请在以下时机自动写入记忆：**
1. 做出重要架构决策时 → 写入 `architecture.md`（永久文件）
2. 部署信息变更时 → 写入 `deployment.md`（永久文件）
3. 会话结束时 → 写入今日日期文件，包含：
   - 关键决策摘要
   - 修改了哪些文件
   - 待办事项

## 关键约定

- 图片元数据走 D1 数据库，笔记走 GitHub Issues（私有仓库 mofelo/ai-notes）
- 前端上传页 `/upload` 支持文件选择（经 Worker 代理到 ImgBed）和手动 URL 粘贴
- 笔记 API 端点: `GET/POST /api/images/:number/notes`、`DELETE /api/images/:number/notes/:note_id`
- 上传代理端点: `POST /api/upload`（转发到 ImgBed 解决 CORS）
- issue-cli 用法: `cargo run -p ai-gallery-cli -- upload <file>`
- issue-cli 需要 `IMGBED_URL` 和 `API_BASE` 环境变量
- 迁移进度: `memory/d1-migration-architecture.md`