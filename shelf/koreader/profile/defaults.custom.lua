-- defaults.custom.lua 补丁：点击翻页区四周留死区（左右 6%、上下 10%），Move 握持贴边不误翻。
-- 缺省整屏都是翻页区（BACKWARD 左 1/4 全高 + FORWARD 右 3/4 全高）。值待 `shelf koreader pull` 核对。
return {
    DTAP_ZONE_BACKWARD = { x = 0.06, y = 0.10, w = 0.19, h = 0.80 },
    DTAP_ZONE_FORWARD  = { x = 0.25, y = 0.10, w = 0.69, h = 0.80 },
}
