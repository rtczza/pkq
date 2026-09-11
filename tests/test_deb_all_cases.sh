#!/usr/bin/env bash
# =============================================================================
# pkq DEB 全功能测试矩阵 v2（与 RPM 版对齐）
#   - 自动断言（PASS/FAIL 计数）+ 末尾汇总
#   - 用法: ./test_deb_all_cases.sh
#     可选: PKGBIN=./target/release/pkq  SHOW_LINES=8
# =============================================================================
set -u

# 固定中文输出：断言默认匹配中文文案；英文用例用 `env LANG=en_US.UTF-8` 单独覆盖
# （GitHub Actions runner 默认 LANG=C.UTF-8，会导致中文断言全部失败）
export LANG=zh_CN.UTF-8
unset LC_ALL LC_MESSAGES

BIN="${PKGBIN:-./target/release/pkq}"
SHOW="${SHOW_LINES:-12}"
PASS=0
FAIL=0

if [ ! -x "$BIN" ]; then
    echo "错误: 未找到可执行文件 $BIN，请先执行 cargo build --release"
    exit 1
fi
if ! command -v dpkg >/dev/null 2>&1; then
    echo "错误: 未找到 dpkg 命令，本脚本仅适用于 DEB 系统"
    exit 1
fi

section() {
    echo ""
    echo "================================================================================"
    echo "  $1"
    echo "================================================================================"
}

expect_contains() {
    local desc="$1" needle="$2"; shift 2
    local out rc
    out=$("$@" 2>&1); rc=$?
    echo "$out" | head -n "$SHOW"
    if echo "$out" | grep -qF -- "$needle"; then
        echo "  [PASS] $desc"; PASS=$((PASS+1))
    else
        echo "  [FAIL] $desc —— 未找到: $needle (exit=$rc)"; FAIL=$((FAIL+1))
    fi
    LAST_OUT="$out"
}

expect_not_contains() {
    local desc="$1" needle="$2"; shift 2
    local out
    out=$("$@" 2>&1)
    echo "$out" | head -n "$SHOW"
    if echo "$out" | grep -qF -- "$needle"; then
        echo "  [FAIL] $desc —— 不应出现: $needle"; FAIL=$((FAIL+1))
    else
        echo "  [PASS] $desc"; PASS=$((PASS+1))
    fi
    LAST_OUT="$out"
}

display_only() {
    local desc="$1"; shift
    echo ">>> $desc"
    "$@" 2>&1 | head -n "$SHOW" || true
    echo ""
}

# ---- 动态探测 ----------------------------------------------------------------
INSTALLED_PKG="bash"
if ! dpkg -s bash >/dev/null 2>&1; then
    INSTALLED_PKG=$(dpkg-query -W -f='${Package}\n' 2>/dev/null | grep -v -E 'lib' | head -n 1)
fi
[ -z "$INSTALLED_PKG" ] && INSTALLED_PKG="bash"

REPO_ONLY_PKG=""
for cand in htop lzop tree sl cowsay jq; do
    if ! dpkg -s "$cand" >/dev/null 2>&1; then
        REPO_ONLY_PKG="$cand"; break
    fi
done
[ -z "$REPO_ONLY_PKG" ] && REPO_ONLY_PKG="htop"

FAKE_PKG="pkq-fake-xyz-888"

echo "================================================================================"
echo "  pkq DEB 全功能测试矩阵 v2"
echo "  - 已安装测试包 : $INSTALLED_PKG"
echo "  - 仓库未安装包 : $REPO_ONLY_PKG"
echo "  - 虚构错包名   : $FAKE_PKG"
echo "================================================================================"

# -----------------------------------------------------------------------------
section "【测试 1】info 命令"
# -----------------------------------------------------------------------------
echo ">>> 1.1 [中文] info $INSTALLED_PKG"
expect_contains "info 中文名称字段" "名称" "$BIN" info "$INSTALLED_PKG"

echo -e "\n>>> 1.2 [英文] info $INSTALLED_PKG"
expect_contains "info 英文名称字段" "Name" env LANG=en_US.UTF-8 "$BIN" info "$INSTALLED_PKG"

echo -e "\n>>> 1.3 [原生对照] dpkg -s $INSTALLED_PKG"
display_only "dpkg -s" dpkg -s "$INSTALLED_PKG"

echo ">>> 1.4 [中文] info $REPO_ONLY_PKG 断言: apt-get 安装引导"
expect_contains "info 未安装包 apt 引导" "apt-get install" "$BIN" info "$REPO_ONLY_PKG"

echo -e "\n>>> 1.5 info $FAKE_PKG 断言: 未找到报错"
expect_contains "info 虚构包报错" "未找到" "$BIN" info "$FAKE_PKG"

# -----------------------------------------------------------------------------
section "【测试 2】list 命令"
# -----------------------------------------------------------------------------
echo ">>> 2.1 list $INSTALLED_PKG 断言: 输出绝对路径"
expect_contains "list 输出绝对路径" "/" "$BIN" list "$INSTALLED_PKG"

