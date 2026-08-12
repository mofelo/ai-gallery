---
name: d1-migration-architecture
description: ai-gallery 从 GitHub Issues 迁移到 Cloudflare D1 的详细层架构方案
metadata:
  type: project
---

## 架构决策

采用双层架构，放弃空仓库 ai-images：

- **CloudFlare-ImgBed**（`https://img.boxblog.ccwu.cc`）= 存储层（"管子+阀门控制器"）
  - 已有完整大类 CRUD（upload/directoryTree/manage/delete|rename|move|tags）
  - **不改一行代码**，继续用
- **ai-gallery** = 详细层
  - 详细元数据（prompt/seed/model/采样器）存**自己的 D1 数据库**
  - 提供搜索/聚类/统计/推演
  - 前端 API 响应格式**不变**（`number` 字段保留）

## 关键文件

- `worker/migrations/0001_init.sql` — D1 建表脚本（已写好）
  - 表 `ai_images`：id/prompt/seed/model/cfg_scale/steps/sampler/width/height/loras/tags/title/created_at/updated_at
  - `tags` 存 JSON 字符串 `'[]'`，`created_at` 存 ISO 日期
  - 索引：idx_ai_images_seed、idx_ai_images_model、idx_ai_images_created_at
- `core/src/types.rs` — `ImageRecord`（18 字段）已有 `from_issue()` YAML 解析器，迁移后可删
- `worker/src/handlers/{images,cluster,deduce,stats}.rs` — 当前全依赖 GitHub API，需改 D1
- `worker/src/handlers/tags.rs` — **不动**（无 GitHub 依赖）

## 当前状态

1. ✅ 建表 SQL 已写好
2. ⏳ 待写 `worker/src/db.rs`（D1 访问层）
3. ⏳ 待改 4 个 handler
4. ⏳ 待新增 `POST /api/images` 写入端点
5. ⏳ 待改 issue-cli
6. ⏳ 待清理 GitHub 代码
7. 🔒 待用户跑 `wrangler login`（Cloudflare 账号 baobaolong12）后创建 D1 数据库

## 依赖

- Cloudflare 账号：baobaolong12
- ImgBed URL：`https://img.boxblog.ccwu.cc`
- 旧 GitHub token（待弃用）：GITHUB_TOKEN 存在 `.env`
- `wrangler login` 必须由用户在终端手动跑