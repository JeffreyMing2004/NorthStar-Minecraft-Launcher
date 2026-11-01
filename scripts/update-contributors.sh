#!/usr/bin/env bash
# 从 git 历史生成贡献者列表，并写入 README 的 contributors 区块与 CONTRIBUTORS.md。
# 用法：
#   bash scripts/update-contributors.sh          生成并写回文件
#   bash scripts/update-contributors.sh --check  只检查现有文件是否最新，过期时退出码 1
set -euo pipefail

check_only=false
for arg in "$@"; do
    case "$arg" in
        --check) check_only=true ;;
        -h|--help)
             sed -n '2,5p' "$0" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *)
            echo "未知参数: $arg" >&2
            exit 2
            ;;
    esac
done

root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$root"

# 统一换行与行尾空格，避免同一份名单因格式抖动被判为变更
normalize() {
    sed -e 's/[[:space:]]*$//' -e '/./,$!d' | sed -e :a -e '/^\n*$/{$d;N;ba' -e '}' || true
}

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

if $check_only; then
    work=$(mktemp -d)
    trap 'rm -rf "$work"' EXIT
    cp README.md "$work/README.md"
    [ -f "$output_file" ] && cp "$output_file" "$work/$output_file"
    readme=$work/README.md
    out=$work/$output_file
else
    readme=README.md
    out=$output_file
fi

if grep -qF "$start_marker" "$readme"; then
    awk -v start="$start_marker" -v end="$end_marker" -v block="$block" '
        index($0, start) == 1 { print block; skip = 1; next }
        skip && index($0, end) == 1 { skip = 0; next }
        !skip { print }
    ' "$readme" > "$readme.tmp"
else
    awk -v block="$block" '
        !done && /^## 参考与致谢/ { print "## 贡献者\n"; print block; print ""; done = 1 }
        { print }
        END { if (!done) { print ""; print "## 贡献者"; print ""; print block } }
    ' "$readme" > "$readme.tmp"
fi
mv "$readme.tmp" "$readme"

{
    echo '# 贡献者'
    echo
    echo "感谢以下 $count 位贡献者对 NorthStar Minecraft Launcher 的代码贡献。"
    echo '本文件由 `scripts/update-contributors.sh` 从 git 历史生成，请勿手动编辑。'
    echo
    printf '%s' "$list"
} > "$out"

if $check_only; then
    if diff -q <(normalize < README.md) <(normalize < "$readme") >/dev/null \
        && [ -f "$output_file" ] && diff -q <(normalize < "$output_file") <(normalize < "$out") >/dev/null; then
        echo "贡献者名单已是最新（$count 位贡献者）"
        exit 0
    fi
    echo "贡献者名单已过期：运行 bash scripts/update-contributors.sh 重新生成" >&2
    exit 1
fi

echo "已更新 README.md 与 $output_file（$count 位贡献者）"