echo -e "\n>>> 2.2 list --all (展示)"
display_only "list --all" "$BIN" list "$INSTALLED_PKG" --all

echo -e "\n>>> 2.3 [原生对照] dpkg -L 前 10 行"
display_only "dpkg -L" dpkg -L "$INSTALLED_PKG"

echo ">>> 2.4 list $REPO_ONLY_PKG 断言: 未安装提示 + apt-get 引导"
expect_contains "list 未安装提示" "本地未安装" "$BIN" list "$REPO_ONLY_PKG"
expect_contains "list 仓库命中引导" "apt-get install" "$BIN" list "$REPO_ONLY_PKG"

echo -e "\n>>> 2.5 list $FAKE_PKG 断言: 统一报错"
expect_contains "list 虚构包报错" "未找到" "$BIN" list "$FAKE_PKG"

# -----------------------------------------------------------------------------
section "【测试 3】owns 文件归属"
# -----------------------------------------------------------------------------
echo ">>> 3.1 真实文件查询 断言: 返回包名"
expect_contains "owns bash 返回包名" "bash" "$BIN" owns /bin/bash
display_only "原生对照 dpkg -S" dpkg -S /bin/bash

echo -e "\n>>> 3.2 通配符查询 断言: 命中"
expect_contains "owns 通配符查询" "bash" "$BIN" owns "*/bin/bash"

echo -e "\n>>> 3.3 公共目录 /etc (展示)"
display_only "owns /etc" "$BIN" owns /etc
expect_contains "owns /etc 截断提示引导 --all" "使用 --all 查看全部" "$BIN" owns /etc
expect_not_contains "owns /etc --all 无截断提示" "仅显示前" "$BIN" owns /etc --all

echo ">>> 3.4 不存在路径 断言: 中英文报错"
expect_contains "owns 不存在路径(中文)" "不存在" "$BIN" owns /opt/not_exist_file_999.so
expect_contains "owns 不存在路径(英文)" "does not exist" env LANG=en_US.UTF-8 "$BIN" owns /opt/not_exist_file_999.so

# -----------------------------------------------------------------------------
section "【测试 4】deps 依赖关系 (分段完整性)"
# -----------------------------------------------------------------------------
echo ">>> 4.1 [中文] deps bash 断言: 多分段（依赖/推荐/建议/冲突/替换 至少含 依赖）"
expect_contains "deps 分段头 依赖" "依赖" "$BIN" deps bash

echo -e "\n>>> 4.2 deps bash 断言: 替换 段存在（bash 替换 bash-completion/doc）"
expect_contains "deps 分段头 替换" "替换" "$BIN" deps bash

echo -e "\n>>> 4.3 [英文] deps bash 断言: Depends 头"
expect_contains "deps 英文分段头" "Depends" env LANG=en_US.UTF-8 "$BIN" deps bash

echo -e "\n>>> 4.4 [原生对照] dpkg -s bash Depends 字段"
display_only "dpkg -s Depends" dpkg -s bash

# -----------------------------------------------------------------------------
section "【测试 5】rdeps 反向依赖 (统计一致性)"
# -----------------------------------------------------------------------------
echo ">>> 5.1 rdeps $INSTALLED_PKG 断言: 列表数与底部汇总一致"
RDEPS_OUT=$("$BIN" rdeps "$INSTALLED_PKG" --all 2>&1)
echo "$RDEPS_OUT" | head -n "$SHOW"
INST_LINES=$(echo "$RDEPS_OUT" | grep -cF '[已安装]' || true)
REPO_LINES=$(echo "$RDEPS_OUT" | grep -cF '[未安装]' || true)
TOTAL_LINES=$((INST_LINES + REPO_LINES))
SUMMARY_LINE=$(echo "$RDEPS_OUT" | tail -n 1)
echo "  列表统计: 已安装=$INST_LINES 未安装=$REPO_LINES 合计=$TOTAL_LINES"
echo "  汇总行  : $SUMMARY_LINE"
if echo "$SUMMARY_LINE" | grep -qF "共 ${TOTAL_LINES} 个反向依赖包" \
   && echo "$SUMMARY_LINE" | grep -qF "已安装: ${INST_LINES}" \
   && echo "$SUMMARY_LINE" | grep -qF "仓库: ${REPO_LINES}"; then
    echo "  [PASS] rdeps 统计与列表一致"; PASS=$((PASS+1))
else
    echo "  [FAIL] rdeps 统计与列表不一致"; FAIL=$((FAIL+1))
fi

echo -e "\n>>> 5.2 rdeps --installed-only 断言: 不含 [未安装]"
expect_not_contains "rdeps installed-only 纯净" "[未安装]" "$BIN" rdeps "$INSTALLED_PKG" --installed-only

echo -e "\n>>> 5.3 rdeps 默认限流断言: 提示使用 --all 查看全部"
expect_contains "rdeps 截断提示引导 --all" "使用 --all 查看全部" "$BIN" rdeps unzip

