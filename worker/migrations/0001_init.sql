-- ai_gallery D1 初始化
-- 详细层：存储 AI 图片的 prompt/seed/model/采样器等元数据
-- CloudFlare-ImgBed 是存储层，存图片和大类；本表存详细生成参数

CREATE TABLE IF NOT EXISTS ai_images (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,  -- API 中映射为 number
    png_url     TEXT NOT NULL,                      -- ImgBed CDN URL
    prompt      TEXT NOT NULL,
    negative    TEXT,
    seed        INTEGER NOT NULL DEFAULT 0,
    model       TEXT,
    model_hash  TEXT,
    cfg_scale   REAL,
    steps       INTEGER,
    sampler     TEXT,
    width       INTEGER,
    height      INTEGER,
    loras       TEXT,
    source      TEXT,
    tags        TEXT NOT NULL DEFAULT '[]',         -- JSON 数组字符串
    title       TEXT NOT NULL,
    created_at  TEXT NOT NULL,
    updated_at  TEXT
);

CREATE INDEX IF NOT EXISTS idx_ai_images_seed ON ai_images(seed);
CREATE INDEX IF NOT EXISTS idx_ai_images_model ON ai_images(model);
CREATE INDEX IF NOT EXISTS idx_ai_images_created_at ON ai_images(created_at DESC);