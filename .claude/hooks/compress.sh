#!/bin/bash
# 压缩旧记忆文件 — 手动触发
#
# 用法: bash .claude/hooks/compress.sh
#
# 逻辑：
# 1. 保留永久文件（architecture.md, deployment.md, MEMORY.md）
# 2. 保留最近 7 天的日期文件
# 3. 将更早的日期文件合并到 archive/ 下
# 4. 更新索引

set -e

MEMORY_DIR="$(cd "$(dirname "$0")/../memory" && pwd)"
ARCHIVE_DIR="${MEMORY_DIR}/archive"
KEEP_DAYS=7

mkdir -p "$ARCHIVE_DIR"

# 找出所有日期文件（YYYY-MM-DD.md）
DATE_FILES=()
while IFS= read -r -d '' f; do
    basename "$f" .md
done < <(find "$MEMORY_DIR" -maxdepth 1 -name '20[0-9][0-9]-[0-9][0-9]-[0-9][0-9].md' -print0 | sort -z) | sort -r

total=${#DATE_FILES[@]}
keep=$KEEP_DAYS

if [ $total -le $keep ]; then
    echo "只有 $total 个日期文件，不需要压缩"
    exit 0
fi

echo "共 $total 个日期文件，保留最近 $keep 天"

# 归档旧文件
for ((i=keep; i<total; i++)); do
    f="${DATE_FILES[$i]}.md"
    if [ -f "$MEMORY_DIR/$f" ]; then
        echo "  归档: $f"
        first_line=$(head -1 "$MEMORY_DIR/$f" 2>/dev/null || echo "# 旧会话")
        cp "$MEMORY_DIR/$f" "$ARCHIVE_DIR/$f"
        rm "$MEMORY_DIR/$f"
    fi
done

# 更新索引：移除已归档的条目
ARCHIVE_COUNT=$((total - keep))
echo "完成，已归档 $ARCHIVE_COUNT 个文件"
echo "提示: 如需完全清理，可手动删除 archive/ 目录"