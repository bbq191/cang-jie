//! 服务端批量队列（2026-09-20 用户反馈驱动）。
//!
//! 此前批量是**浏览器逐个提交**（前端 `for…await` 循环）：①要先逐行勾选，100 本要点 100 下；②关掉页面/切设备，
//! 剩下没提交的就永远不会跑了。现在批量任务提交给网关，由网关自己的后台线程按顺序逐本执行，页面只负责提交
//! 和展示进度——关掉浏览器、换设备重开，队列照跑，状态照看。
//!
//! **顺序执行**：一次只处理一本，等它在 book-serve 那边真正处理完（[`poll_until_settled`]）才取下一本，所以同一时刻最多
//! 一本书在投——网页的「加入 xochitl」只走这条队列。原来网关还有一道并发/内存预算闸门（`budget.rs`，按书的体积分大小档
//! 限并发）：那是设备上优化大书时内存峰值接近文件体积留下的，书架 2026-10-07 不再优化书后，加入 xochitl 是流式上传、
//! book-serve 只多占几 MB，加上这条队列本来就一本一本来，闸门已经拦不到任何东西，同日删掉。
//! **状态落盘**（`state/batch.json`，每次变化写一次）：网关重启（比如部署新版本）后 [`resume`] 读回未完成的队列
//! 继续跑——"关闭浏览器再回来能保持上次的未完记录并继续操作"（用户 2026-09-20 要求）。恢复时按最新母版库状态重新
//! 校验每一本（已经不在母版库的不再做）。任务自带动作，现在只剩「加入 xochitl」。
//! 已下线的动作：「批量加入 KOReader」（2026-09-29 设备卸载 KOReader）、「批量优化」（2026-10-07 书架不再优化书）。
use rmsvc_core::http::{ApiError, ApiResult, Reply, Request};
use rmsvc_core::paths::Paths;
use rmsvc_core::registry::SvcClient;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    Deliver,
}

impl Action {
    /// 全部动作（网页按 `stg.batch.<key>` 出标题，`ui.rs` 的测试遍历它核对语言包）。
    pub const ALL: &'static [Action] = &[Action::Deliver];

    pub fn parse(s: &str) -> Option<Action> {
        Action::ALL.iter().copied().find(|a| a.key() == s)
    }
    pub fn key(self) -> &'static str {
        match self {
            Action::Deliver => "deliver",
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
    #[serde(skip)]
    worker_alive: bool,
    /// [`resume`] 正在等 book-serve 起来（开机时网关可能先起）：界面据此显示"等书架服务启动"，而不是像卡住的批量。
    /// 运行时标志，不落盘。
    #[serde(skip)]
    waiting_service: bool,
    /// 「全部中止」时正在处理的那一本也要停（见 [`run_one`]）；worker 取下一本时复位。运行时标志，不落盘（同 `worker_alive`）。
    #[serde(skip)]
    abort_current: bool,
}

fn state() -> &'static Mutex<State> {
    static S: OnceLock<Mutex<State>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(State::default()))
}

fn lock() -> std::sync::MutexGuard<'static, State> {
    rmsvc_core::sync::lock(state())
}

fn file_of(paths: &Paths) -> std::path::PathBuf {
    paths.state_dir().join("batch.json")
}

/// 落盘互斥：**序列化与写盘放在同一把锁里**。此前 `persist` 在状态锁内序列化、出锁后才写文件，HTTP 线程
/// （`enqueue`/`stop`）与 worker 线程各自调用，较早序列化的旧快照可能比新快照后写盘，把队列回退到过期状态
/// （重启后 `resume` 会读到它）。现在后取得本锁的一定序列化得更晚，落盘顺序 = 状态变化顺序。
fn persist_lock() -> &'static Mutex<()> {
    static L: OnceLock<Mutex<()>> = OnceLock::new();
    L.get_or_init(|| Mutex::new(()))
}

/// 落盘当前状态（每次变化调一次；失败只影响"重启后续跑"这一能力，不影响本次运行，静默）。
fn persist(paths: &Paths) {
    let _g = rmsvc_core::sync::lock(persist_lock());
    let json = { serde_json::to_vec(&*lock()).ok() };
    if let Some(b) = json {
        let _ = rmsvc_core::fs::write_atomic(&file_of(paths), &b); // 父目录由 write_atomic 自己建
    }
    // 状态每变一次就落一次盘，正好是"该通知网页刷新"的时机（前端不再批量运行时每 3 秒轮询）。
    crate::events::notify_books("batch");
}

