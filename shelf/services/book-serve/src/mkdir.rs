//! 原生书库「建文件夹」队列：外部进程不能直接建文件夹（跟不能直改 `.metadata` 一样，唯一合法路是
//! xochitl 自己的代码路 `Library.createCollection(parentId, name)`）——由注入 MainView 的
//! `shelf/xovi/shelf-mkdir-agent.qmd` 长轮询 `GET /mkdir/pending?wait=` 执行。本模块只管队列：`add()` 入队去重，
//! `pending()` 顺手把已经真实存在的文件夹名剔除（QML 端无需 ack，跟 `trash.rs` 的剔除方式对称）。
//! 队列文件 `$XDG_STATE_HOME/shelf/books/mkdir-pending.json`。
//!
//! **2026-09-19 复活**：这套队列+qmd 代理 2026-09-07 最早是给 `note-serve` 生成《书名》一章一本用的，
//! 2026-09-09 note-serve 改成复用书本自己的设备文件夹后没了消费方，2026-09-15 被当死代码物理删除。
//! 这次真正的消费方是网页母版库「加入 xochitl → 文件夹」自由输入框（2026-09-19 用户反馈"填个文件夹
//! 名依然不会创建文件夹"）——`staging::Staging::deliver` 落库前调用，folder 不存在就入队，同步等
//! （`fswatch::wait_for`）agent 真的建出来再继续投递，见 `delivery.rs` 的 `XochitlDelivery::ensure_folder`。代码本身
//! `git show <删除前的 commit>^:...` 原样捞回，逻辑没变——当年写的时候就已经想清楚了，只是一直没等到
//! 真消费方。**`Library.createCollection` 这条调用链当年只做到"反编译 + 静态调用链一致"，从没有真机
//! 点过新建文件夹按钮做交叉验证**（见 `shelf-mkdir-agent.qmd` 头注原样保留的踩坑记录），这次借着
//! 有了真消费方顺手把这层最后验证补上。
//!
//! **按层建（2026-10-07）**：直接导入（`import.rs`）的 `folder` 是多级路径（`漫画/死亡筆記(愛藏版)`），要逐级建——
//! 队列项因此带上父文件夹 uuid（[`Pending::parent`]，空串＝书库根；此前的队列文件没有这个字段，按根读）。去重和
//! "已经建出来就剔除"都按（父, 名）判：只有那个父文件夹正下方有同名活文件夹才算存在。网页的 `POST /mkdir/add {name}`
//! 不变：建在根，名字里的 `/` 当普通字符。
//!
//! 持久化、入队去重、剔除、交给代理、交满次数放弃这一整套委托 `pending_queue::AgentQueue<T>`（2026-09-09 消重复、2026-10-10
//! 再合并 `pending` 流程，跟 `trash.rs` 是同一份基础设施，见该模块文档）；这里只留领域校验（名字合法性/文件夹是否已存在）。
use crate::agent_failures::AgentFailures;
use crate::pending_queue::{AgentItem, AgentQueue, HANDOUT_MAX_ATTEMPTS};
use serde::{Deserialize, Serialize};
use rmsvc_core::xochitl::{find_child_folder, folder_keys, is_uuid_shape};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// 同一个文件夹名交给 QML 代理后，这段时间内不再重复交出。建夹是异步的：`Library.createCollection`
/// 调用后 `.metadata` 稍晚才落盘，长轮询让代理几乎立刻再来拉，若这时仍把同名交出去就会建出两个重名文件夹。
/// 超过这个时间还没建出来（createCollection 没生效）才再交一次，等于自带重试。
const HANDOUT_QUIET: Duration = Duration::from_secs(15);

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Pending {
    pub name: String,
    /// 建在哪个文件夹下面（uuid；空串＝书库根）。2026-10-07 前的队列文件没有这个字段，按根读。
    #[serde(default)]
    pub parent: String,
    pub at: u64,
}

/// 交给 QML 代理的一项：在 `parent`（空串＝根）下建 `name`。`GET /mkdir/pending` 的 `items` 就是这个。
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct MkdirItem {
    pub name: String,
    pub parent: String,
}

