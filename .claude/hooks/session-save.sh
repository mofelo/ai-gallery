#!/bin/bash
# SessionStop hook — 自动保存会话摘要到项目记忆
#
# 在会话结束时执行，将关键信息追加到 .claude/memory/<日期>.md
# 如果已有今日记录则跳过，避免重复

set -e

MEMORY_DIR="$(cd "$(dirname "$0")/../memory" && pwd)"
DATE_FILE="${MEMORY_DIR}/$(date +%Y-%m-%d).md"
INDEX_FILE="${MEMORY_DIR}/MEMORY.md"

# 如果今日已有记录，跳过（避免重复写入）
if [ -f "$DATE_FILE" ]; then
    exit 0
fi

# 写入今日会话记录（占位，实际内容由 Claude 在会话中写入）
cat > "$DATE_FILE" << EOF
# $(date +%Y-%m-%d) 会话

> 自动记录 — 会话结束 $(date +%H:%M)

## 关键决策

（由 CLAUDE.md 指令在会话结束时填充）

## 工作摘要

（由 CLAUDE.md 指令在会话结束时填充）
EOF

# 更新索引（如果尚未包含）
if ! grep -q "$(basename "$DATE_FILE")" "$INDEX_FILE" 2>/dev/null; then
    echo "- [$(date +%Y-%m-%d)]($(basename "$DATE_FILE")) — 会话记录" >> "$INDEX_FILE"
fi