/// 这本书该动作是否有意义（跟界面批量按钮同一套资格条件）：加入 xochitl = xochitl 原生能读的格式
/// （`rmsvc_core::formats::NATIVE_EXTS`，即 EPUB/PDF；book-serve 列表的 `format` 字段就是这几个扩展名或 `other`）。
pub fn eligible(action: Action, item: &Value) -> bool {
    let format = item.get("format").and_then(|v| v.as_str()).unwrap_or("");
    match action {
        Action::Deliver => rmsvc_core::formats::NATIVE_EXTS.contains(&format),
    }
}

/// `POST /api/batch` 应答：这次排进队列几本、跳过几本（已在队列里的同名书、点名了但不适用的）。
#[derive(Serialize, Debug, PartialEq)]
pub struct Enqueued {
    pub queued: usize,
    pub skipped: usize,
}

/// `POST /api/batch/stop` 应答：清掉了几本还没开始的。
#[derive(Serialize)]
struct Stopped {
    cleared: usize,
}

/// 到 book-serve 的客户端。每次调用（入队、恢复、处理一本书）现建一个、这一次里共用：每次请求按注册表现查地址，
/// 服务重启换了端口也能找到；没注册时报"book-serve 未运行"。
fn book_serve(paths: &Paths) -> SvcClient {
    SvcClient::new(paths.clone(), "book-serve", BOOK_SERVE_TIMEOUT_SECS)
}
/// 对 book-serve 单个请求的上限：列表、提交加入都是零耗时操作（加入本身在 book-serve 后台线程里跑）。
const BOOK_SERVE_TIMEOUT_SECS: u64 = 30;

fn staging_items(c: &SvcClient) -> Result<Vec<Value>, String> {
    let list = c.get_json("/staging")?;
    Ok(list.get("items").and_then(|v| v.as_array()).cloned().unwrap_or_default())
}

/// 母版库列表（`/staging` 的 `items` 数组）里名为 `name` 的条目。
fn find_item<'a>(items: &'a [Value], name: &str) -> Option<&'a Value> {
    items.iter().find(|it| it.get("name").and_then(|v| v.as_str()) == Some(name))
}

