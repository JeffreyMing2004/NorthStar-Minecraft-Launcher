#!/usr/bin/env bash
# 从 git 历史生成贡献者列表，并写入 README 的 contributors 区块与 CONTRIBUTORS.md。
# 用法：bash scripts/update-contributors.sh
set -euo pipefail

root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$root"

# 头像/个人页归属站点，默认 GitHub；CNB 镜像可设为 https://cnb.cool
profile_base=${CONTRIBUTOR_PROFILE_BASE:-https://github.com}
output_file=CONTRIBUTORS.md
start_marker='<!-- contributors:start -->'
end_marker='<!-- contributors:end -->'

# 统一昵称与 GitHub 账号。格式：邮箱|显示名|GitHub 用户名（第三列留空表示无公开账号）
aliases='1640053235@qq.com|JeffreyMing|JeffreyMing2004
1972138560@qq.com|sanyisi|sanyisi
codebuddy@cnb.cool|CodeBuddy|'

# 邮箱 -> "显示名<TAB>用户名"
declare -A name_of login_of
while IFS='|' read -r email show login; do
    [ -n "${email:-}" ] || continue
    name_of["$email"]=$show
    login_of["$email"]=${login:-}
done <<< "$aliases"

mapfile -t entries < <(git log --format='%ae%x09%an' | while IFS=$'\t' read -r email name; do
    [ -n "$email" ] || continue
    if [ -n "${name_of[$email]:-}" ]; then show=${name_of[$email]}; login=${login_of[$email]}; else show=$name; login=''; fi
    printf '%s\t%s\n' "$show" "$login"
done | sort -u)

declare -A logins
names=()
for entry in "${entries[@]}"; do
    show=${entry%%$'\t'*}
    login=${entry#*$'\t'}
    if [ -z "${logins[$show]+set}" ]; then
        names+=("$show")
        logins["$show"]=''
    fi
    if [ -z "${logins[$show]}" ] && [ -n "$login" ]; then
        logins["$show"]=$login
    fi
done

list=''
for show in "${names[@]}"; do
    login=${logins[$show]}
    if [ -n "$login" ]; then
        list+="- [$show]($profile_base/$login)"$'\n'
    else
        list+="- $show"$'\n'
    fi
done
count=${#names[@]}

block="$start_marker
共 $count 位贡献者：

${list%$'\n'}

该列表由 \`scripts/update-contributors.sh\` 从 git 历史生成，可按需重新运行。
$end_marker"

if grep -qF "$start_marker" README.md; then
    awk -v start="$start_marker" -v end="$end_marker" -v block="$block" '
        index($0, start) == 1 { print block; skip = 1; next }
        skip && index($0, end) == 1 { skip = 0; next }
        !skip { print }
    ' README.md > README.md.tmp
else
    awk -v block="$block" '
        !done && /^## 参考与致谢/ { print "## 贡献者\n"; print block; print ""; done = 1 }
        { print }
        END { if (!done) { print ""; print "## 贡献者"; print ""; print block } }
    ' README.md > README.md.tmp
fi
mv README.md.tmp README.md

{
    echo '# 贡献者'
    echo
    echo "感谢以下 $count 位贡献者对 NorthStar Minecraft Launcher 的代码贡献。"
    echo '本文件由 `scripts/update-contributors.sh` 从 git 历史生成，请勿手动编辑。'
    echo
    printf '%s' "$list"
} > "$output_file"

echo "已更新 README.md 与 $output_file（$count 位贡献者）"
