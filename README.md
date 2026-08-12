# AI Gallery — AI 生成图片管理站

管理、归纳、总结、推演你的 AI 图片。

## 架构

```
┌─────────────────┐     ┌──────────────────┐     ┌──────────────────┐
│ CloudFlare-ImgBed│     │  Cloudflare D1   │     │  Rust Worker +   │
│ (纯图床/存储)    │     │  (数据库/详细层) │     │  Astro 前端      │
│                  │     │                  │     │                  │
│ 存图片文件       │     │  prompt/seed/    │     │  聚类/搜索/统计  │
│ 提供 CDN URL     │     │  model/tags      │     │  推演/详情       │
│ RESTful API      │     │  cfg/step/size   │     │  画廊/统计       │
└────────┬─────────┘     └────────┬─────────┘     └────────┬─────────┘
         │                        │                        │
         └────────────────────────┼────────────────────────┘
                                  │
                          Astro 前端页面
                       (图片 URL 从 ImgBed 取
                       元数据从 Worker 取)
```

## 数据流

1. CLI 上传图片到 CloudFlare-ImgBed → 拿到 CDN URL
2. CLI 读取 PNG 元数据（prompt/seed/model/...）
3. CLI POST JSON 到 Worker `/api/images` → 写入 D1 数据库
4. Rust Worker 读取 D1 → 提供 API（搜索/聚类/统计/推演）
5. Astro 前端展示图片（`<img src>` 指向 ImgBed CDN URL）

## 功能

### 管理
- 画廊浏览，按时间/模型/标签筛选
- 全文搜索（prompt/标题/标签/模型）
- 图片详情页（完整元数据展示）

### 归纳（聚类）
- 按模型聚类
- 按 Prompt token 共现聚类
- 按 Seed 变体聚类（同 prompt 不同 seed）
- 按标签聚类
- 按时序聚类

### 总结（统计）
- 模型使用频率
- 标签分布
- Prompt 关键词词云
- 月度趋势
- 采样器分布
- Seed 范围分布

### 推演
- Prompt token 共现分析
- 推荐搭配词
- 自动生成建议 prompt

## 项目结构

```
ai-gallery/
├── Cargo.toml              # Rust workspace
├── core/                   # 核心库（类型/错误/响应）
├── worker/                 # Cloudflare Worker（API + 算法）
│   ├── src/db.rs           # D1 数据库访问层
│   ├── src/handlers/       # 处理器（搜索/聚类/统计/推演/标签）
│   └── migrations/         # D1 建表 SQL
├── issue-cli/              # CLI 工具（上传/解析/POST Worker API）
├── src/                    # Astro 前端
│   ├── pages/
│   │   ├── index.astro     # 画廊首页
│   │   ├── search.astro    # 搜索页
│   │   ├── stats.astro     # 统计页
│   │   ├── cluster.astro   # 聚类页
│   │   └── image/[id].astro # 详情页
│   └── lib/api.ts          # API 客户端
└── wrangler.toml           # Cloudflare Pages 配置
```

## 部署

### Worker API

```bash
cd worker
wrangler deploy
```

### 前端

```bash
npm install
npm run build
wrangler deploy
```

### D1 数据库

```bash
# 创建数据库（首次）
npx wrangler d1 create ai-gallery
# 复制得到的 database_id 到 wrangler.toml

# 运行迁移（本地开发）
npx wrangler d1 execute ai-gallery --local --file=migrations/0001_init.sql

# 运行迁移（生产）
npx wrangler d1 execute ai-gallery --remote --file=migrations/0001_init.sql
```

### 环境变量

Worker 不需要 secret（D1 无鉴权）。

issue-cli 需要设置:
- `IMGBED_URL` — ImgBed 上传地址
- `API_BASE` — Worker API 地址（默认 `https://ai-gallery-api.baobaolong12.workers.dev`）

## 数据源

数据存储在 Cloudflare D1 数据库的 `ai_images` 表，字段：

| 字段 | 类型 | 说明 |
|------|------|------|
| id | INTEGER | 自增主键（API 中映射为 number） |
| png_url | TEXT | ImgBed CDN URL |
| prompt | TEXT | 生成提示词 |
| negative | TEXT | 反向提示词 |
| seed | INTEGER | 随机种子 |
| model | TEXT | 模型名称 |
| model_hash | TEXT | 模型 Hash |
| cfg_scale | REAL | CFG Scale |
| steps | INTEGER | 步数 |
| sampler | TEXT | 采样器 |
| width/height | INTEGER | 图片尺寸 |
| loras | TEXT | 使用的 LoRA |
| source | TEXT | 图片来源平台 |
| tags | TEXT | 标签 (JSON 数组) |
| title | TEXT | 标题 |
| created_at | TEXT | 创建时间 |
| updated_at | TEXT | 更新时间 |