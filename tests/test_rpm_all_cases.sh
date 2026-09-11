#!/usr/bin/env bash
# =============================================================================
# pkq RPM 跨发行版全功能测试矩阵 v2
#   - 在原展示矩阵基础上增加自动断言（PASS/FAIL 计数）与末尾汇总
#   - 覆盖：info/list/owns/deps/rdeps/source/search/JSON/cache/Banner/BrokenPipe
#   - 用法: ./test_rpm_all_cases.sh
#     可选: PKGBIN=/tmp/pkq ./test_rpm_all_cases.sh
#           SHOW_LINES=8 ./test_rpm_all_cases.sh   (每用例展示行数)
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
if ! command -v rpm >/dev/null 2>&1; then
    echo "错误: 未找到 rpm 命令，本脚本仅适用于 RPM 系统-test"
    exit 1
fi

section() {
    echo ""
    echo "================================================================================"
    echo "  $1"
    echo "================================================================================"
}

# ---- 断言辅助 ---------------------------------------------------------------
# expect_contains <描述> <期望子串> <命令...>  : 运行命令, 展示输出前 N 行, 断言包含
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

# expect_not_contains <描述> <禁止子串> <命令...>
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

# display_only <描述> <命令...> : 仅展示，不参与计数
display_only() {
    local desc="$1"; shift
    echo ">>> $desc"
    "$@" 2>&1 | head -n "$SHOW" || true
    echo ""
}

# ---- 动态探测 ----------------------------------------------------------------
# 已安装测试包：优先 bash，兜底取第一个非 gpg-pubkey 的真实包
INSTALLED_PKG="bash"
if ! rpm -q bash >/dev/null 2>&1; then
    INSTALLED_PKG=$(rpm -qa --qf '%{NAME}\n' 2>/dev/null \
        | grep -v -E '^(gpg-pubkey|libX|kernel-)' | head -n 1)
fi
if [ -z "$INSTALLED_PKG" ]; then
    echo "错误: 未找到任何已安装的测试包"; exit 1
fi

# 仓库未安装包：候选列表中挑第一个本地未安装的
REPO_ONLY_PKG=""
for cand in htop lzop tree sl cowsay jq zip; do
    if ! rpm -q "$cand" >/dev/null 2>&1; then
        REPO_ONLY_PKG="$cand"; break
    fi
done
[ -z "$REPO_ONLY_PKG" ] && REPO_ONLY_PKG="htop"

FAKE_PKG="pkq-fake-xyz-888"

echo "================================================================================"
echo "  pkq RPM 跨发行版全功能测试矩阵 v2"
echo "  - 已安装测试包 : $INSTALLED_PKG"
echo "  - 仓库未安装包 : $REPO_ONLY_PKG"
echo "  - 虚构错包名   : $FAKE_PKG"
echo "================================================================================"

# -----------------------------------------------------------------------------
section "【测试 1】info 命令 (已安装/仓库未安装/虚构包)"
# -----------------------------------------------------------------------------
echo ">>> 1.1 [中文] info $INSTALLED_PKG 断言: 中文名称字段"
expect_contains "info 中文输出包含 名称字段" "名称" "$BIN" info "$INSTALLED_PKG"

echo -e "\n>>> 1.2 [英文] info $INSTALLED_PKG 断言: Name 字段"
expect_contains "info 英文输出包含 Name字段" "Name" env LANG=en_US.UTF-8 "$BIN" info "$INSTALLED_PKG"

echo -e "\n>>> 1.3 [原生对照] rpm -qi $INSTALLED_PKG"
display_only "rpm -qi 关键字段" rpm -qi "$INSTALLED_PKG"

echo ">>> 1.4 [中文] info $REPO_ONLY_PKG (仓库未安装) 断言: 安装命令引导"
expect_contains "info 未安装包给出 dnf 安装引导" "dnf install" "$BIN" info "$REPO_ONLY_PKG"

echo -e "\n>>> 1.5 [中文] info $FAKE_PKG 断言: 未找到报错"
expect_contains "info 虚构包报错" "未找到" "$BIN" info "$FAKE_PKG"

# -----------------------------------------------------------------------------
section "【测试 2】list 命令 (文件列表/过滤/安装引导)"
# -----------------------------------------------------------------------------
echo ">>> 2.1 list $INSTALLED_PKG 断言: 输出真实绝对路径"
expect_contains "list 输出绝对路径" "/" "$BIN" list "$INSTALLED_PKG"

echo -e "\n>>> 2.2 list $INSTALLED_PKG --all (全量展示)"
display_only "list --all" "$BIN" list "$INSTALLED_PKG" --all

echo -e "\n>>> 2.3 [原生对照] rpm -ql 前 10 行"
display_only "rpm -ql" rpm -ql "$INSTALLED_PKG"

