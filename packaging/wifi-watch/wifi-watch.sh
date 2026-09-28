#!/bin/sh
# wifi-watch —— 常驻看护 wlan0 载波（装为 systemd 服务 wifi-watch.service，/usr 单元，OTA 后重装）。
# 定位（2026-09-06）：连上 60 s 必掉的真凶是 cfg80211 regdomain 宽限——精简 regulatory.db 的 CN 不含 5150–5350，路由 5G 信道 36
# 被判非法而断开（书架白皮书 §03w），根治是锁 2.4G 或路由改 149+ 信道。本脚本只做兜底：slumber 醒来后 wlan0 偶发假死
# （NetworkManager 仍标 connected、永不自愈）时 `nmcli con up`。⚠ `nmcli con up` 对已激活连接会先断再连，所以判据必须是真 NO-CARRIER。
# 做法：每 INTERVAL 秒看一次；wlan0 存在、rfkill 未软锁、NM 有 wifi 连接、却连续 STRIKES 次 NO-CARRIER → `nmcli con up`。
# 只在"NM 以为连着但链路死了"时动手；用户关 WiFi（rfkill/NM 断开）不干预。日志 journalctl -u wifi-watch。
# 固化（用户 2026-09-06 拍板）：给当前活动的 WiFi 连接补 `powersave=$POWERSAVE`（缺省 2=关），必要时补
# `802-11-wireless.band=$BAND`（缺省 bg=2.4G），并重新激活一次——新 SSID / 在设置里重连后自动生效。BAND= / POWERSAVE= 置空即不管。
# ⚠ 2026-09-28 改：频段**只在 AP 真落在设备不许用的 5G 段（BAD_LO–BAD_HI MHz，缺省 5150–5350，精简 regulatory.db 的 CN
# 没有这段）时才锁**。旧版对每个连接都无条件锁 2.4G：手机热点「📱」在 5745 MHz（信道 149，CN 合法）被锁后重连报
# ssid-not-found，09-27 两次把 WiFi 弄断、退回别的网络，而且该连接从此再也连不上。另外改完重新激活失败时，把这次改的
# 设置**回滚**并再激活一次，日志记 nmcli 的真实错误行（旧版只记最后一行 "Hint: use journalctl …"，看不出原因）。
# 上网探测（2026-09-28）：连上新网络后的下一个周期探一次 PROBE_URL（缺省小米 generate_204），结果写 STATE_FILE 给网关页头
# 横幅用——酒店这类要网页登录的 WiFi 上，xochitl 每约 50 秒取一次云端令牌、每次卡满 30 秒超时，设备因此整段不睡（09-27
# 真机：30 分钟 100% 醒着，约 17%/h），而 reMarkable 上没法完成网页登录。只在"新连上"和"上次不通时每 RECHECK 周期"探，
# 链路正常且上次通了就不再探；关 WiFi 时删掉状态文件。PROBE_URL= 置空即不探。
# 覆盖配置：/home/root/.config/wifi-watch.conf（shell 片段，可设 BAND / POWERSAVE / BAD_LO / BAD_HI / INTERVAL；路径可用 WIFI_WATCH_CONF 改）。
#
IFACE=${IFACE:-wlan0}
INTERVAL=${INTERVAL:-15}
STRIKES=${STRIKES:-2}
RECHECK=${RECHECK:-40}   # 快路径下每 RECHECK 个周期兜底复查一次固化（40×15s=10 分钟）
BAND=${BAND-bg}
POWERSAVE=${POWERSAVE-2}
BAD_LO=${BAD_LO:-5150}
BAD_HI=${BAD_HI:-5350}
SYSFS=${SYSFS:-/sys/class/net}
PROBE_URL=${PROBE_URL-http://connect.rom.miui.com/generate_204}
STATE_FILE=${STATE_FILE:-/home/root/.local/state/shelf/wifi-connectivity.json}
TICKS=${TICKS:-0}        # 测试用：跑满这么多个周期就退出；0=永远
WIFI_WATCH_CONF=${WIFI_WATCH_CONF:-/home/root/.config/wifi-watch.conf}
# shellcheck disable=SC1090  # 用户自己的覆盖配置，路径运行时才知道
[ -r "$WIFI_WATCH_CONF" ] && . "$WIFI_WATCH_CONF"
strikes=0
probe_due=""
net_state=""
enforced=""
prev=""
tick=0

# 当前 wlan0 的活动连接名（要 fork nmcli，只在需要时调用）
active_con() {
    nmcli -t -f DEVICE,NAME con show --active 2>/dev/null | grep "^$IFACE:" | head -n 1 | cut -d: -f2-
}

# 当前连着的 AP 频率（MHz 整数；没连上 / iw 读不到则空）
cur_freq() {
    iw dev "$IFACE" link 2>/dev/null | sed -n 's/^[[:space:]]*freq:[[:space:]]*\([0-9][0-9]*\).*/\1/p'
}

# 重新激活；失败返回非 0，并把 nmcli 的第一行真实错误放进 $err
reactivate() {
    out="$(nmcli -w 45 con up "$1" 2>&1)"; rc=$?
    err="$(printf '%s\n' "$out" | grep -i -m 1 'error')"
    [ "$rc" -eq 0 ] && [ -z "$err" ]
}

# 固化频段/省电（每个连接只查一次，避免反复打 nmcli）
enforce() {
    con="$1"
    [ "$enforced" != "$con" ] || return 0
    enforced="$con"
    changed=""; set_band=""; set_ps=""
    if [ -n "$BAND" ]; then
        freq="$(cur_freq)"
        if [ -n "$freq" ] && [ "$freq" -ge "$BAD_LO" ] && [ "$freq" -le "$BAD_HI" ]; then
            old_band="$(nmcli -g 802-11-wireless.band con show "$con" 2>/dev/null)"
            if [ "$old_band" != "$BAND" ] && nmcli con modify "$con" 802-11-wireless.band "$BAND" 2>/dev/null; then
                set_band=1; changed="band=$BAND(AP ${freq}MHz)"
            fi
        fi
    fi
    if [ -n "$POWERSAVE" ]; then
        # ⚠ `nmcli -g` 回的是文字（default/ignore/disable/enable 对应 0–3），不是数字；两种写法都认，免得每次启动白改一遍并断线重连。
        old_ps="$(nmcli -g 802-11-wireless.powersave con show "$con" 2>/dev/null)"
        case "$POWERSAVE:$old_ps" in
            "$POWERSAVE:$POWERSAVE"|"$POWERSAVE:$POWERSAVE "*|0:default|1:ignore|2:disable|3:enable) ;;
            *) if nmcli con modify "$con" 802-11-wireless.powersave "$POWERSAVE" 2>/dev/null; then
                   set_ps=1; changed="$changed powersave=$POWERSAVE"
               fi ;;
        esac
    fi
    [ -n "$changed" ] || return 0
    if reactivate "$con"; then
        echo "固化 '$con' $changed → 已重新激活"
        return 0
    fi
    echo "固化 '$con' $changed → 重新激活失败：${err:-nmcli 退出码 $rc}；回滚"
    [ -z "$set_band" ] || nmcli con modify "$con" 802-11-wireless.band "$old_band" 2>/dev/null
    [ -z "$set_ps" ] || nmcli con modify "$con" 802-11-wireless.powersave "${old_ps:-default}" 2>/dev/null
    if reactivate "$con"; then echo "回滚 '$con' 后已重新激活"; else echo "回滚 '$con' 后仍未激活：${err:-nmcli 退出码 $rc}"; fi
}

