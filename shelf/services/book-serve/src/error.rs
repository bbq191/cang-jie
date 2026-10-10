//! 母版库与两个代理队列（回收站 / 建文件夹）的领域错误：按种类分开，HTTP 层（`api.rs`）据此给 400 / 404 / 409 / 500，
//! 领域模块不碰 http 类型（同 `import.rs` 的 `ImportError`）。
//!
//! 2026-10-10（审计 CORE-1）：此前这些方法都回 `String`，路由一律 `map_err(ApiError::bad)` 报 400——写盘失败、后台队列停了这种
//! 设备自己的故障也被说成"请求不对"，书已不在也是 400、正在处理中也是 400，网关和排障都分不清。
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// 请求本身不对（文件名非法、格式不收、名字为空、uuid 形状不对、名字与书库对不上）→ 400。
    Invalid(String),
    /// 要操作的东西不在（母版库里没有这本书、书库里没有这份文档）→ 404。
    NotFound(String),
    /// 与当前状态冲突（正在处理中、新名字已被占用、已经在回收站）→ 409。
    Conflict(String),
    /// 设备这边出错（读写盘、后台作业队列停了）→ 500。
    Io(String),
}

impl Error {
    /// 给人看的那句话（回执 `message` 原样用它，文案与改动前一致）。
    pub fn message(&self) -> &str {
        match self {
            Error::Invalid(m) | Error::NotFound(m) | Error::Conflict(m) | Error::Io(m) => m,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message())
    }
}

/// 落库这类只记文案的调用方（结果写进边车 `message`）照旧拿字符串。
impl From<Error> for String {
    fn from(e: Error) -> String {
        e.message().to_string()
    }
}