/// 入队。`names=None` 表示"母版库里所有该动作适用的书"；`Some` 只处理点名的。已经在队列里/正在处理的同名书跳过
/// （不重复排）；点名了但不适用的（如 CBZ）计入 skipped。
pub fn enqueue(paths: &Paths, action: Action, names: Option<Vec<String>>, folder: &str) -> Result<Enqueued, String> {
    let items = staging_items(&book_serve(paths))?;
    let wanted: Vec<String> = match &names {
        Some(n) => n.clone(),
        None => items.iter().filter_map(|it| it.get("name").and_then(|v| v.as_str()).map(str::to_string)).collect(),
    };
    let mut queued = 0usize;
    let mut skipped = 0usize;
    let spawn = {
        let mut st = lock();
        for name in wanted {
            let ok = find_item(&items, &name).is_some_and(|it| eligible(action, it));
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
            start_round(&mut st, queued);
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

/// `POST /api/batch {action, names?, all?, folder?}`：`names` 缺省且 `all:true` 表示"母版库里所有适用的"。
pub fn submit(paths: &Paths, req: &mut Request<'_>) -> ApiResult {
    let j = req.json()?;
    let action = j.opt_str("action").and_then(Action::parse).ok_or_else(|| ApiError::bad("action 只能是 deliver"))?;
    // 区分"没给 names"（看 all）与"给了数组"：只有是数组才算给了。
    let names = j.opt_str_list("names");
    if names.is_none() && !j.bool_or("all", false) {
        return Err(ApiError::bad("要么给 names，要么 all:true"));
    }
    let e = enqueue(paths, action, names, j.str_or("folder", "")).map_err(ApiError::bad)?;
    Ok(Reply::ok(&e))
}

/// `POST /api/batch/stop`：全部中止，见 [`stop`]。
pub fn stop_route(paths: &Paths, _req: &mut Request<'_>) -> ApiResult {
    Ok(Reply::ok(&Stopped { cleared: stop(paths) }))
}

/// 记入这次新入队的 `queued` 本。一轮批量从空闲开始才重置计数（已经在跑的时候追加，累加进同一轮）；重置时**队列里
/// 可能还留着上一轮没跑的**（[`resume`] 等不到 book-serve 放弃时队列保留在内存里、没有 worker），它们会跟这次一起跑，
/// 所以总数按"重置后队列里实际有多少本"算——此前直接清零再加 `queued`，进度会显示成"5/2"。
fn start_round(st: &mut State, queued: usize) {
    if !st.worker_alive {
        st.done = 0;
        st.failed.clear();
        st.total = (st.queue.len() - queued) as u32;
    }
    st.total += queued as u32;
}

/// `GET /api/batch/status` 应答（任何会话都能看）。
#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// 有 worker 在处理（或 [`resume`] 恢复出来的队列在等 book-serve）。
    running: bool,
    /// 网关刚重启、恢复出来的队列在等 book-serve 起来。
    waiting_service: bool,
    /// 正在处理/排队那本的动作；都没有但有上一轮结果时是 `deliver`（动作只剩这一种，原来另存一份"最近动作"，跟当前任务里的
    /// 重复，2026-10-07 删掉）；从没跑过是 `null`。
    action: Option<Action>,
    total: u32,
    done: u32,
    current: Option<String>,
    /// 排队中的书名（至多 200 个，网页只拿来标"排队中"）。
    queued: Vec<String>,
    failed: Vec<crate::wire::Failed>,
}

pub fn status() -> Status {
    let st = lock();
    Status {
        running: st.worker_alive,
        waiting_service: st.waiting_service,
        action: st.current.as_ref().or(st.queue.front()).map(|j| j.action).or((st.total > 0).then_some(Action::Deliver)),
        total: st.total,
        done: st.done,
        current: st.current.as_ref().map(|j| j.name.clone()),
        queued: st.queue.iter().take(200).map(|j| j.name.clone()).collect(),
        failed: st.failed.iter().map(|(n, m)| crate::wire::Failed::new(n, m)).collect(),
    }
}

/// **全部中止**：清空还没开始的；正在处理的那一本如果还没交给 `book-serve` 就不再提交，已经交出去的会自然跑完
/// （整本上传 / 大文件通道都没有安全的中断点）。返回被清掉的数量。
pub fn stop(paths: &Paths) -> usize {
    let n = {
        let mut st = lock();
        let n = st.queue.len();
        st.queue.clear();
        // 被清掉的不会再处理，总数同步扣掉，否则停止后进度还显示"1/4"（其实只有 1 本要做）。
        st.total = st.total.saturating_sub(n as u32);
        st.abort_current = st.current.is_some(); // 当前这本若还没交给 book-serve，run_one 看到它就不再提交
        n
    };
    persist(paths);
    n
}

/// 上次进程退出时还"进行中"的那一本放回队首，重新校验后从头再来。原来同一本中断两次就记失败不再重放（防"这本书让网关
/// 崩溃、每次被拉起都先重放它再崩"）：网关这边只转发和轮询、不读书的内容，读书出错只会在 book-serve 里，2026-10-07 删掉。
fn recover_interrupted(saved: &mut State) {
    if let Some(cur) = saved.current.take() {
        saved.queue.push_front(cur);
    }
}

/// 网关启动时调用：读回上次没跑完的队列继续跑。**后台线程里等 `book-serve` 就绪**（开机/整体重启时网关可能先起），
/// 再按最新母版库状态重新校验每一本——上次进行中的那本如果已经不在母版库，就不再做。
pub fn resume(paths: &Paths) {
    let Ok(text) = std::fs::read_to_string(file_of(paths)) else { return };
    // 文件损坏/不是 JSON：当作没有未完成的队列（下次落盘覆盖）。
    let Ok(mut saved) = serde_json::from_str::<State>(&text) else { return };
    recover_interrupted(&mut saved);
    if saved.queue.is_empty() {
        // 没有未完成的：只把"上次结果"（完成数/失败原因）读回来供界面展示。
        *lock() = saved;
        return;
    }
    // 先把读回的队列装进内存（`worker_alive=true` 表示"有 worker 会来处理它"）：状态页立刻能看到排队项，
    // 等待 book-serve 期间用户再提交新批量也会并进这一份（不会 spawn 第二个 worker，也不会覆盖它）。
    // 此前是 120 秒等不到 book-serve 就直接 return，队列既没进内存、也没人再管，随后任何一次入队的
    // `persist` 都会把磁盘上这份未完成队列覆盖掉——开机时 book-serve 起得慢就会丢整个队列。
    {
        let mut st = lock();
        *st = saved;
        st.worker_alive = true;
        st.waiting_service = true;
    }
    let paths = paths.clone();
    std::thread::spawn(move || {
        let items = wait_for_book_serve(&paths);
        lock().waiting_service = false;
        match items {
            Some(items) => validate_queue(&mut lock(), &items),
            // 等了 RESUME_WAIT_MAX 仍没有 book-serve：队列**保留**在内存和磁盘上（不清空、不丢），只是不再有人主动跑；
            // 下一次入队会带起 worker 连同这份旧队列一起处理，或用户在页面点"全部中止"清掉。
            None => {
                lock().worker_alive = false;
                eprintln!("[gateway] 批量队列恢复：等了 {} 分钟 book-serve 仍不可用，队列已保留，待下次入队时继续", RESUME_WAIT_MAX.as_secs() / 60);
                persist(&paths);
                return;
            }
        }
        persist(&paths);
        worker(&paths); // 队列被 stop 清空则 worker 一进来就结束
    });
}

/// 等 book-serve 就绪的上限（见 [`resume`]）。
const RESUME_WAIT_MAX: Duration = Duration::from_secs(30 * 60);
/// 等 book-serve 时注册表没动静的兜底重试间隔（服务注册了但还没开始接请求、inotify 不可用时靠它）。
const RESUME_RETRY: Duration = Duration::from_secs(30);

/// 等到能从 book-serve 取到母版库列表，至多 [`RESUME_WAIT_MAX`]。**等注册表变化**（rmsvc-core 的 `registry_wake`，
/// book-serve 起来时会注册）而不是定时轮询——此前前 30 秒每 2 秒、之后每 30 秒探一次。
fn wait_for_book_serve(paths: &Paths) -> Option<Vec<Value>> {
    let wake = rmsvc_core::events::registry_wake(paths);
    let c = book_serve(paths);
    let deadline = Instant::now() + RESUME_WAIT_MAX;
    loop {
        let seen = wake.generation();
        if let Ok(items) = staging_items(&c) {
            return Some(items);
        }
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return None;
        }
        wake.wait_change(seen, RESUME_RETRY.min(left));
    }
}

/// 按最新母版库状态重新校验队列：不存在/已不适用的项剔除，总数同步扣减。
fn validate_queue(st: &mut State, items: &[Value]) {
    let before = st.queue.len();
    st.queue.retain(|j| find_item(items, &j.name).is_some_and(|it| eligible(j.action, it)));
    let dropped = (before - st.queue.len()) as u32;
    st.total = st.total.saturating_sub(dropped);
}

fn worker(paths: &Paths) {
    loop {
        let job = {
            let mut st = lock();
            match st.queue.pop_front() {
                Some(j) => {
                    st.current = Some(j.clone());
                    st.abort_current = false;
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
        // release 已是 panic=unwind（见 Cargo.toml），这里能真正兜住 run_one 内的 panic，只让这一本失败。
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

/// 这本书这次处理的结论，看等待结束时最后一次取到的母版库列表里它的 `delivered.<kind>`：`ok` → 成功；`failed` → 带
/// book-serve 写的原因失败。其余（没等到结果：等满一小时、book-serve 连续查不到、条目途中被删或改名、状态还是 `pending`）
/// 一律算失败并说明——此前只认 `failed`，这几种结果不明的情况都被记成成功。
fn outcome(last: Option<&Value>, name: &str, kind: &str) -> Result<(), String> {
    let unknown = || Err("没等到结果（书架服务超时、不可达，或这本书已不在母版库），请到 xochitl 书库里核对".to_string());
    let Some(items) = last.and_then(|l| l.get("items")).and_then(|v| v.as_array()) else { return unknown() };
    let Some(c) = find_item(items, name).and_then(|it| it.get("delivered")).and_then(|d| d.get(kind)) else { return unknown() };
    match c.get("status").and_then(|v| v.as_str()) {
        Some("ok") => Ok(()),
        Some("failed") => Err(c.get("message").and_then(|m| m.as_str()).unwrap_or("加入失败").to_string()),
        _ => unknown(),
    }
}

/// 「全部中止」落在当前这本已经出队、但还没真正交给 book-serve 的时候，这本记的失败原因。
const ABORTED: &str = "已全部中止";

/// [`stop`] 是否要求中止当前这一本（worker 取下一本时复位）。
fn abort_requested() -> bool {
    lock().abort_current
}

fn run_one(paths: &Paths, job: &Job) -> Result<(), String> {
    let (path, body) = match job.action {
        Action::Deliver => ("/staging/deliver", json!({"name": job.name, "folder": job.folder})),
    };
    // 「全部中止」若落在出队之后、交给 book-serve 之前：不在这里拦一下，这本书会照样整本投完。
    if abort_requested() {
        return Err(ABORTED.into());
    }
    let c = book_serve(paths);
    // 失败原因只取对方错误体里的 message（不带 "book-serve POST /x:" 前缀）；没注册时是"book-serve 未运行"。
    c.try_post_json(path, &body).map_err(|e| e.message)?;
    let last = poll_until_settled(&c, &job.name);
    outcome(last.as_ref(), &job.name, job.action.key())
}

// ───────────── 等 book-serve 把这本处理完 ─────────────

/// 等"book-serve 有新事件"的兜底超时：正常靠 [`crate::events::books_wake`] 事件唤醒（忙态结束 book-serve 会发 `books`
/// 事件），这里只防事件丢了/订阅线程重连空窗。
const POLL_FALLBACK: Duration = Duration::from_secs(30);
/// 等一本书处理完的上限：服务崩溃/重启导致侦测不到结果时，超时后如实放弃、接着处理下一本。
const SETTLE_POLL_TIMEOUT: Duration = Duration::from_secs(60 * 60);
/// 两次 `GET /staging` 之间的最短间隔：book-serve 每有母版库事件就发一条 `staging`，每条都整表查一次就是持续的
/// loopback 请求 + 整表序列化。代价是下一本最多晚这么久才开始，与整本处理时长相比可忽略。
const MIN_REQUERY: Duration = Duration::from_secs(5);
/// 连续几次查询失败才认定"服务已不可达、任务没了"。
const MAX_POLL_FAILURES: u32 = 6;
/// 查询失败后至多隔多久重试（服务卡住时事件多半不来，不能只等事件）。
const FAILURE_RETRY: Duration = Duration::from_secs(5);

/// 查 book-serve 的 `/staging`（直连后端服务，不经网关自己这层转发）直到 [`is_settled`] 判定这本书已经不再忙，
/// 或等到 [`SETTLE_POLL_TIMEOUT`] 放弃。**事件驱动**：每次查完就阻塞等 [`crate::events::books_wake`]，至多 [`POLL_FALLBACK`]
/// 兜底一次；先取代数再查，查询期间到达的事件不会漏。查询要**连续** [`MAX_POLL_FAILURES`] 次失败才放弃：book-serve 忙着
/// 上传大书时查询超时最容易撞上，一次失败就放弃会让下一本在这本还没投完时就开始。返回最后一次取到的列表
/// （判定处理完的那一份；放弃时是最后一次成功取到的，可能还显示忙），结论由 [`outcome`] 从它读，不再另取一次。
fn poll_until_settled(client: &SvcClient, name: &str) -> Option<Value> {
    let wake = crate::events::books_wake();
    wait_settled(|| client.get_json("/staging"), name, Instant::now() + SETTLE_POLL_TIMEOUT, || wake.generation(), |seen, d| throttled_wait(wake, seen, d, MIN_REQUERY))
}

/// 这本书是不是已经不再忙，输入是 `GET /staging` 原样返回的 JSON（`{"items":[...]}`）。条目还在且 `busy==false`、或条目已经
/// 不在列表里（被删或改名，不能死等一个永远不会再出现的 `busy:false`）都算处理完；解析不了也当处理完，不让侦测本身出错把队列卡住。
fn is_settled(list_json: &Value, name: &str) -> bool {
    let Some(items) = list_json.get("items").and_then(|v| v.as_array()) else { return true };
    find_item(items, name).is_none_or(|it| !it.get("busy").and_then(|v| v.as_bool()).unwrap_or(false))
}

/// 等 `wake` 的代数离开 `seen`（至多 `max`），但从调用起**至少**过 `min.min(max)` 才返回——事件再密也不会让调用方
/// 比这更频繁地去查。
fn throttled_wait(wake: &crate::events::Wake, seen: u64, max: Duration, min: Duration) {
    let start = Instant::now();
    wake.wait_change(seen, max);
    if let Some(rest) = min.min(max).checked_sub(start.elapsed()) {
        std::thread::sleep(rest);
    }
}

/// [`poll_until_settled`] 的循环本体，查询/代数/等待都由调用方注入，便于离线测试。返回最后一次成功取到的列表。
fn wait_settled(mut query: impl FnMut() -> Result<Value, String>, name: &str, deadline: Instant, generation: impl Fn() -> u64, wait: impl Fn(u64, Duration)) -> Option<Value> {
    let mut failures = 0u32;
    let mut last = None;
    loop {
        let seen = generation();
        let failed = match query() {
            Ok(json) if is_settled(&json, name) => return Some(json),
            Ok(json) => {
                last = Some(json); // 还在忙，继续轮询
                false
            }
            Err(_) => true,
        };
        failures = if failed { failures + 1 } else { 0 };
        if failures >= MAX_POLL_FAILURES || Instant::now() >= deadline {
            return last;
        }
        let left = deadline.saturating_duration_since(Instant::now());
        if failed {
            wait(generation(), FAILURE_RETRY.min(left)); // 等下一次事件，至多 FAILURE_RETRY
        } else {
            wait(seen, POLL_FALLBACK.min(left));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(format: &str) -> Value {
        json!({"name": "x", "format": format})
    }

    #[test]
    fn eligibility_matches_selection_bar_buttons() {
        assert!(eligible(Action::Deliver, &item("epub")));
        assert!(eligible(Action::Deliver, &item("pdf")));
        assert!(!eligible(Action::Deliver, &item("cbz")), "xochitl 只收 EPUB/PDF");
        assert!(!eligible(Action::Deliver, &item("other")), "book-serve 把格式收窄前留下的文件报成 other");
    }

    #[test]
    fn action_parse_roundtrip() {
        assert_eq!(Action::parse(Action::Deliver.key()), Some(Action::Deliver));
        assert_eq!(Action::parse("nope"), None);
        assert_eq!(Action::parse("koreader"), None, "批量加入 KOReader 随 2026-09-29 卸载 KOReader 撤掉");
        assert_eq!(Action::parse("optimize"), None, "批量优化随 2026-10-07 书架不再优化书撤掉");
    }

    /// 旧版落盘文件里的 `attempts`、`action` 字段读回时忽略；坏文件读不出来。
    #[test]
    fn saved_state_tolerates_old_fields() {
        let st: State = serde_json::from_str(r#"{"queue":[{"action":"deliver","name":"a.epub","folder":"","attempts":1}],"current":null,"total":1,"done":0,"failed":[],"action":"deliver"}"#).unwrap();
        assert_eq!((st.queue.len(), st.total), (1, 1));
        assert!(serde_json::from_str::<State>("{坏").is_err());
    }

    #[test]
    fn state_roundtrips_through_json_and_skips_worker_flag() {
        let mut st = State::default();
        st.queue.push_back(Job { action: Action::Deliver, name: "a.epub".into(), folder: "乱马1/2".into() });
        st.current = Some(Job { action: Action::Deliver, name: "b.epub".into(), folder: String::new() });
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
    fn recover_interrupted_puts_current_back_first() {
        let job = |n: &str| Job { action: Action::Deliver, name: n.into(), folder: String::new() };
        let mut st = State { current: Some(job("b.epub")), total: 2, ..Default::default() };
        st.queue.push_back(job("c.epub"));
        recover_interrupted(&mut st);
        assert!(st.current.is_none() && st.failed.is_empty());
        assert_eq!(st.queue.iter().map(|j| j.name.as_str()).collect::<Vec<_>>(), ["b.epub", "c.epub"], "中断的那本排在队首重放");
    }

    #[test]
    fn validate_queue_drops_missing_or_ineligible_and_adjusts_total() {
        let job = |n: &str, a: Action| Job { action: a, name: n.into(), folder: String::new() };
        let mut st = State { total: 3, ..Default::default() };
        st.queue.push_back(job("keep.epub", Action::Deliver));
        st.queue.push_back(job("comic.cbz", Action::Deliver)); // 不是 EPUB/PDF → 不再适用
        st.queue.push_back(job("gone.epub", Action::Deliver)); // 母版库里没了
        let items = vec![json!({"name": "keep.epub", "format": "epub"}), json!({"name": "comic.cbz", "format": "cbz"})];
        validate_queue(&mut st, &items);
        assert_eq!(st.queue.iter().map(|j| j.name.as_str()).collect::<Vec<_>>(), ["keep.epub"]);
        assert_eq!(st.total, 1);
    }

    /// 回归：resume 放弃等待后队列留在内存里（没有 worker），下一次入队开新一轮时总数要把这些遗留项算进去。
    #[test]
    fn new_round_counts_leftover_queue_in_total() {
        let job = |n: &str| Job { action: Action::Deliver, name: n.into(), folder: String::new() };
        let mut st = State { total: 9, done: 7, ..Default::default() };
        st.failed.push(("old".into(), "x".into()));
        st.queue.push_back(job("left1"));
        st.queue.push_back(job("left2"));
        st.queue.push_back(job("new")); // 这次入队的 1 本
        start_round(&mut st, 1);
        assert_eq!((st.total, st.done, st.failed.len()), (3, 0, 0), "遗留 2 本 + 新入队 1 本");
        // 已经在跑时追加：只累加，不重置
        st.worker_alive = true;
        st.done = 1;
        st.queue.push_back(job("more"));
        start_round(&mut st, 1);
        assert_eq!((st.total, st.done), (4, 1));
    }

    /// 提交入口先校验参数，不碰 book-serve：未知/已下线的动作、既没 names 也没 all 都直接 400。
    #[test]
    fn submit_rejects_bad_action_or_missing_scope_before_touching_services() {
        let t = tempfile::tempdir().unwrap();
        let paths = Paths::sandbox(t.path());
        let call = |body: &[u8]| {
            let mut b: &[u8] = body;
            let mut r = Request { method: rmsvc_core::http::Method::Post, path: "/api/batch".into(), query: Default::default(), params: Default::default(), content_type: "application/json".into(), content_length: None, headers: vec![], body: &mut b };
            submit(&paths, &mut r).map(|_| ()).unwrap_err()
        };
        assert!(call(br#"{"action":"koreader","all":true}"#).message.contains("只能是 deliver"));
        assert!(call(br#"{"action":"optimize","all":true}"#).message.contains("只能是 deliver"), "批量优化已撤");
        assert!(call(br#"{"action":"deliver"}"#).message.contains("names"));
        assert_eq!(call(br#"{"action":"deliver","all":true}"#).status, 400, "参数齐了才去找 book-serve（沙箱里没有，报不可用）");
    }

    #[test]
    fn stop_clears_pending_persists_and_adjusts_total() {
        let _g = rmsvc_core::sync::lock(&GLOBAL_STATE);
        let t = tempfile::tempdir().unwrap();
        let paths = Paths::sandbox(t.path());
        {
            let mut st = lock();
            st.queue.clear();
            st.current = Some(Job { action: Action::Deliver, name: "now".into(), folder: String::new() });
            st.queue.push_back(Job { action: Action::Deliver, name: "a".into(), folder: String::new() });
            st.queue.push_back(Job { action: Action::Deliver, name: "b".into(), folder: String::new() });
            st.total = 3; // 1 本已在做 + 2 本排队
        }
        assert_eq!(stop(&paths), 2);
        assert_eq!(serde_json::to_value(status()).unwrap()["total"], 1, "停止后总数应只剩正在做的那 1 本");
        assert!(abort_requested(), "正在处理的那一本也要标记中止");
        let saved: State = serde_json::from_str(&std::fs::read_to_string(file_of(&paths)).unwrap()).unwrap();
        assert!(saved.queue.is_empty(), "停止后落盘的队列也必须是空的，否则重启会把被中止的又跑起来");
        let mut st = lock();
        st.current = None;
        st.abort_current = false;
    }

    /// 批量测试共用进程级单例 `state()`：动它的测试串行跑，免得互相改掉对方的标志。
    static GLOBAL_STATE: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// 回归：「全部中止」落在当前这本出队之后、交给 book-serve 之前——不能再把它提交出去（此前中止请求对 book-serve
    /// 是空操作，这本书会照样整本跑完）。假 book-serve 只记有没有人连过来。
    #[test]
    fn run_one_does_not_submit_after_stop() {
        let _g = rmsvc_core::sync::lock(&GLOBAL_STATE);
        let t = tempfile::tempdir().unwrap();
        let paths = Paths::sandbox(t.path());
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let info = rmsvc_core::registry::ServiceInfo { name: "book-serve".into(), port: listener.local_addr().unwrap().port(), label: String::new(), version: String::new(), pid: std::process::id(), ui: None };
        let _reg = rmsvc_core::registry::register(&paths, &info).unwrap();
        let job = Job { action: Action::Deliver, name: "stop-race.epub".into(), folder: String::new() };
        lock().abort_current = true;
        assert_eq!(run_one(&paths, &job).unwrap_err(), ABORTED);
        assert!(listener.accept().is_err(), "中止后不该再向 book-serve 提交");
        lock().abort_current = false;
    }

    /// 回归：一次查询失败（book-serve 忙、查询超时）不能提前算处理完，要等它真的不忙。
    #[test]
    fn transient_query_failure_does_not_settle_early() {
        use std::cell::Cell;
        let busy = serde_json::json!({"items": [{"name": "big.epub", "busy": true}]});
        let idle = serde_json::json!({"items": [{"name": "big.epub", "busy": false}]});
        let script: Vec<Result<serde_json::Value, String>> = vec![Ok(busy.clone()), Err("timeout".into()), Err("timeout".into()), Ok(busy), Err("timeout".into()), Ok(idle)];
        let calls = Cell::new(0usize);
        let last = wait_settled(|| { let i = calls.get(); calls.set(i + 1); script[i].clone() }, "big.epub", Instant::now() + Duration::from_secs(3600), || 0, |_, _| {});
        assert_eq!(last.as_ref().and_then(|l| find_item(l["items"].as_array().unwrap(), "big.epub")).map(|it| it["busy"].clone()), Some(json!(false)), "返回判定处理完的那一份列表");
        assert_eq!(calls.get(), 6, "应一直等到真正不忙（第 6 次查询）才返回");
    }

    /// 事件密集（比如每秒一条）也不会让等待方查得比 `min` 更勤；没有事件时照旧等到 `max`（`max` 比 `min` 短时以 `max` 为准）。
    #[test]
    fn throttled_wait_never_returns_before_min_interval() {
        let w = std::sync::Arc::new(crate::events::Wake::default());
        let seen = w.generation();
        w.bump(); // 事件早就到了：wait_change 立即返回，但仍要等满 min
        let t = Instant::now();
        throttled_wait(&w, seen, Duration::from_secs(5), Duration::from_millis(150));
        let el = t.elapsed();
        assert!(el >= Duration::from_millis(150) && el < Duration::from_secs(3), "{el:?}");
        let seen = w.generation();
        let t = Instant::now();
        throttled_wait(&w, seen, Duration::from_millis(60), Duration::from_secs(5));
        let el = t.elapsed();
        assert!(el >= Duration::from_millis(60) && el < Duration::from_secs(3), "无事件：max 更短时以 max 为准，{el:?}");
    }

    /// 连续失败到上限 → 放弃（服务真挂了，任务已随之消失）。
    #[test]
    fn persistent_failure_gives_up_after_limit() {
        use std::cell::Cell;
        let calls = Cell::new(0u32);
        assert!(wait_settled(|| { calls.set(calls.get() + 1); Err("down".into()) }, "x", Instant::now() + Duration::from_secs(3600), || 0, |_, _| {}).is_none());
        assert_eq!(calls.get(), MAX_POLL_FAILURES);
    }

    /// 结论只认 `ok`；`failed` 带原因；`pending`、条目不在、没取到列表都算"没等到结果"的失败（此前都记成成功）。
    #[test]
    fn outcome_only_ok_counts_as_success() {
        let list = |status: &str| json!({"items": [{"name": "a.epub", "busy": false, "delivered": {"deliver": {"status": status, "message": "上传失败: boom"}}}]});
        assert_eq!(outcome(Some(&list("ok")), "a.epub", "deliver"), Ok(()));
        assert_eq!(outcome(Some(&list("failed")), "a.epub", "deliver"), Err("上传失败: boom".into()));
        for unknown in [outcome(Some(&list("pending")), "a.epub", "deliver"), outcome(Some(&list("ok")), "gone.epub", "deliver"), outcome(None, "a.epub", "deliver")] {
            assert!(unknown.unwrap_err().contains("没等到结果"));
        }
    }

    /// 状态里的 `action`：有当前/排队任务取它的；只剩上一轮结果时是 deliver；从没跑过是 null。
    #[test]
    fn status_action_is_derived_and_waiting_flag_reported() {
        let _g = rmsvc_core::sync::lock(&GLOBAL_STATE);
        let saved = std::mem::take(&mut *lock());
        let status = || serde_json::to_value(status()).unwrap();
        assert!(status()["action"].is_null() && status()["waitingService"] == json!(false));
        lock().total = 2;
        assert_eq!(status()["action"], "deliver");
        lock().waiting_service = true;
        assert_eq!(status()["waitingService"], true);
        assert!(status().get("queuedCount").is_none());
        *lock() = saved;
    }

    /// 线上格式快照（2026-10-10，GW-1）：`GET /api/batch/status`、`POST /api/batch/stop` 的字段，网页母版库底栏读它们。
    #[test]
    fn wire_snapshot_status_and_stop() {
        let _g = rmsvc_core::sync::lock(&GLOBAL_STATE);
        let saved = std::mem::take(&mut *lock());
        {
            let mut st = lock();
            st.current = Some(Job { action: Action::Deliver, name: "b.epub".into(), folder: String::new() });
            st.queue.push_back(Job { action: Action::Deliver, name: "c.epub".into(), folder: "漫画".into() });
            st.total = 3;
            st.done = 1;
            st.failed.push(("a.epub".into(), "boom".into()));
            st.worker_alive = true;
        }
        assert_eq!(
            serde_json::to_value(status()).unwrap(),
            json!({"running": true, "waitingService": false, "action": "deliver", "total": 3, "done": 1, "current": "b.epub", "queued": ["c.epub"], "failed": [{"name": "a.epub", "message": "boom"}]})
        );
        let t = tempfile::tempdir().unwrap();
        let paths = Paths::sandbox(t.path());
        let mut b: &[u8] = b"";
        let mut r = Request { method: rmsvc_core::http::Method::Post, path: "/api/batch/stop".into(), query: Default::default(), params: Default::default(), content_type: String::new(), content_length: None, headers: vec![], body: &mut b };
        let rep = stop_route(&paths, &mut r).unwrap();
        assert_eq!(serde_json::from_slice::<Value>(&rep.body).unwrap(), json!({"cleared": 1}));
        assert_eq!(serde_json::to_value(Enqueued { queued: 2, skipped: 1 }).unwrap(), json!({"queued": 2, "skipped": 1}), "POST /api/batch 应答");
        *lock() = saved;
    }
}
