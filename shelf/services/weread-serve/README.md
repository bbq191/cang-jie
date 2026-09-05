# weread-serve（预留槽位）

微信读书**内容源**服务的入口：扫码登录 → 选书 → 下成 EPUB → **原样落母版库**（`shelf_core::paths::staging_dir()`），去向由用户在网页
「传书 → 母版库」选（书架白皮书 §03r 决策 3、Phase D）。复用 `reading/device-rs` 的下书栈（login / fetch / pipeline，单向下书），
不做内嵌浏览器在线读（`shelf/weread-web/` 已存档）。注册名 `weread-serve`、loopback 端口 `8794`、网关 URL 段 `/api/weread/*`
（`manage::MODULES` 里 `installable:false`，接通后翻开）。形态待定：独立服务或先由 book-serve 代理。
不是也不替代旧项目的 `wr-serve`（微读 EPUB 下书线原样保留兜底，书架不引用它）。
