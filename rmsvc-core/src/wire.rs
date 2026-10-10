//! 跨服务 / 前后端共用的线上状态值（2026-10-10 起）。此前 `"pending"`/`"ok"`/`"failed"`/`"onopen"`/`"timeout"`
//! 这批字面量散落在 book-serve 的边车、网关的批量队列和网页脚本里各写一遍，拼错一个字母编译器发现不了。
//! 这里只定义枚举本身，**线上 JSON 形状不变**（小写字符串，与边车里已有的记录逐字节一致）；各服务第二阶段再迁移过来。
use serde::{Deserialize, Serialize};

/// 落库（把母版库里的书加入 xochitl）的异步结果，边车 `delivered.deliver.status`。
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
#[serde(rename_all = "lowercase")]
pub enum DeliverStatus {
    /// 后台线程还在跑。
    #[default]
    Pending,
    Ok,
    Failed,
    /// 认不出的历史值兜底（设备上的旧边车写过 `"cancelled"`——书架还"优化"书时取消操作留下的——也可能有别的）。
    /// 未知值不能让整份边车解析失败，所以用 `#[serde(other)]` 收住，而不是把 `cancelled` 硬映射成 `Failed`：
    /// 那会把"用户自己取消"误显示成"失败"，且只认得这一个旧值。写回时成了 `"unknown"`（原值丢了，但那些值本来就
    /// 已经没有任何代码在用）。调用方按"不是 ok"处理即可。
    #[serde(other)]
    Unknown,
}

/// 投原生后的渲染自检结果，边车 `delivered.render.status`。
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
#[serde(rename_all = "lowercase")]
pub enum RenderStatus {
    /// 等 xochitl 渲染。
    #[default]
    Pending,
    Ok,
    /// 大文件通道的 EPUB：首次打开才渲染。
    Onopen,
    Timeout,
    /// 2026-10-07 前写过的"页数远低于期望"；之后不再写出，旧边车里的记录照样能读。
    Warn,
    /// 认不出的历史值兜底，理由同 [`DeliverStatus::Unknown`]。
    #[serde(other)]
    Unknown,
}

impl DeliverStatus {
    /// 线上字符串（与 serde 一致）。
    pub fn as_str(self) -> &'static str {
        match self {
            DeliverStatus::Pending => "pending",
            DeliverStatus::Ok => "ok",
            DeliverStatus::Failed => "failed",
            DeliverStatus::Unknown => "unknown",
        }
    }
}

impl RenderStatus {
    /// 线上字符串（与 serde 一致）。
    pub fn as_str(self) -> &'static str {
        match self {
            RenderStatus::Pending => "pending",
            RenderStatus::Ok => "ok",
            RenderStatus::Onopen => "onopen",
            RenderStatus::Timeout => "timeout",
            RenderStatus::Warn => "warn",
            RenderStatus::Unknown => "unknown",
        }
    }
}

impl std::fmt::Display for DeliverStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::fmt::Display for RenderStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deliver_status_round_trips_as_lowercase_strings() {
        for (s, v) in [("pending", DeliverStatus::Pending), ("ok", DeliverStatus::Ok), ("failed", DeliverStatus::Failed)] {
            assert_eq!(serde_json::to_string(&v).unwrap(), format!("\"{s}\""));
            assert_eq!(serde_json::from_str::<DeliverStatus>(&format!("\"{s}\"")).unwrap(), v);
            assert_eq!(v.as_str(), s);
            assert_eq!(v.to_string(), s);
        }
        assert_eq!(serde_json::from_str::<DeliverStatus>("\"OK\"").unwrap(), DeliverStatus::Unknown, "大小写敏感：不是 ok");
    }

    /// 旧边车里的历史值（`cancelled` 等）与任意未知字符串都能读，不让整份边车解析失败。
    #[test]
    fn unknown_legacy_values_are_tolerated() {
        for v in ["cancelled", "optimizing", "", "随便什么"] {
            assert_eq!(serde_json::from_str::<DeliverStatus>(&format!("{v:?}")).unwrap(), DeliverStatus::Unknown, "{v}");
            assert_eq!(serde_json::from_str::<RenderStatus>(&format!("{v:?}")).unwrap(), RenderStatus::Unknown, "{v}");
        }
        assert_eq!(serde_json::from_str::<RenderStatus>("\"warn\"").unwrap(), RenderStatus::Warn);
        #[derive(Deserialize)]
        struct Sidecar {
            deliver: Check,
        }
        #[derive(Deserialize)]
        struct Check {
            status: DeliverStatus,
            message: String,
        }
        let old: Sidecar = serde_json::from_str(r#"{"native":1,"deliver":{"status":"cancelled","message":"已取消","at":1}}"#).unwrap();
        assert_eq!((old.deliver.status, old.deliver.message.as_str()), (DeliverStatus::Unknown, "已取消"));
        assert_eq!(serde_json::to_string(&DeliverStatus::Unknown).unwrap(), "\"unknown\"");
    }

    #[test]
    fn render_status_round_trips_and_reads_legacy_warn() {
        let all = [
            ("pending", RenderStatus::Pending),
            ("ok", RenderStatus::Ok),
            ("onopen", RenderStatus::Onopen),
            ("timeout", RenderStatus::Timeout),
            ("warn", RenderStatus::Warn),
        ];
        for (s, v) in all {
            assert_eq!(serde_json::to_string(&v).unwrap(), format!("\"{s}\""));
            assert_eq!(serde_json::from_str::<RenderStatus>(&format!("\"{s}\"")).unwrap(), v);
            assert_eq!(v.as_str(), s);
        }
        // 2026-10-07 前的旧边车记录（带已退役的 expected 字段）
        #[derive(Deserialize)]
        struct Old {
            status: RenderStatus,
        }
        let old: Old = serde_json::from_str(r#"{"uuid":"u","pages":3,"status":"warn","expected":200,"at":1}"#).unwrap();
        assert_eq!(old.status, RenderStatus::Warn);
    }
}
