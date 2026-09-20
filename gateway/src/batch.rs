//! 服务端批量队列（2026-09-20 用户反馈驱动）。
//!
//! 此前批量是**浏览器逐个提交**（前端 `for…await` 循环）：①要先逐行勾选，100 本要点 100 下；②关掉页面/切设备，
//! 剩下没提交的就永远不会跑了。现在批量任务提交给网关，由网关自己的后台线程按顺序逐本执行，页面只负责提交
//! 和展示进度——关掉浏览器、换设备重开，队列照跑，状态照看。
//!
//! 放网关而不是 `book-serve`：网关是三个独立服务（优化/加入 xochitl 在 `book-serve`，加入 KOReader 在
//! `koreader-serve`）的唯一转发关口，且并发/内存预算闸门（[`crate::budget`]）就在这里——批量里每一本
//! 仍走同一道闸门（[`crate::budget::Budget::admit`]），跟别的操作互相排队、不叠加内存。
//! **顺序执行**：一次只处理一本（设备双核，优化内部已经在并行处理图片，见 `bookconv::imgpool`）。
//! **状态落盘**（`state/batch.json`，每次变化写一次）：网关重启（比如部署新版本）后 [`resume`] 读回未完成的队列
//! 继续跑——"关闭浏览器再回来能保持上次的未完记录并继续操作"（用户 2026-09-20 要求）。恢复时按最新母版库状态重新
//! 校验每一本（已经优化完的不会重做）。任务自带动作，一个队列里可以混合优化/加入 xochitl/加入 KOReader。
use rmsvc_core::paths::Paths;
use rmsvc_core::registry;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    Optimize,
    Deliver,
    Koreader,
}

impl Action {
    pub fn parse(s: &str) -> Option<Action> {
        match s {
            "optimize" => Some(Action::Optimize),
            "deliver" => Some(Action::Deliver),
            "koreader" => Some(Action::Koreader),
            _ => None,
        }
    }
    fn key(self) -> &'static str {
        match self {
            Action::Optimize => "optimize",
            Action::Deliver => "deliver",
            Action::Koreader => "koreader",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Job {
    action: Action,
    name: String,
    folder: String,
}

#[derive(Default, Serialize, Deserialize)]
struct State {
    queue: VecDeque<Job>,
    current: Option<Job>,
    total: u32,
    done: u32,
    failed: Vec<(String, String)>,
    /// 最近一次处理的动作（给界面显示"批量优化/批量加入…"用；队列里混动作时取当前这一本的）。
    action: Option<Action>,
    #[serde(skip)]
    worker_alive: bool,
}

fn state() -> &'static Mutex<State> {
    static S: OnceLock<Mutex<State>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(State::default()))
}

fn lock() -> std::sync::MutexGuard<'static, State> {
    state().lock().unwrap_or_else(|e| e.into_inner())
}

fn file_of(paths: &Paths) -> std::path::PathBuf {
    paths.state_dir().join("batch.json")
}

/// 落盘当前状态（每次变化调一次；失败只影响"重启后续跑"这一能力，不影响本次运行，静默）。
fn persist(paths: &Paths) {
    let json = { serde_json::to_vec(&*lock()).ok() };
    if let Some(b) = json {
        let _ = std::fs::create_dir_all(paths.state_dir());
        let _ = rmsvc_core::fs::write_atomic(&file_of(paths), &b);
    }
}

/// 这本书该动作是否有意义（跟界面批量按钮同一套资格条件）：优化=EPUB/PDF 且还没优化；加入 xochitl=EPUB/PDF；
/// 加入 KOReader=KOReader 已安装。
pub fn eligible(action: Action, item: &Value, koreader_installed: bool) -> bool {
    let format = item.get("format").and_then(|v| v.as_str()).unwrap_or("");
    let is_book = format == "epub" || format == "pdf";
    match action {
        Action::Optimize => is_book && !item.get("optimized").and_then(|v| v.as_bool()).unwrap_or(false),
        Action::Deliver => is_book,
        Action::Koreader => koreader_installed,
    }
}

pub struct Enqueued {
    pub queued: usize,
    pub skipped: usize,
}

fn agent(secs: u64) -> ureq::Agent {
    ureq::AgentBuilder::new().timeout(Duration::from_secs(secs)).build()
}