impl AgentItem for Pending {
    /// （上级 uuid, 名字）：不同上级下的同名文件夹是不同的项。
    type Key = (String, String);
    const KIND: &'static str = "mkdir";
    fn key(&self) -> (String, String) {
        (self.parent.clone(), self.name.clone())
    }
    fn give_up_log(&self) -> String {
        let parent = if self.parent.is_empty() { "书库根" } else { self.parent.as_str() };
        format!("建文件夹《{}》（上级 {parent}）已交给 xochitl {HANDOUT_MAX_ATTEMPTS} 次仍没建出来，放弃（移出队列）", self.name)
    }
    fn failure(&self) -> (&str, &str) {
        (&self.name, "")
    }
}

fn item_of((parent, name): (String, String)) -> MkdirItem {
    MkdirItem { name, parent }
}

pub struct MkdirQueue {
    /// 落盘队列 + 长轮询 + 交出后静默期（见 [`HANDOUT_QUIET`]），与 `trash.rs` 共用一套。
    q: AgentQueue<Pending>,
    lib_dir: PathBuf,
}

impl MkdirQueue {
    pub fn new(state_books_dir: &Path, lib_dir: &Path) -> MkdirQueue {
        MkdirQueue { q: AgentQueue::new(state_books_dir.join("mkdir-pending.json"), HANDOUT_QUIET), lib_dir: lib_dir.to_path_buf() }
    }

    pub fn with_failures(mut self, f: Arc<AgentFailures>) -> MkdirQueue {
        self.q = self.q.with_failures(f);
        self
    }

    /// 单测用：缩短/取消"重复交出"的静默期。
    #[cfg(test)]
    fn with_handout_quiet(mut self, d: Duration) -> MkdirQueue {
        self.q = self.q.with_handout_quiet(d);
        self
    }

    /// 入队一个文件夹名；已经真实存在或已在队列里都不重复加。名字不能为空——`/`、`\` 曾经也被当
    /// "路径分隔符防误传"拦掉，2026-09-19 真机反馈坐实是误伤：这个名字全程只当 JSON `visibleName`
    /// 字符串走（`Library.createCollection(parentId, name)` 收的是普通 JS 字符串，不是文件系统路径，
    /// 本模块不把名字当路径拆——多级文件夹由直接导入逐级调 `add_in` 建，见 `delivery::XochitlDelivery::ensure_folder_path`），真实书名/文件夹名带斜杠很常见（如《乱马1/2》），
    /// 拦它没有技术依据、只会挡合法输入——见 `Xochitl::folder_by_name`/`child_folder`/`upload_and_claim` 全程都是按
    /// `visibleName` 字符串整体比较，folder 的文件系统路径只走 uuid，从不落到名字里。
    ///
    /// 建在书库根（`POST /mkdir/add`、网页落库用），等于 `add_in("", name)`。
    pub fn add(&self, name: &str) -> Result<usize, String> {
        self.add_in("", name)
    }