echo ">>> 2.4 list $REPO_ONLY_PKG 断言: 未安装提示 + dnf 安装引导"
expect_contains "list 未安装提示" "本地未安装" "$BIN" list "$REPO_ONLY_PKG"
expect_contains "list 仓库命中引导" "安装命令" "$BIN" list "$REPO_ONLY_PKG"
expect_contains "list 引导为 dnf" "dnf install" "$BIN" list "$REPO_ONLY_PKG"

echo -e "\n>>> 2.5 list $FAKE_PKG 断言: 本地与仓库均不存在的统一报错"
expect_contains "list 虚构包报错" "未找到" "$BIN" list "$FAKE_PKG"

# -----------------------------------------------------------------------------
section "【测试 3】owns 文件归属与路径边界"
# -----------------------------------------------------------------------------
echo ">>> 3.1 真实二进制查询 断言: 返回包名"
if [ -f /usr/bin/bash ] || [ -f /bin/bash ]; then
    expect_contains "owns bash 返回包名" "bash" "$BIN" owns /usr/bin/bash
    display_only "原生对照 rpm -qf" rpm -qf /usr/bin/bash
else
    REAL_BIN=$(rpm -ql "$INSTALLED_PKG" | grep -E '^/(usr/)?(s?bin)/' | head -n 1)
    if [ -n "$REAL_BIN" ]; then
        expect_contains "owns $REAL_BIN 返回包名" "$INSTALLED_PKG" "$BIN" owns "$REAL_BIN"
        display_only "原生对照 rpm -qf" rpm -qf "$REAL_BIN"
    fi
fi

echo -e "\n>>> 3.2 通配符 Glob 查询 断言: 命中"
expect_contains "owns 通配符查询" "bash" "$BIN" owns "*/bin/bash"

echo -e "\n>>> 3.3 公共目录 /etc (多包共享识别, 展示)"
display_only "owns /etc" "$BIN" owns /etc

echo ">>> 3.4 不存在路径 断言: 中英文精准报错"
expect_contains "owns 不存在路径(中文)" "不存在" "$BIN" owns /opt/not_exist_file_999.so
expect_contains "owns 不存在路径(英文)" "does not exist" env LANG=en_US.UTF-8 "$BIN" owns /opt/not_exist_file_999.so

# -----------------------------------------------------------------------------
section "【测试 4】deps 依赖关系 (分段/版本约束/反查真实包名)"
# -----------------------------------------------------------------------------
echo ">>> 4.1 [中文] deps $INSTALLED_PKG 断言: 依赖分段头存在"
expect_contains "deps 中文分段头 依赖" "依赖" "$BIN" deps "$INSTALLED_PKG"

echo -e "\n>>> 4.2 [回归] deps 不得残留 soname 能力(如 xxx()(64bit))"
expect_not_contains "deps 无 .so 能力残留" "()(64bit)" "$BIN" deps "$INSTALLED_PKG"

echo -e "\n>>> 4.3 [英文] deps 断言: Depends 头"
expect_contains "deps 英文分段头" "Depends" env LANG=en_US.UTF-8 "$BIN" deps "$INSTALLED_PKG"

echo -e "\n>>> 4.4 [原生对照] rpm -qR 前 8 行"
display_only "rpm -qR" rpm -qR "$INSTALLED_PKG"

echo ">>> 4.5 deps $REPO_ONLY_PKG (仓库未安装包, 展示)"
display_only "deps 仓库包" "$BIN" deps "$REPO_ONLY_PKG"

# -----------------------------------------------------------------------------
section "【测试 5】rdeps 反向依赖 (统计一致性/状态标签)"
# -----------------------------------------------------------------------------
echo ">>> 5.1 rdeps $INSTALLED_PKG 断言: 列表数与底部汇总严格一致"
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

echo -e "\n>>> 5.2 rdeps --installed-only 断言: 不含 [未安装] 标签"
expect_not_contains "rdeps installed-only 无仓库标签" "[未安装]" "$BIN" rdeps "$INSTALLED_PKG" --installed-only

# -----------------------------------------------------------------------------
section "【测试 6】source 源码包互查"
# -----------------------------------------------------------------------------
echo ">>> 6.1 source $INSTALLED_PKG 断言: 输出二进制包列表"
expect_contains "source 输出二进制包段" "二进制包" "$BIN" source "$INSTALLED_PKG"

echo -e "\n>>> 6.2 source $REPO_ONLY_PKG (展示)"
display_only "source 仓库包" "$BIN" source "$REPO_ONLY_PKG"

echo ">>> 6.3 source $FAKE_PKG 断言: 未找到报错"
expect_contains "source 虚构包报错" "未找到" "$BIN" source "$FAKE_PKG"