fn staging_items(paths: &Paths) -> Result<Vec<Value>, String> {
    let base = registry::find(paths, "book-serve").ok_or("book-serve 未安装或未运行")?.base_url();
    let list: Value = serde_json::from_reader(agent(10).get(&format!("{base}/staging")).call().map_err(|e| format!("查母版库失败: {e}"))?.into_reader())
        .map_err(|e| format!("母版库应答不是 JSON: {e}"))?;
    Ok(list.get("items").and_then(|v| v.as_array()).cloned().unwrap_or_default())
}

/// 入队。`names=None` 表示"母版库里所有该动作适用的书"；`Some` 只处理点名的。已经在队列里/正在处理的同名书跳过
/// （不重复排）；不适用的（如已优化的又点优化）计入 skipped。
pub fn enqueue(paths: &Paths, action: Action, names: Option<Vec<String>>, folder: &str) -> Result<Enqueued, String> {
    let koreader = registry::find(paths, "koreader-serve").is_some();
    let items = staging_items(paths)?;
    let wanted: Vec<String> = match &names {
        Some(n) => n.clone(),
        None => items.iter().filter_map(|it| it.get("name").and_then(|v| v.as_str()).map(str::to_string)).collect(),
    };
    let mut queued = 0usize;
    let mut skipped = 0usize;
    let spawn = {
        let mut st = lock();
        for name in wanted {
            let item = items.iter().find(|it| it.get("name").and_then(|v| v.as_str()) == Some(name.as_str()));
            let ok = item.map(|it| eligible(action, it, koreader)).unwrap_or(false);
            let dup = st.current.as_ref().map(|j| j.name == name && j.action == action).unwrap_or(false) || st.queue.iter().any(|j| j.name == name && j.action == action);
            if !ok {
                if names.is_some() {
                    skipped += 1; // 点名的不适用才算"跳过"；`names=None`（全部）时不适用的是预期内的筛选，不计
                }
                continue;
            }
            if dup {
                skipped += 1;
                continue;
            }
            st.queue.push_back(Job { action, name, folder: folder.to_string() });
            queued += 1;
        }
        if queued > 0 {
            // 一轮批量从空闲开始才重置计数；已经在跑的时候追加，累加进同一轮。
            if !st.worker_alive {
                st.done = 0;
                st.failed.clear();
                st.total = 0;
            }
            st.total += queued as u32;
            st.action = Some(action);
            let spawn = !st.worker_alive;
            st.worker_alive = true;
            spawn
        } else {
            false
        }
    };
    if queued > 0 {
        persist(paths);
        if spawn {
            let paths = paths.clone();
            std::thread::spawn(move || worker(&paths));
        }
    }
    Ok(Enqueued { queued, skipped })
}

/// 当前批量状态（任何会话都能看）。
pub fn status() -> Value {
    let st = lock();
    json!({
        "running": st.worker_alive,
        "action": st.current.as_ref().map(|j| j.action.key()).or(st.action.map(Action::key)),
        "total": st.total,
        "done": st.done,
        "current": st.current.as_ref().map(|j| j.name.clone()),
        "queued": st.queue.iter().take(200).map(|j| j.name.clone()).collect::<Vec<_>>(),
        "queuedCount": st.queue.len(),
        "failed": st.failed.iter().map(|(n, m)| json!({"name": n, "message": m})).collect::<Vec<_>>(),
    })
}

/// **全部中止**：清空还没开始的；正在处理的那一本：还卡在并发闸门排队 → 取消排队（[`crate::budget::Budget::cancel`]）；
/// 已经在 `book-serve` 里跑 → 请求它取消（EPUB 优化、按卷拆分投递支持中途停，其它步骤会自然跑完，见
/// `Staging::request_cancel`）。返回被清掉的数量。
pub fn stop(paths: &Paths) -> usize {
    let (n, current) = {
        let mut st = lock();
        let n = st.queue.len();
        st.queue.clear();
        // 被清掉的不会再处理，总数同步扣掉，否则停止后进度还显示"1/4"（其实只有 1 本要做）。
        st.total = st.total.saturating_sub(n as u32);
        (n, st.current.clone())
    };
    if let Some(c) = current {
        if !crate::budget::global().cancel(&c.name) {
            if let Some(info) = registry::find(paths, "book-serve") {
                let _ = post_json(&info.base_url(), "staging/cancel", json!({"name": c.name}), 10); // 尽力而为：没在跑/不可中断都无所谓
            }
        }
    }
    persist(paths);
    n
}