    /// 在 `parent`（文件夹 uuid，空串＝根）下入队建 `name`：那个父文件夹正下方已经有同名活文件夹 → `Ok(0)`（不用建）；
    /// 队列里已有同一（父, 名）不重复加；返回入队后的队列长度。`parent` 不是空串也不是 uuid 形状 → `Err`。
    pub fn add_in(&self, parent: &str, name: &str) -> Result<usize, String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("文件夹名不能为空".into());
        }
        if !parent.is_empty() && !is_uuid_shape(parent) {
            return Err("上级文件夹 uuid 格式不对".into());
        }
        if find_child_folder(&self.lib_dir, parent, name).is_some() {
            return Ok(0); // 已经存在，不用建
        }
        self.q.add(|p| p.parent == parent && p.name == name, || Pending { name: name.to_string(), parent: parent.to_string(), at: rmsvc_core::clock::now_secs() })
    }

    /// 待办（QML 代理拉取）：顺手清掉已经真实建出来的（QML 端无需 ack），以及交满次数仍没建出来、放弃的。
    /// 返回 (待办项列表, 本次清掉几条)。
    /// 刚交出去不久（[`HANDOUT_QUIET`]）的项不再重复返回；返回的项同时记为"已交出"。
    /// 线上走的是长轮询版 [`Self::pending_wait`]，这个立即返回的版本只给单测用。
    #[cfg(test)]
    pub fn pending(&self) -> Result<(Vec<MkdirItem>, usize), String> {
        let (keys, dropped) = self.q.pending(self.still_pending())?;
        Ok((keys.into_iter().map(item_of).collect(), dropped))
    }

    /// "这一项还要不要等"的判据：那个父文件夹正下方还没有同名活文件夹。书库文件夹只扫一遍、且只在队列非空时扫
    /// （`prune` 对空队列不调谓词）：此前每条待办各调一次按名字找文件夹，每次都把书库里全部 `.metadata` 读一遍解析一遍，
    /// k 条待办＝k 遍全库扫描，长轮询每次唤醒都来一轮（2026-09-24 审计）。判据与 `find_child_folder` 相同。
    /// 每次查队列造一个新的（长轮询醒来重扫）。
    fn still_pending(&self) -> impl Fn(&Pending) -> bool + '_ {
        let folders = std::cell::OnceCell::new();
        move |p: &Pending| !folders.get_or_init(|| folder_keys(&self.lib_dir)).contains(&(p.parent.clone(), p.name.clone()))
    }

    /// 长轮询版 [`Self::pending`]：有待办立即返回；没有就阻塞到入队唤醒或 `wait` 到期（到期返回空列表）。
    /// 这样 QML 代理不必每 8 秒定时拉一次——空闲时整条链路零唤醒，入队后也是即刻响应而不是平均等 4 秒。
    /// `wait` 为零＝不等（旧的立即返回语义）。
    pub fn pending_wait(&self, wait: Duration) -> Result<(Vec<MkdirItem>, usize), String> {
        let (keys, dropped) = self.q.pending_wait(wait, || self.still_pending())?;
        Ok((keys.into_iter().map(item_of).collect(), dropped))
    }

    pub fn list(&self) -> Vec<Pending> {
        self.q.list()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    /// 建在根下的这些名字（期望值）。
    fn root(names: &[&str]) -> Vec<MkdirItem> {
        names.iter().map(|n| MkdirItem { name: n.to_string(), parent: String::new() }).collect()
    }

    fn lib(t: &tempfile::TempDir) -> PathBuf {
        let d = t.path().join("xochitl");
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn add_rejects_bad_names_and_existing_folders_then_pending_prunes_created() {
        let t = tempfile::tempdir().unwrap();
        let lib_dir = lib(&t);
        let q = MkdirQueue::new(&t.path().join("state"), &lib_dir).with_handout_quiet(Duration::ZERO);

        assert!(q.add("").unwrap_err().contains("不能为空"));

        assert_eq!(q.add("《人骨拼圖》").unwrap(), 1);
        assert_eq!(q.add("《人骨拼圖》").unwrap(), 1, "重复入队不翻倍");
        assert_eq!(q.add(" 《人骨拼圖》 ").unwrap(), 1, "trim 后同名，不重复入队"); // add() trims name first

        assert_eq!(q.add("《消失的爱人》").unwrap(), 2);

        let (names, pruned) = q.pending().unwrap();
        assert_eq!((names.len(), pruned), (2, 0));

        // QML 代理把《人骨拼圖》真的建出来了
        std::fs::write(lib_dir.join("f1.metadata"), r#"{"type":"CollectionType","visibleName":"《人骨拼圖》","parent":""}"#).unwrap();
        let (names, pruned) = q.pending().unwrap();
        assert_eq!((names, pruned), (root(&["《消失的爱人》"]), 1));
        assert_eq!(q.list().len(), 1);

        // 已存在的文件夹再入队直接判"不用建"，不落队列
        assert_eq!(q.add("《人骨拼圖》").unwrap(), 0);
        assert_eq!(q.list().len(), 1, "没有新增");
    }

    /// 2026-09-19 真机反馈：《乱马1/2》这类带 `/` 的真实文件夹名曾被当"路径分隔符防误传"拒绝，
    /// 导致 `ensure_folder` 静默放弃、书落回根目录——这个名字全程只当 JSON `visibleName` 字符串走
    /// （`Library.createCollection` 收的是普通 JS 字符串，不是文件系统路径），拦它没有技术依据。
    #[test]
    fn add_accepts_names_with_slash() {
        let t = tempfile::tempdir().unwrap();
        let lib_dir = lib(&t);
        let q = MkdirQueue::new(&t.path().join("state"), &lib_dir);
        assert_eq!(q.add("乱马1/2").unwrap(), 1);
        assert_eq!(q.list()[0].name, "乱马1/2");
        assert_eq!(q.add(r"a\b").unwrap(), 2);
    }

    /// 按（父, 名）去重、判存在：不同上级下的同名子文件夹各是各的；根下有同名的不妨碍在别的文件夹下建。
    #[test]
    fn add_in_dedupes_and_prunes_by_parent_and_name() {
        const P1: &str = "11111111-1111-4111-8111-111111111111";
        const P2: &str = "22222222-2222-4222-8222-222222222222";
        let t = tempfile::tempdir().unwrap();
        let lib_dir = lib(&t);
        let q = MkdirQueue::new(&t.path().join("state"), &lib_dir).with_handout_quiet(Duration::ZERO);
        std::fs::write(lib_dir.join("r.metadata"), r#"{"type":"CollectionType","visibleName":"卷01","parent":""}"#).unwrap();
        assert_eq!(q.add("卷01").unwrap(), 0, "根下已有");
        assert_eq!(q.add_in(P1, "卷01").unwrap(), 1, "根下有同名不算 P1 下有");
        assert_eq!(q.add_in(P1, "卷01").unwrap(), 1, "同一（父, 名）不重复");
        assert_eq!(q.add_in(P2, "卷01").unwrap(), 2, "别的上级下的同名是另一项");
        assert!(q.add_in("../x", "卷01").unwrap_err().contains("uuid"));
        let it = |parent: &str| MkdirItem { name: "卷01".into(), parent: parent.into() };
        assert_eq!(q.pending().unwrap(), (vec![it(P1), it(P2)], 0));
        // P1 下建出来了：只剔除 P1 那一项
        std::fs::write(lib_dir.join("c1.metadata"), format!(r#"{{"type":"CollectionType","visibleName":"卷01","parent":"{P1}"}}"#)).unwrap();
        assert_eq!(q.pending().unwrap(), (vec![it(P2)], 1));
        assert_eq!(q.list().iter().map(|p| p.parent.as_str()).collect::<Vec<_>>(), [P2]);
    }

    /// 2026-10-07 前的队列文件没有 `parent` 字段：按根读，照常交出、照常剔除。
    #[test]
    fn old_queue_file_without_parent_reads_as_root() {
        let t = tempfile::tempdir().unwrap();
        let lib_dir = lib(&t);
        let state = t.path().join("state");
        std::fs::create_dir_all(&state).unwrap();
        std::fs::write(state.join("mkdir-pending.json"), r#"[{"name":"旧","at":1}]"#).unwrap();
        let q = MkdirQueue::new(&state, &lib_dir).with_handout_quiet(Duration::ZERO);
        assert_eq!(q.list(), vec![Pending { name: "旧".into(), parent: String::new(), at: 1 }]);
        assert_eq!(q.pending().unwrap().0, root(&["旧"]));
        assert_eq!(q.add("旧").unwrap(), 1, "和新入队的根项是同一项");
        std::fs::write(lib_dir.join("f.metadata"), r#"{"type":"CollectionType","visibleName":"旧","parent":""}"#).unwrap();
        assert_eq!(q.pending().unwrap(), (root(&[]), 1));
    }

    #[test]
    fn handed_out_name_is_not_repeated_within_quiet_period() {
        let t = tempfile::tempdir().unwrap();
        let q = MkdirQueue::new(&t.path().join("state"), &lib(&t));
        q.add("甲").unwrap();
        assert_eq!(q.pending().unwrap().0, root(&["甲"]));
        assert!(q.pending().unwrap().0.is_empty(), "静默期内不重复交出，免得代理立刻再拉时建出重名文件夹");
        // 静默期过后仍没建出来 → 再交一次（自带重试）
        let q2 = MkdirQueue::new(&t.path().join("state"), &lib(&t)).with_handout_quiet(Duration::ZERO);
        assert_eq!(q2.pending().unwrap().0, root(&["甲"]));
        assert_eq!(q2.pending().unwrap().0, root(&["甲"]));
    }

    /// 交满次数仍没建出来的名字放弃（移出队列、算进"清掉几条"让网页刷新），不再每个静默期重交一次。
    #[test]
    fn gives_up_after_max_attempts() {
        let t = tempfile::tempdir().unwrap();
        let fails = Arc::new(AgentFailures::new(&t.path().join("state"), None));
        let q = MkdirQueue::new(&t.path().join("state"), &lib(&t)).with_handout_quiet(Duration::ZERO).with_failures(fails.clone());
        q.add("丁").unwrap();
        for _ in 0..HANDOUT_MAX_ATTEMPTS {
            assert_eq!(q.pending().unwrap(), (root(&["丁"]), 0));
        }
        assert_eq!(q.pending().unwrap(), (root(&[]), 1), "交满放弃");
        assert!(q.list().is_empty(), "移出队列");
        assert_eq!(fails.list().iter().map(|f| (f.kind.as_str(), f.name.as_str())).collect::<Vec<_>>(), [("mkdir", "丁")], "放弃记下来给网页看");
        assert_eq!(q.pending().unwrap(), (root(&[]), 0));
    }

    /// 回归：交出后没建成的名字，长轮询在静默期到期时就重交，而不是等满整个 `wait`（代理发 290 秒）。
    #[test]
    fn long_poll_rehands_unfinished_item_when_quiet_period_ends() {
        let t = tempfile::tempdir().unwrap();
        let q = MkdirQueue::new(&t.path().join("state"), &lib(&t)).with_handout_quiet(Duration::from_millis(200));
        q.add("丙").unwrap();
        assert_eq!(q.pending_wait(Duration::from_secs(10)).unwrap().0, root(&["丙"]), "首次立即交出");
        let t0 = Instant::now();
        assert_eq!(q.pending_wait(Duration::from_secs(10)).unwrap().0, root(&["丙"]), "静默期过后重交");
        let took = t0.elapsed();
        assert!(took >= Duration::from_millis(150) && took < Duration::from_secs(3), "应在静默期到期时醒来，实际等了 {took:?}");
        // 建出来之后：静默期到期醒来也只是发现已完成，不再交出，等到 wait 到期
        std::fs::write(lib(&t).join("f.metadata"), r#"{"type":"CollectionType","visibleName":"丙","parent":""}"#).unwrap();
        let t0 = Instant::now();
        assert!(q.pending_wait(Duration::from_millis(600)).unwrap().0.is_empty());
        assert!(t0.elapsed() >= Duration::from_millis(550));
    }

    #[test]
    fn pending_wait_returns_promptly_on_add_and_times_out_when_idle() {
        let t = tempfile::tempdir().unwrap();
        let q = std::sync::Arc::new(MkdirQueue::new(&t.path().join("state"), &lib(&t)));

        // 空闲：等满就返回空
        let t0 = Instant::now();
        assert!(q.pending_wait(Duration::from_millis(150)).unwrap().0.is_empty());
        assert!(t0.elapsed() >= Duration::from_millis(140));

        // 入队即刻唤醒，远早于 wait 上限
        let q2 = q.clone();
        let h = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            q2.add("乙").unwrap();
        });
        let t0 = Instant::now();
        let (names, _) = q.pending_wait(Duration::from_secs(10)).unwrap();
        assert_eq!(names, root(&["乙"]));
        assert!(t0.elapsed() < Duration::from_secs(3), "应被入队唤醒，实际等了 {:?}", t0.elapsed());
        h.join().unwrap();

        // wait=0 等价于旧的立即返回
        let t0 = Instant::now();
        assert!(q.pending_wait(Duration::ZERO).unwrap().0.is_empty());
        assert!(t0.elapsed() < Duration::from_millis(100));
    }
}