echo -e "\n>>> 5.4 rdeps --all 断言: 输出不含限流提示"
expect_not_contains "rdeps --all 无截断提示" "仅显示前" "$BIN" rdeps unzip --all

# -----------------------------------------------------------------------------
section "【测试 6】source 源码包互查"
# -----------------------------------------------------------------------------
echo ">>> 6.1 source $INSTALLED_PKG 断言: 二进制包列表"
expect_contains "source 输出二进制包段" "二进制包" "$BIN" source "$INSTALLED_PKG"

echo -e "\n>>> 6.2 source $FAKE_PKG 断言: 未找到报错"
expect_contains "source 虚构包报错" "未找到" "$BIN" source "$FAKE_PKG"

# -----------------------------------------------------------------------------
section "【测试 7】search 搜索"
# -----------------------------------------------------------------------------
echo ">>> 7.1 search $INSTALLED_PKG 断言: Banner + 软件包分段"
expect_contains "search 顶部元数据 Banner" "上次元数据过期检查" "$BIN" search "$INSTALLED_PKG"
expect_contains "search 软件包分段头" "==> 软件包" "$BIN" search "$INSTALLED_PKG"

echo -e "\n>>> 7.2 search $INSTALLED_PKG 断言: 文件路径匹配分段"
expect_contains "search 文件路径分段头" "==> 文件路径匹配" "$BIN" search "$INSTALLED_PKG"

echo -e "\n>>> 7.3 search zip --installed-only 断言: 不含 [未安装]"
expect_not_contains "search installed-only 纯净" "[未安装]" "$BIN" search zip --installed-only

echo -e "\n>>> 7.4 search unzip 断言: 无虚拟 Provides 符号混入文件段"
expect_not_contains "search 无 perl( 污染" "perl(" "$BIN" search unzip

echo -e "\n>>> 7.4b search bash 主列表信噪比断言: 噪音包折叠"
expect_contains "search bash 可能相关段存在" "==> 可能相关" "$BIN" search bash
expect_not_contains "search bash 主列表无 bats 噪音" "  bats " "$BIN" search bash
expect_contains "search bash 可能相关段存在" "可能相关" "$BIN" search bash

echo -e "\n>>> 7.5 BrokenPipe 断言"
ERR_TMP=$(mktemp)
"$BIN" search zip 2>"$ERR_TMP" | head -n 3 >/dev/null
if grep -q "panic" "$ERR_TMP"; then
    echo "  [FAIL] BrokenPipe 出现 panic"; FAIL=$((FAIL+1))
else
    echo "  [PASS] BrokenPipe 正常退出"; PASS=$((PASS+1))
fi
rm -f "$ERR_TMP"

# -----------------------------------------------------------------------------
section "【测试 8】JSON 结构化输出"
# -----------------------------------------------------------------------------
if command -v python3 >/dev/null 2>&1; then
    JSON_TMP=$(mktemp)
    echo ">>> 8.1 info JSON"
    "$BIN" -o json info "$INSTALLED_PKG" >"$JSON_TMP" 2>/dev/null
    if python3 -m json.tool "$JSON_TMP" >/dev/null 2>&1; then
        echo "  [PASS] info JSON 合法"; PASS=$((PASS+1))
    else
        echo "  [FAIL] info JSON 非法"; FAIL=$((FAIL+1))
    fi
    head -n "$SHOW" "$JSON_TMP"

    echo -e "\n>>> 8.2 deps JSON"
    "$BIN" -o json deps "$INSTALLED_PKG" >"$JSON_TMP" 2>/dev/null
    if python3 -m json.tool "$JSON_TMP" >/dev/null 2>&1; then
        echo "  [PASS] deps JSON 合法"; PASS=$((PASS+1))
    else
        echo "  [FAIL] deps JSON 非法"; FAIL=$((FAIL+1))
    fi
    rm -f "$JSON_TMP"
else
    echo "(跳过: 未安装 python3)"
fi

# -----------------------------------------------------------------------------
section "【测试 9】cache 缓存管理子命令"
# -----------------------------------------------------------------------------
echo ">>> 9.1 cache status"
expect_contains "cache status 缓存目录标签" "缓存目录" "$BIN" cache status
expect_contains "cache status 总计标签" "总计" "$BIN" cache status

echo -e "\n>>> 9.2 cache clean 无 --yes 断言: 提示确认"
expect_contains "cache clean 需确认" "--yes" "$BIN" cache clean contents

echo -e "\n>>> 9.3 cache clean 非法目标 断言: 报错"
expect_contains "cache clean 非法目标报错" "未知清理目标" "$BIN" cache clean bogus-target

# -----------------------------------------------------------------------------
section "测试汇总"
# -----------------------------------------------------------------------------
echo "  PASS: $PASS"
echo "  FAIL: $FAIL"
echo "================================================================================"
[ "$FAIL" -eq 0 ] || exit 1
exit 0
