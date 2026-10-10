//! 书库文件夹的类型：[`FolderId`]（一个活文件夹的 uuid）与 [`Folder`]（书库根，或某个文件夹）。
//!
//! 为什么要类型（2026-10-10，审计 X-1/X-3）：此前文件夹"名字"与"uuid"都是 `&str`，上传接口成对出现
//! （`upload`/`upload_into`、`upload_file`/`upload_file_into`……），把 uuid 传给按名字的那个也照样编译通过——
//! note-serve 就这样把笔记本投到书库根投了一个月。新接口只收 [`Folder`]：
//! - **按名字解析由调用方先做**（[`super::Xochitl::folder_by_name`] 全库按名找、[`super::Xochitl::child_folder`] 按层找），
//!   拿到 [`FolderId`] 再投。全库按名找在多级同名文件夹下有歧义（「漫画/卷01」「小说/卷01」），按层找没有；
//!   哪种语义对是调用方的业务决定，基座不替它猜，所以不提供"传名字进来、里面找"的新入口。
//! - 根用 `Folder::Root` 显式表达，不再用空串——空串既可能是"根"，也可能是"名字没填"。
//! - [`FolderId`] 只能经 [`FolderId::parse`]（校验 uuid 形状，挡住 `trash`、`../` 与名字）或书库查询得到。
use super::library::is_uuid_shape;

/// 一个文件夹的 uuid（形状已校验：36 字符十六进制与 `-`）。不保证此刻书库里真有它（可能刚被删）。
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FolderId(String);

impl FolderId {
    /// 校验 uuid 形状；不像 uuid（空串、`trash`、文件夹名字、路径）→ `None`。
    pub fn parse(s: &str) -> Option<FolderId> {
        is_uuid_shape(s).then(|| FolderId(s.to_string()))
    }

    /// 不校验形状（内部：旧的按 `&str` 的入口委托新实现时，原样保留它们收任意字符串的行为）。
    pub(crate) fn unchecked(s: &str) -> FolderId {
        FolderId(s.to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for FolderId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// 投递目标：书库根，或某个文件夹。
#[derive(Clone, Debug, PartialEq, Eq, Hash, Default)]
pub enum Folder {
    #[default]
    Root,
    Id(FolderId),
}

impl Folder {
    /// xochitl `.metadata` 的 `parent` 写法：根是空串，文件夹是 uuid。
    pub fn as_parent_str(&self) -> &str {
        match self {
            Folder::Root => "",
            Folder::Id(id) => id.as_str(),
        }
    }

    /// 从 `.metadata` 的 `parent` 字段解析：空串 → 根；uuid 形状 → 文件夹；`trash` 或别的 → `None`。
    pub fn from_parent_str(s: &str) -> Option<Folder> {
        if s.is_empty() {
            Some(Folder::Root)
        } else {
            FolderId::parse(s).map(Folder::Id)
        }
    }

    /// 内部：旧接口的 `folder_uuid: &str`（空串＝根，其余原样当 uuid，不校验——保持它们一直以来的行为）。
    pub(crate) fn from_legacy_uuid(s: &str) -> Folder {
        if s.is_empty() {
            Folder::Root
        } else {
            Folder::Id(FolderId::unchecked(s))
        }
    }
}

impl From<FolderId> for Folder {
    fn from(id: FolderId) -> Folder {
        Folder::Id(id)
    }
}

impl From<Option<FolderId>> for Folder {
    /// `None`（按名字没找到）→ 根：与旧接口"找不到就落书库根"的兜底一致，但兜底发生在调用方眼皮底下。
    fn from(id: Option<FolderId>) -> Folder {
        id.map(Folder::Id).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const U: &str = "0a1b2c3d-4e5f-6789-abcd-ef0123456789";

    #[test]
    fn folder_id_rejects_names_trash_and_paths() {
        assert_eq!(FolderId::parse(U).unwrap().as_str(), U);
        for bad in ["", "trash", "漫画", "../../etc", "0a1b2c3d-4e5f-6789-abcd-ef012345678"] {
            assert!(FolderId::parse(bad).is_none(), "{bad:?}");
        }
    }

    #[test]
    fn folder_parent_str_round_trip() {
        assert_eq!(Folder::from_parent_str(""), Some(Folder::Root));
        assert_eq!(Folder::from_parent_str(U).unwrap().as_parent_str(), U);
        assert_eq!(Folder::from_parent_str("trash"), None, "回收站不是投递目标");
        assert_eq!(Folder::Root.as_parent_str(), "");
        assert_eq!(Folder::from(None::<FolderId>), Folder::Root);
        assert_eq!(Folder::from(FolderId::parse(U)), Folder::Id(FolderId::parse(U).unwrap()));
        assert_eq!(Folder::from_legacy_uuid(""), Folder::Root);
        assert_eq!(Folder::from_legacy_uuid("c2").as_parent_str(), "c2", "旧接口收的任意字符串原样保留");
    }
}
