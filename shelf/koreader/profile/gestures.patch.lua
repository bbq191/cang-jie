-- gestures.lua 补丁：长按左上角 = 退出 KOReader（防睡眠卡死 #14348，两个上下文都设）。
-- 结构以设备快照为准（`shelf koreader pull` 看 gesture_reader / gesture_fm 的真实键名再改本文件）。
return {
    gesture_reader = { hold_top_left_corner = { exit = true } },
    gesture_fm     = { hold_top_left_corner = { exit = true } },
}