# -----------------------------------------------------------------------------
section "【测试 7】search 搜索 (分段/Banner/BrokenPipe)"
# -----------------------------------------------------------------------------
echo ">>> 7.1 search $INSTALLED_PKG 断言: 元数据 Banner + 软件包分段头"
expect_contains "search 顶部元数据 Banner" "上次元数据过期检查" "$BIN" search "$INSTALLED_PKG"
expect_contains "search 软件包分段头" "==> 软件包" "$BIN" search "$INSTALLED_PKG"

echo -e "\n>>> 7.2 search $INSTALLED_PKG 断言: 文件路径匹配分段存在"
expect_contains "search 文件路径分段头" "==> 文件路径匹配" "$BIN" search "$INSTALLED_PKG"

echo -e "\n>>> 7.3 search zip --installed-only 断言: 不含 [未安装] 标签"
expect_not_contains "search installed-only 纯净" "[未安装]" "$BIN" search zip --installed-only

echo -e "\n>>> 7.4 search zip --all-files (展示)"
display_only "search --all-files" "$BIN" search zip --all-files

echo -e "\n>>> 7.4b search bash 主列表信噪比断言: 噪音包折叠"
expect_contains "search bash 可能相关段存在" "==> 可能相关" "$BIN" search bash
expect_not_contains "search bash 主列表无 bats 噪音" "  bats " "$BIN" search bash
expect_contains "search bash 可能相关段存在" "可能相关" "$BIN" search bash

echo -e "\n>>> 7.5 BrokenPipe 断言: 管道截断不 panic"
ERR_TMP=$(mktemp)
"$BIN" search zip 2>"$ERR_TMP" | head -n 3 >/dev/null
if grep -q "panic" "$ERR_TMP"; then
    echo "  [FAIL] BrokenPipe 出现 panic:"; head -n 5 "$ERR_TMP"; FAIL=$((FAIL+1))
else
    echo "  [PASS] BrokenPipe 正常退出"; PASS=$((PASS+1))
fi
rm -f "$ERR_TMP"

# -----------------------------------------------------------------------------
section "【测试 8】JSON 结构化输出校验"
# -----------------------------------------------------------------------------
if command -v python3 >/dev/null 2>&1; then
    echo ">>> 8.1 info JSON 有效性"
    JSON_TMP=$(mktemp)
    "$BIN" -o json info "$INSTALLED_PKG" >"$JSON_TMP" 2>/dev/null
    if python3 -m json.tool "$JSON_TMP" >/dev/null 2>&1; then
        echo "  [PASS] info JSON 合法"; PASS=$((PASS+1))
    else
        echo "  [FAIL] info JSON 非法"; FAIL=$((FAIL+1))
    fi
    head -n "$SHOW" "$JSON_TMP"

    echo -e "\n>>> 8.2 deps JSON 有效性"
    "$BIN" -o json deps "$INSTALLED_PKG" >"$JSON_TMP" 2>/dev/null
    if python3 -m json.tool "$JSON_TMP" >/dev/null 2>&1; then
        echo "  [PASS] deps JSON 合法"; PASS=$((PASS+1))
    else
        echo "  [FAIL] deps JSON 非法"; FAIL=$((FAIL+1))
    fi

    echo -e "\n>>> 8.3 owns JSON 有效性 (取 rpm -ql 首个真实文件)"
    REAL_FILE=$(rpm -ql "$INSTALLED_PKG" | head -n 1)
    "$BIN" -o json owns "$REAL_FILE" >"$JSON_TMP" 2>/dev/null
    if python3 -m json.tool "$JSON_TMP" >/dev/null 2>&1; then
        echo "  [PASS] owns JSON 合法"; PASS=$((PASS+1))
    else
        echo "  [FAIL] owns JSON 非法"; FAIL=$((FAIL+1))
    fi
    rm -f "$JSON_TMP"
else
    echo "(跳过: 未安装 python3)"
fi

# -----------------------------------------------------------------------------
section "【测试 9】cache 缓存管理子命令 (P1-4)"
# -----------------------------------------------------------------------------
echo ">>> 9.1 cache status 断言: 输出缓存目录与总计"
expect_contains "cache status 缓存目录标签" "缓存目录" "$BIN" cache status
expect_contains "cache status 总计标签" "总计" "$BIN" cache status

echo -e "\n>>> 9.2 cache clean contents (无 --yes) 断言: 提示确认且不删除"
expect_contains "cache clean 需确认" "--yes" "$BIN" cache clean contents

echo -e "\n>>> 9.3 cache clean bogus-target 断言: 未知目标报错"
expect_contains "cache clean 非法目标报错" "未知清理目标" "$BIN" cache clean bogus-target

# -----------------------------------------------------------------------------
section "测试汇总"
# -----------------------------------------------------------------------------
echo "  PASS: $PASS"
echo "  FAIL: $FAIL"
echo "================================================================================"
[ "$FAIL" -eq 0 ] || exit 1
exit 0
