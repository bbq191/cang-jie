//! shelf-core —— 书架（shelf/）各服务的共享底座。**不引用旧项目任何 crate**（device-core /
//! weread-device），需要的能力按"剥离移植"独立实现（`xochitl`、`fswatch` 两模块注明来源）。
//!
//! 模块职责（端口/适配器分层：领域模块不碰 HTTP 类型，`http` 是唯一适配层）：
//! - `paths`    XDG 基目录规范的单一路径表（三语言共用同一张表，见 shelf/docs）。
//! - `registry` 服务自注册/发现（`$XDG_RUNTIME_DIR/shelf/services/<name>.json`），网关据此拔插。
//! - `multipart` 流式 multipart/form-data 解析（多文件、落盘不进内存，设备 MemoryMax 友好）。
//! - `asset`    资产仓库抽象（Repository）+ 上传流程模板（Template Method），字体/壁纸共用。
//! - `http`     tiny_http 适配：路由、JSON 回执、查询串。
//! - `xochitl`  原生书库免重启注入（`/upload` GET-then-upload 归档、防复制风暴判据）。
//! - `fswatch`  inotify 防抖目录监听（spool 追平）。
//! - `service`  服务启动模板：解析参数→建目录→注册→起服务器。
//! - `config`   服务配置/状态 JSON 读写模板（load/seed/save），收编各服务的 config 复制。
//! - `fs`       原子写（tmp→rename）+ unix 权限，config/registry/fonts.json 共用。
//! - `tls`      私有 CA + 叶证书生成/加载（网关 HTTPS；装一次 CA 免提示）。
//! - `auth`     密码哈希（salted SHA-256）、Basic/Cookie 解析、内存会话表。
//! - `netinfo`  本机 IPv4 表（证书 SAN、mDNS 选址）。
//! - `mdns`     极简 mDNS 应答器（`shelf.local` 伪域名）。
pub mod asset;
pub mod auth;
pub mod config;
pub mod fs;
pub mod fswatch;
pub mod http;
pub mod mdns;
pub mod netinfo;
pub mod multipart;
pub mod paths;
pub mod registry;
pub mod service;
pub mod tls;
pub mod ttf;
pub mod xochitl;
