//! ink-serve 配置 `~/.config/notes/ink.json`（首启写出缺省供改）：聚簇/配对阈值 + 页坐标 ↔ 缩略图的几何。
//! 阈值缺省是常识值，真机样本（步骤 0）到手后标定并把结论写进白皮书。
use notecore::geom::Thresholds;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct IngestConfig {
    /// 两笔间距 ≤ 此值同一片手写（页坐标单位）。
    pub cluster_gap: f32,
    /// 簇到勾画矩形 ≤ 此值算"写在旁边"。
    pub pair_gap: f32,
    /// 书页坐标系宽高（v6 笔画坐标；经典 1404×1872，x 以页中线为 0）。缩略图按此等比映射。
    pub page_width: f32,
    pub page_height: f32,
    /// 笔画 x 坐标原点在页中线（true）还是左沿（false）。
    pub x_origin_center: bool,
    /// 裁图四周留白（页坐标单位）。
    pub crop_margin: f32,
    /// 书库目录写入防抖秒数。
    pub debounce_secs: u64,
}

impl Default for IngestConfig {
    fn default() -> Self {
        IngestConfig { cluster_gap: 40.0, pair_gap: 120.0, page_width: 1404.0, page_height: 1872.0, x_origin_center: true, crop_margin: 24.0, debounce_secs: 4 }
    }
}

impl IngestConfig {
    pub fn thresholds(&self) -> Thresholds {
        Thresholds { cluster_gap: self.cluster_gap, pair_gap: self.pair_gap }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_json_fills_defaults() {
        let c: IngestConfig = serde_json::from_str(r#"{"pairGap": 200}"#).unwrap();
        assert_eq!(c.pair_gap, 200.0);
        assert_eq!(c.cluster_gap, IngestConfig::default().cluster_gap);
        assert_eq!(c.thresholds().pair_gap, 200.0);
    }
}