/// 网关启动时调用：读回上次没跑完的队列继续跑。**后台线程里等 `book-serve` 就绪**（开机/整体重启时网关可能先起），
/// 再按最新母版库状态重新校验每一本——上次进行中的那本如果已经优化完/不存在，就不再重做。
pub fn resume(paths: &Paths) {
    let Ok(text) = std::fs::read_to_string(file_of(paths)) else { return };
    let Ok(mut saved) = serde_json::from_str::<State>(&text) else { return };
    if let Some(cur) = saved.current.take() {
        saved.queue.push_front(cur); // 上次进行中的那本：重新校验后从头再来
    }
    if saved.queue.is_empty() {
        // 没有未完成的：只把"上次结果"（完成数/失败原因）读回来供界面展示。
        *lock() = saved;
        return;
    }
    let paths = paths.clone();
    std::thread::spawn(move || {
        let mut items = None;
        for _ in 0..60 {
            if let Ok(i) = staging_items(&paths) {
                items = Some(i);
                break;
            }
            std::thread::sleep(Duration::from_secs(2));
        }
        let Some(items) = items else { return };
        let koreader = registry::find(&paths, "koreader-serve").is_some();
        let before = saved.queue.len();
        saved.queue.retain(|j| items.iter().find(|it| it.get("name").and_then(|v| v.as_str()) == Some(j.name.as_str())).map(|it| eligible(j.action, it, koreader)).unwrap_or(false));
        let dropped = (before - saved.queue.len()) as u32;
        saved.total = saved.total.saturating_sub(dropped);
        let spawn = {
            let mut st = lock();
            if st.worker_alive || !st.queue.is_empty() {
                return; // 等待期间用户已经提交了新的批量，别覆盖
            }
            *st = saved;
            st.worker_alive = !st.queue.is_empty();
            st.worker_alive
        };
        persist(&paths);
        if spawn {
            worker(&paths);
        }
    });
}

fn worker(paths: &Paths) {
    loop {
        let job = {
            let mut st = lock();
            match st.queue.pop_front() {
                Some(j) => {
                    st.current = Some(j.clone());
                    j
                }
                None => {
                    st.current = None;
                    st.worker_alive = false;
                    drop(st);
                    persist(paths);
                    return;
                }
            }
        };
        persist(paths);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run_one(paths, &job))).unwrap_or_else(|_| Err("批量处理内部异常（已捕获）".to_string()));
        {
            let mut st = lock();
            st.done += 1;
            if let Err(e) = result {
                st.failed.push((job.name.clone(), e));
            }
        }
        persist(paths);
    }
}

fn post_json(base: &str, path: &str, body: Value, secs: u64) -> Result<Value, String> {
    let req = agent(secs).post(&format!("{base}/{path}")).set("Content-Type", "application/json");
    match req.send_string(&body.to_string()) {
        Ok(r) => serde_json::from_reader::<_, Value>(r.into_reader()).map_err(|e| e.to_string()),
        Err(ureq::Error::Status(_, r)) => {
            let v = serde_json::from_reader::<_, Value>(r.into_reader()).unwrap_or(Value::Null);
            Err(v.get("error").or_else(|| v.get("message")).and_then(|m| m.as_str()).unwrap_or("请求被拒绝").to_string())
        }
        Err(e) => Err(format!("服务无响应: {e}")),
    }
}

/// 这本书在母版库列表里的 `delivered.<kind>` 终态（`ok`/`failed`/`cancelled` + 文案）；条目没了（优化时改名）→ None。
fn final_check(base: &str, name: &str, kind: &str) -> Option<(String, String)> {
    let list: Value = serde_json::from_reader(agent(10).get(&format!("{base}/staging")).call().ok()?.into_reader()).ok()?;
    let it = list.get("items")?.as_array()?.iter().find(|it| it.get("name").and_then(|v| v.as_str()) == Some(name))?;
    let c = it.get("delivered")?.get(kind)?;
    Some((c.get("status")?.as_str()?.to_string(), c.get("message").and_then(|m| m.as_str()).unwrap_or("").to_string()))
}

