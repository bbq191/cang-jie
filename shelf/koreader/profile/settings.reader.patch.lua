-- settings.reader.lua 补丁（deep-merge 进设备 settings.reader.lua）。键来源：阅读白皮书 §11.1b。
-- 语义：标量覆盖、表递归、"__DELETE__" 删键。footer 轮显项以设备快照为准（`shelf koreader pull` 后 diff 核对）。
return {
    -- 脚注回得去（2026-09-02 定案）
    footnote_link_in_popup = true,           -- 点脚注底部弹窗，不跳页
    link_prefer_footnote = true,             -- 中文书脚注常无 epub:type，放宽判定
    swipe_to_go_back = true,                 -- 单指左→右滑回上一位置
    larger_tap_area_to_follow_links = true,  -- 7.3″ 上标太小，放大命中区
    -- 刷新/屏闪
    wf_level = 1,                            -- 翻页 CONTENT 波形（3=全 FAST 会把抗锯齿二值化=锯齿）
    full_refresh_count = 16,
    avoid_flashing_ui = true,
    color_rendering = false,                 -- 全局关，彩书书内单独开
    -- 中文排版
    floating_punctuation = 1,                -- 悬挂标点（crengine 独有）
    -- 默认字体（项目中文字体，须已镜像进 koreader/fonts/）
    cre_font = "LXGW Neo ZhiSong Screen Full",
    -- 状态栏按小屏收紧
    footer = {
        reclaim_height = true,
        progress_style_thin = true,
        battery = false,
        time = false,
    },
}
