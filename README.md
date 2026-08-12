# AI Gallery — AI 生成图片管理站

管理、归纳、总结、推演你的 AI 图片。

## 架构

```
┌─────────────────┐     ┌──────────────────┐     ┌──────────────────┐
│ CloudFlare-ImgBed│     │  GitHub Issues   │     │  Rust Worker +   │
│ (纯图床/存储)    │     │  (元数据/参数)   │     │  Astro 前端      │
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
3. CLI 创建 GitHub Issue（ai-images 仓库），body 含 frontmatter 格式元数据
4. Rust Worker 读取 Issues → 提供 API
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
├── core/                   # 核心库（类型/错误/响应/GitHub API）
├── worker/                 # Cloudflare Worker（API + 算法）
├── issue-cli/              # CLI 工具（上传/解析/建 issue）
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

### 环境变量

Worker 需要设置:
- `GITHUB_TOKEN` — GitHub Personal Access Token（访问 ai-images 仓库）

## 数据源

GitHub 仓库 `ai-images`，每个 Issue 一张图片，body 使用 YAML frontmatter 格式：

```yaml
---
prompt: "cyberpunk girl, neon lights, intricate details"
negative: "low quality, blurry"
seed: 123456789
model: "sd3.5_medium"
model_hash: "abc123def"
cfg_scale: 7.0
steps: 30
sampler: "DPM++ 2M Karras"
width: 1024
height: 1024
loras: "detail_enhancer"
source: "A1111"
png_url: "https://imgbed.example.com/file/123_test.png"
---
图片描述...
```