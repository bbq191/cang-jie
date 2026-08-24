# HanaMinB（花園明朝 B / Hanazono Mincho B）

- 用途：CJK **扩展 B 兜底字体**——主字体霞鹜新致宋（LXGW Neo ZhiSong Screen Full）只覆盖到 BMP + CJK 扩展 A，而词典雾凇 41448 大字表含约 1.3 万个 CJK 扩展 B（U+20000 起，SIP 辅助平面）生僻字（如 `hang` 尾部 𠡊）。这些字打到候选框/笔记本时主字体缺字→豆腐块，靠 HanaMinB 在 fontconfig fallback 链里字形级回退显示。花園明朝拆成 A/B 两个文件正是因为单个 TTF 上限 65535 字形装不下全部汉字：A=BMP、B=扩展 B 及以上辅助平面。
- 来源：GlyphWiki Project（`kamichi@fonts.jp`，`http://glyphwiki.org/`）的 `HanaMinB.ttf`。本仓库这份 name 表：Full name `HanaMinB Regular`，Version `2017-09-04; (gw1796547)`，Copyright `Created by GlyphWiki.`（`otfinfo -i` / fontTools name 表读出），单文件约 30MB。
- 许可证：**SIL Open Font License, Version 1.1**（双授权，另可选用更宽松的 Hanazono Font License）——**联网实测确认**。版权持有者 "GlyphWiki Project (kamichi@fonts.jp)"，Reserved Font Names 含 "Hanazono Font" / "HanaMinA" / "HanaMinB" / "花園フォント" / "花園明朝A" / "花園明朝B"。OFL 1.1 允许免费个人/商用、可嵌入设备、可随软件打包再分发，唯一限制是不得单独售卖字体本身、须保留版权与许可声明、且派生字体不得沿用 Reserved Font Name。出处：
  - 官方主页 GlyphWiki 花園フォント：<https://glyphwiki.org/wiki/Group:%E8%8A%B1%E5%9C%92%E3%83%95%E3%82%A9%E3%83%B3%E3%83%88>
  - 许可证正文（MIT 镜像的 `LICENSE.HanaMin`，OFL 1.1 全文 + 上述 Reserved Font Names）：<http://web.mit.edu/xavid/Public/bazki/lib/bazki/LaTeX/fonts/LICENSE.HanaMin>
  - GitHub `cjkvi/HanaMinAFDKO`（花園明朝 AFDKO 构建仓库，README 标注 OFL）：<https://github.com/cjkvi/HanaMinAFDKO>
- 本项目用法：**原样、不改名、独立 `.ttf` 放 /home 运行时 mmap，不编译进 `.so`、不修改**——符合 OFL 1.1 的原样再分发条款（保留声明、不单独售卖、不改 Reserved Font Name）。仅作 fontconfig fallback 末端，不作主字体。真机 2026-08-23 验证：扩展 B 生僻字回退 HanaMinB 显示、不再方块。
- 部署：`.ttf` → `/home/root/.local/share/fonts/`；`deploy/fontconfig-cangjie.conf` 把 `HanaMinB` 作为 `sans-serif` 及 zh-* 的 weak 兜底（排在 `LXGW Neo ZhiSong Screen Full` 之后），纯 /home、无 verity 风险。字体覆盖诊断/踩坑。