# 探一次外网：204=通（ok）；拿到别的状态码=被拦去登录页（portal）；连不上/超时=没网（none）。写状态文件（原子改名）。
probe() {
    [ -n "$PROBE_URL" ] || return 0
    code="$(wget -q -S -T 8 -O /dev/null "$PROBE_URL" 2>&1 | sed -n 's/^[[:space:]]*HTTP\/[0-9.]*[[:space:]]*\([0-9][0-9]*\).*/\1/p' | sed -n 1p)"
    case "$code" in
        204) st=ok ;;
        "") st=none ;;
        *) st=portal ;;
    esac
    [ "$st" = "$net_state" ] && [ -e "$STATE_FILE" ] && return 0
    [ "$st" = ok ] || echo "'$1' 上不了外网：$st（探测 $PROBE_URL 得到 ${code:-无响应}）"
    net_state="$st"
    mkdir -p "$(dirname "$STATE_FILE")" 2>/dev/null
    esc="$(printf '%s' "$1" | sed 's/\\/\\\\/g; s/"/\\"/g')"
    printf '{"ssid":"%s","state":"%s","code":"%s","at":%s}\n' "$esc" "$st" "$code" "$(date +%s)" > "$STATE_FILE.tmp" && mv -f "$STATE_FILE.tmp" "$STATE_FILE"
}

tick_all=0
while :; do
    if [ "$TICKS" -gt 0 ]; then tick_all=$((tick_all + 1)); [ "$tick_all" -le "$TICKS" ] || exit 0; fi
    sleep "$INTERVAL"
    [ -e "$SYSFS/$IFACE" ] || continue
    # carrier：1=有载波，0=NO-CARRIER，读失败（接口 admin down）=空。read 是 shell 内建，不 fork。
    carrier=""
    { read -r carrier < "$SYSFS/$IFACE/carrier"; } 2>/dev/null
    # 接口 admin down（用户关了 WiFi）：不可能有 NO-CARRIER，无事可做，连 rfkill/nmcli 都不必问。
    if [ -z "$carrier" ]; then
        strikes=0; prev=""; tick=0; probe_due=""; net_state=""
        [ ! -e "$STATE_FILE" ] || rm -f "$STATE_FILE"
        continue
    fi
    if [ "$carrier" = 1 ]; then
        strikes=0
        tick=$((tick + 1))
        # 新连上时 DHCP/DNS 可能还没好：本周期只记下"待探"，下个周期再探
        if [ -n "$probe_due" ]; then
            probe_due=""
            con="$(active_con)"
            [ -z "$con" ] || probe "$con"
        fi
        if [ "$prev" != 1 ] || [ "$tick" -ge "$RECHECK" ]; then
            [ "$prev" = 1 ] && [ "$net_state" = ok ] || probe_due=1
            tick=0
            if ! rfkill list wifi 2>/dev/null | grep -q "Soft blocked: yes"; then
                con="$(active_con)"
                [ -n "$con" ] && enforce "$con"
            fi
        fi
        prev=1
        continue
    fi
    prev="$carrier"
    tick=0
    # ── 慢路径（链路疑似死了 / 接口没起）：与原版逻辑一致 ──
    if rfkill list wifi 2>/dev/null | grep -q "Soft blocked: yes"; then strikes=0; continue; fi
    con="$(active_con)"
    [ -n "$con" ] || { strikes=0; continue; }
    enforce "$con"
    if [ "$carrier" = 0 ]; then
        strikes=$((strikes + 1))
        if [ "$strikes" -ge "$STRIKES" ]; then
            out="$(nmcli con up "$con" 2>&1 | tail -n 1)"
            echo "$IFACE NO-CARRIER x$strikes → nmcli up '$con': $out"
            strikes=0
        fi
    else
        strikes=0
    fi
done
