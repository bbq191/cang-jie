//! ink-serve 配置 `~/.config/notes/ink.json`（首启写出缺省供改）：聚簇/配对阈值、裁图留白、防抖。
//! 2026-09-07 真机样本标定：`cluster_gap`/`pair_gap` 常识缺省验证有效未改。页坐标系的标定结论（EPUB 页 `.rm` 坐标系
//! 是 EPUB 排版引擎自己的画布 **960×1280**、x 以页中线为 0，不是物理屏 1404×1872；拿真机缩略图里三条高亮的橙色像素
//! 行/列实测反推，细节见笔记线白皮书 §03g）当初是给"按缩略图裁"用的，那条路径退役后 `pageWidth`/`pageHeight`/
//! `xOriginCenter` 三个配置项再没人读，2026-09-30 删掉（老配置文件里的这几个键反序列化时忽略，不影响读取）。
use notecore::geom::Thresholds;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct IngestConfig {
    /// 两笔间距 ≤ 此值同一片手写（页坐标单位）。
    pub cluster_gap: f32,
    /// 簇到勾画矩形 ≤ 此值算"写在旁边"。
    pub pair_gap: f32,
    /// 裁图四周留白（页坐标单位）。
    pub crop_margin: f32,
    /// 书库目录写入防抖秒数。
    pub debounce_secs: u64,
}

impl Default for IngestConfig {
    fn default() -> Self {
        IngestConfig { cluster_gap: 40.0, pair_gap: 160.0, crop_margin: 24.0, debounce_secs: 4 }
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
        // 老配置文件里已删掉的键（pageWidth 等）照样读得进来
        let c: IngestConfig = serde_json::from_str(r#"{"pairGap": 200, "pageWidth": 960, "xOriginCenter": true}"#).unwrap();
        assert_eq!(c.pair_gap, 200.0);
        assert_eq!(c.cluster_gap, IngestConfig::default().cluster_gap);
        assert_eq!(c.thresholds().pair_gap, 200.0);
    }
}