fn run_one(paths: &Paths, job: &Job) -> Result<(), String> {
    let book = registry::find(paths, "book-serve").ok_or("book-serve 未安装或未运行")?.base_url();
    let bytes = paths.staging_dir().join(&job.name).metadata().map(|m| m.len()).unwrap_or(0);
    // 同单条操作一样过并发/内存预算闸门；批量顺序执行，所以通常立即放行，只在别处同时在跑大书时才排队。
    let slot = crate::budget::global().admit(crate::budget::tier_of(bytes), &job.name)?;
    let settled = |kind: &str, slot: crate::budget::Slot<'static>| -> Result<(), String> {
        crate::proxy::poll_until_settled(&book, &job.name);
        drop(slot);
        match final_check(&book, &job.name, kind) {
            Some((s, m)) if s == "failed" || s == "cancelled" => Err(m),
            _ => Ok(()),
        }
    };
    match job.action {
        Action::Optimize => {
            post_json(&book, "staging/optimize", json!({"name": job.name}), 60)?;
            settled("optimize", slot)
        }
        Action::Deliver => {
            post_json(&book, "staging/deliver", json!({"name": job.name, "folder": job.folder}), 60)?;
            settled("deliver", slot)
        }
        Action::Koreader => {
            let ko = registry::find(paths, "koreader-serve").ok_or("koreader-serve 未安装或未运行")?.base_url();
            let r = post_json(&ko, "books/adopt", json!({"name": job.name, "folder": job.folder}), 900);
            drop(slot);
            r?;
            // 记一笔"已加入 KOReader"（各服务只写自己的目录，落库记录归 book-serve），失败不算这本书失败。
            let _ = post_json(&book, "staging/mark", json!({"name": job.name, "target": "koreader"}), 30);
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(format: &str, optimized: bool) -> Value {
        json!({"name": "x", "format": format, "optimized": optimized})
    }

    #[test]
    fn eligibility_matches_selection_bar_buttons() {
        assert!(eligible(Action::Optimize, &item("epub", false), false));
        assert!(eligible(Action::Optimize, &item("pdf", false), false));
        assert!(!eligible(Action::Optimize, &item("epub", true), false), "已优化的不再优化");
        assert!(!eligible(Action::Optimize, &item("cbz", false), false), "只有 EPUB/PDF 能优化");
        assert!(eligible(Action::Deliver, &item("epub", true), false));
        assert!(!eligible(Action::Deliver, &item("cbz", false), false), "xochitl 只收 EPUB/PDF");
        assert!(eligible(Action::Koreader, &item("cbz", false), true));
        assert!(!eligible(Action::Koreader, &item("epub", false), false), "KOReader 没装不能加入");
    }

    #[test]
    fn action_parse_roundtrip() {
        for a in [Action::Optimize, Action::Deliver, Action::Koreader] {
            assert_eq!(Action::parse(a.key()), Some(a));
        }
        assert_eq!(Action::parse("nope"), None);
    }

    #[test]
    fn state_roundtrips_through_json_and_skips_worker_flag() {
        let mut st = State::default();
        st.queue.push_back(Job { action: Action::Deliver, name: "a.epub".into(), folder: "乱马1/2".into() });
        st.current = Some(Job { action: Action::Optimize, name: "b.epub".into(), folder: String::new() });
        st.total = 3;
        st.done = 1;
        st.failed.push(("c.epub".into(), "boom".into()));
        st.worker_alive = true;
        let text = serde_json::to_string(&st).unwrap();
        assert!(!text.contains("worker_alive"), "运行时标志不落盘");
        let back: State = serde_json::from_str(&text).unwrap();
        assert_eq!((back.total, back.done, back.queue.len(), back.failed.len()), (3, 1, 1, 1));
        assert_eq!(back.current.unwrap().name, "b.epub");
        assert_eq!(back.queue[0].folder, "乱马1/2", "任务自带的目标文件夹必须保留");
        assert!(!back.worker_alive);
    }

    #[test]
    fn stop_clears_pending_persists_and_adjusts_total() {
        let t = tempfile::tempdir().unwrap();
        let paths = Paths::resolve(|k| if k == "XDG_STATE_HOME" { Some(t.path().to_string_lossy().to_string()) } else { None });
        {
            let mut st = lock();
            st.queue.clear();
            st.queue.push_back(Job { action: Action::Optimize, name: "a".into(), folder: String::new() });
            st.queue.push_back(Job { action: Action::Optimize, name: "b".into(), folder: String::new() });
            st.total = 3; // 1 本已在做 + 2 本排队
        }
        assert_eq!(stop(&paths), 2);
        assert_eq!(status()["total"], 1, "停止后总数应只剩正在做的那 1 本");
        let saved: State = serde_json::from_str(&std::fs::read_to_string(file_of(&paths)).unwrap()).unwrap();
        assert!(saved.queue.is_empty(), "停止后落盘的队列也必须是空的，否则重启会把被中止的又跑起来");
    }
}
