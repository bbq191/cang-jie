//! **设备端一键自检**（2026-10-14）：在 xochitl 书库里真走一遍书架依赖的代理链路，把"只能在设备旁手测"的几步变成一次点击。
//!
//! 五步，依次做（前一步失败，后面依赖它的步骤记"跳过"，但清理照做）：
//! 1. `mkdir-root`：在书库根下建一个临时文件夹「书架自检 <时间戳>」（建文件夹代理 `shelf-mkdir-agent.qmd`，上级为空）；
//! 2. `mkdir-child`：在它里面再建「子文件夹」——`createCollection` 带**非空上级**，网页多级加入位置与 sheng-ren 多级导入都靠它，
//!    此前没在真机上走过（白皮书待测清单 #24、#29）；
//! 3. `upload-claim`：往子文件夹里上传一本一页的测试书并认领（`XochitlDelivery::upload_and_claim`，与落库同一条路）；
//! 4. `render`：等 xochitl 排出页数（同渲染自检）；
//! 5. `trash`：把测试书、子文件夹、临时文件夹一起交给回收站代理（`shelf-trash-agent.qmd`），等它们都进回收站。
//!    把**文件夹**移进回收站此前也没在真机上验证过（之前只移过文档）。
//!
//! 结果：`$XDG_STATE_HOME/shelf/books/selftest.json` 存最近 [`KEEP`] 次；进行中的那次只在内存里（book-serve 重启就没了）。
//! 清不干净的（某一步卡住）列进 `leftovers`，不自动重试、不去碰别的条目——交给人在设备上处理。
//! 只在用户点「运行自检」时跑，不定时；排进与落库、直接导入共用的后台作业队列（`jobs.rs`），不会和真实投递同时往 xochitl 传。
use crate::delivery::XochitlDelivery;
use crate::trash::TrashQueue;
use rmsvc_core::clock::now_secs;
use rmsvc_core::fswatch::wait_for;
use rmsvc_core::sync::lock;
use rmsvc_core::xochitl::{self, Folder};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// 临时文件夹名前缀；测试书的书名也以它开头（清不干净时按它在书库里找）。
pub const PREFIX: &str = "书架自检";
pub const CHILD: &str = "子文件夹";
/// 保留几次结果。
pub const KEEP: usize = 10;

/// 各步的等待上限。建文件夹每一级另有投递层自己的上限（`delivery::FOLDER_WAIT_TIMEOUT`）。
#[derive(Clone, Copy)]
pub struct Waits {
    pub render: Duration,
    pub trash: Duration,
    pub debounce: Duration,
}

pub const WAITS: Waits = Waits { render: Duration::from_secs(120), trash: Duration::from_secs(90), debounce: Duration::from_secs(1) };

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    /// `mkdir-root` / `mkdir-child` / `upload-claim` / `render` / `trash`。
    pub key: String,
    /// `ok` / `fail` / `skip`（前置步骤失败，没做）。
    pub state: String,
    pub ms: u64,
    /// 失败原因或补充（页数等）。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub detail: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Run {
    pub at: u64,
    /// 还在跑的那一步（做完为空）。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub current: String,
    pub done: bool,
    /// 五步都 ok、也没有清不掉的。
    pub ok: bool,
    pub steps: Vec<Step>,
    /// 没进回收站的条目名（含之前几次自检留下的）。
    pub leftovers: Vec<String>,
}

#[derive(Serialize, Deserialize, Default)]
struct Saved {
    runs: Vec<Run>,
}

pub struct SelfTest {
    file: PathBuf,
    /// 测试书的临时文件（上传完就删）。
    probe: PathBuf,
    delivery: Arc<XochitlDelivery>,
    trash: Arc<TrashQueue>,
    running: Mutex<Option<Run>>,
    waits: Waits,
}

impl SelfTest {
    pub fn new(state_books_dir: &Path, delivery: Arc<XochitlDelivery>, trash: Arc<TrashQueue>) -> SelfTest {
        SelfTest { file: state_books_dir.join("selftest.json"), probe: state_books_dir.join("selftest-probe.epub"), delivery, trash, running: Mutex::new(None), waits: WAITS }
    }

    #[cfg(test)]
    fn with_waits(mut self, w: Waits) -> SelfTest {
        self.waits = w;
        self
    }

    /// `GET /selftest`：`{running: Run|null, runs: [最近几次，新的在前]}`。
    pub fn status(&self) -> serde_json::Value {
        let saved: Saved = rmsvc_core::config::load_or_default(&self.file);
        serde_json::json!({"running": lock(&self.running).clone(), "runs": saved.runs})
    }

    /// `POST /selftest`：排进作业队列。已经在跑 → `Err`（409）。
    pub fn start(self: &Arc<Self>, jobs: &crate::jobs::Jobs) -> Result<(), crate::error::Error> {
        {
            let mut r = lock(&self.running);
            if r.is_some() {
                return Err(crate::error::Error::Conflict("自检正在进行".into()));
            }
            *r = Some(Run { at: now_secs(), current: "queued".into(), ..Default::default() });
        }
        let me = self.clone();
        let queued = jobs.run(Box::new(move || {
            let run = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| me.execute())).unwrap_or_else(|_| Run {
                at: now_secs(),
                done: true,
                steps: vec![Step { key: "internal".into(), state: "fail".into(), ms: 0, detail: "自检内部异常（已捕获）".into() }],
                ..Default::default()
            });
            me.save(&run);
            *lock(&me.running) = None;
        }));
        if let Err(e) = queued {
            *lock(&self.running) = None;
            return Err(crate::error::Error::Io(e));
        }
        Ok(())
    }

    fn save(&self, run: &Run) {
        let mut saved: Saved = rmsvc_core::config::load_or_default(&self.file);
        saved.runs.insert(0, run.clone());
        saved.runs.truncate(KEEP);
        if let Err(e) = rmsvc_core::config::save(&self.file, &saved, None) {
            println!("[book-serve] 自检结果没存上: {e}");
        }
        println!("[book-serve] 自检{}：{}", if run.ok { "通过" } else { "没通过" }, run.steps.iter().map(|s| format!("{}={}", s.key, s.state)).collect::<Vec<_>>().join(" "));
    }

    /// 更新进行中的那次（网页轮询 `GET /selftest` 看到）。
    fn publish(&self, run: &Run) {
        if let Some(r) = lock(&self.running).as_mut() {
            *r = run.clone();
        }
    }

    fn execute(&self) -> Run {
        let lib = self.delivery.library_dir().to_path_buf();
        let at = now_secs();
        let root = format!("{PREFIX} {at}");
        let mut run = Run { at, ..Default::default() };
        // 建出来的条目（uuid, 书库里的名字），最后统一交给回收站
        let mut made: Vec<(String, String)> = Vec::new();

        let root_id = self.step(&mut run, "mkdir-root", true, || match self.delivery.ensure_folder_path(&[&root]) {
            (Folder::Id(id), 1) => Ok((id.as_str().to_string(), String::new())),
            _ => Err("文件夹没建出来（建文件夹代理没载入，或 xochitl 没响应）".into()),
        });
        if let Some(id) = &root_id {
            made.push((id.clone(), root.clone()));
        }
        let child_id = self.step(&mut run, "mkdir-child", root_id.is_some(), || match self.delivery.ensure_folder_path(&[&root, CHILD]) {
            (Folder::Id(id), 2) => Ok((id.as_str().to_string(), String::new())),
            _ => Err("在临时文件夹里建子文件夹没成功（带上级的 createCollection）".into()),
        });
        if let Some(id) = &child_id {
            made.push((id.clone(), CHILD.to_string()));
        }
        let book = self.step(&mut run, "upload-claim", child_id.is_some(), || {
            let folder = Folder::from_parent_str(child_id.as_deref().unwrap_or_default()).ok_or_else(|| "子文件夹 id 不对".to_string())?;
            let epub = shelf_conv::placeholder::text_epub(&format!("{PREFIX}探针 {at}"), "书架自检临时放进来的测试书，自检结束会移进回收站。")?;
            std::fs::write(&self.probe, epub).map_err(|e| format!("写测试书失败: {e}"))?;
            let r = self.delivery.upload_and_claim(&self.probe, &format!("{PREFIX}探针 {at}.epub"), &folder);
            let _ = std::fs::remove_file(&self.probe);
            let c = r.map_err(|e| e.to_string())?;
            Ok((c.uuid, String::new()))
        });
        if let Some(uuid) = &book {
            let name = xochitl::read_meta(&lib, uuid).ok().flatten().map(|m| m.visible_name).unwrap_or_default();
            made.insert(0, (uuid.clone(), name));
        }
        let w = self.waits;
        self.step(&mut run, "render", book.is_some(), || {
            let uuid = book.clone().unwrap_or_default();
            wait_for(&lib, w.debounce, w.render, || xochitl::page_count(&lib, &uuid)).map(|p| (String::new(), format!("{p} 页"))).ok_or_else(|| format!("{} 秒内没排出页数", w.render.as_secs()))
        });
        self.step(&mut run, "trash", !made.is_empty(), || {
            let mut errs = Vec::new();
            for (uuid, name) in &made {
                if let Err(e) = self.trash.add(uuid, name) {
                    errs.push(format!("《{name}》入队失败：{e}"));
                }
            }
            let live = |u: &str| xochitl::read_meta(&lib, u).ok().flatten().is_some_and(|m| m.is_live());
            let all_gone = wait_for(&lib, w.debounce, w.trash, || made.iter().all(|(u, _)| !live(u)).then_some(()));
            match (all_gone, errs.is_empty()) {
                (Some(()), true) => Ok((String::new(), format!("{} 项", made.len()))),
                _ => Err(if errs.is_empty() { format!("{} 秒内没全部进回收站", w.trash.as_secs()) } else { errs.join("；") }),
            }
        });
        run.leftovers = leftovers(&lib);
        run.current.clear();
        run.done = true;
        run.ok = run.steps.iter().all(|s| s.state == "ok") && run.leftovers.is_empty();
        run
    }

    /// 跑一步并记进 `run`；`go` 为假（前置步骤没成）记 skip。成功回 `f` 给的值（`.0`），`.1` 记进 detail。
    fn step(&self, run: &mut Run, key: &str, go: bool, f: impl FnOnce() -> Result<(String, String), String>) -> Option<String> {
        if !go {
            run.steps.push(Step { key: key.into(), state: "skip".into(), ms: 0, detail: String::new() });
            return None;
        }
        run.current = key.into();
        self.publish(run);
        let t = Instant::now();
        let r = f();
        let ms = t.elapsed().as_millis() as u64;
        let (state, detail, out) = match r {
            Ok((v, d)) => ("ok", d, Some(v)),
            Err(e) => ("fail", e, None),
        };
        run.steps.push(Step { key: key.into(), state: state.into(), ms, detail });
        self.publish(run);
        out
    }
}

/// 书库里还活着的自检条目（文件夹与测试书名字都以 [`PREFIX`] 开头；子文件夹跟着它的上级）。
fn leftovers(lib: &Path) -> Vec<String> {
    let live = xochitl::live_entries(lib);
    let roots: Vec<&str> = live.iter().filter(|(_, m)| m.visible_name.starts_with(PREFIX)).map(|(u, _)| u.as_str()).collect();
    let mut out: Vec<String> = live.iter().filter(|(u, m)| roots.contains(&u.as_str()) || roots.contains(&m.parent.as_str())).map(|(_, m)| m.visible_name.clone()).collect();
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::staging::tests::{fake_xochitl, spawn_agent};
    use std::sync::atomic::{AtomicBool, Ordering};

    const MS: fn(u64) -> Duration = Duration::from_millis;

    /// 模拟回收站代理 + xochitl 排版：交来的 uuid 改成 `parent:"trash"`；新上传的 EPUB 补上页数。
    fn spawn_trash_and_render(q: Arc<TrashQueue>, lib: PathBuf, stop: Arc<AtomicBool>, folders_too: bool) {
        std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                for e in std::fs::read_dir(&lib).unwrap().flatten() {
                    let p = e.path();
                    if p.extension().is_some_and(|x| x == "epub") && !p.with_extension("content").exists() {
                        std::fs::write(p.with_extension("content"), r#"{"pageCount":1}"#).unwrap();
                    }
                }
                let (uuids, _) = q.pending_wait(MS(50)).unwrap();
                for u in uuids {
                    let f = lib.join(format!("{u}.metadata"));
                    let mut v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&f).unwrap()).unwrap();
                    if !folders_too && v["type"] == "CollectionType" {
                        continue;
                    }
                    v["parent"] = "trash".into();
                    std::fs::write(&f, v.to_string()).unwrap();
                }
            }
        });
    }

    fn setup(t: &tempfile::TempDir, agents: bool, folders_too: bool) -> (Arc<SelfTest>, PathBuf, Arc<AtomicBool>) {
        let lib = t.path().join("xochitl");
        std::fs::create_dir_all(&lib).unwrap();
        let x = Arc::new(xochitl::Xochitl::new(&fake_xochitl(lib.clone()), &lib, 10));
        let mkdir = Arc::new(crate::mkdir::MkdirQueue::new(t.path(), &lib));
        let d = Arc::new(XochitlDelivery::new(x, mkdir.clone(), 0).with_waits(MS(800), Duration::from_secs(5)));
        let trash = Arc::new(TrashQueue::new(t.path(), &lib));
        let stop = Arc::new(AtomicBool::new(false));
        if agents {
            spawn_agent(mkdir, lib.clone(), false, stop.clone());
            spawn_trash_and_render(trash.clone(), lib.clone(), stop.clone(), folders_too);
        }
        let w = Waits { render: Duration::from_secs(5), trash: MS(1500), debounce: MS(20) };
        (Arc::new(SelfTest::new(t.path(), d, trash).with_waits(w)), lib, stop)
    }

    fn states(r: &Run) -> Vec<(String, String)> {
        r.steps.iter().map(|s| (s.key.clone(), s.state.clone())).collect()
    }

    fn st(v: &[(&str, &str)]) -> Vec<(String, String)> {
        v.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect()
    }

    #[test]
    fn all_steps_pass_with_working_agents_and_result_is_saved() {
        let t = tempfile::tempdir().unwrap();
        let (s, lib, stop) = setup(&t, true, true);
        let jobs = crate::jobs::Jobs::default();
        s.start(&jobs).unwrap();
        assert!(matches!(s.start(&jobs), Err(crate::error::Error::Conflict(_))), "跑着的时候不能再起一次");
        let t0 = Instant::now();
        while !s.status()["running"].is_null() {
            assert!(t0.elapsed() < Duration::from_secs(20));
            std::thread::sleep(MS(20));
        }
        stop.store(true, Ordering::Relaxed);
        let v = s.status();
        let run: Run = serde_json::from_value(v["runs"][0].clone()).unwrap();
        assert_eq!(states(&run), st(&[("mkdir-root", "ok"), ("mkdir-child", "ok"), ("upload-claim", "ok"), ("render", "ok"), ("trash", "ok")]), "{run:?}");
        assert!(run.ok && run.done && run.leftovers.is_empty());
        assert_eq!(run.steps[4].detail, "3 项", "测试书、子文件夹、临时文件夹");
        assert!(!t.path().join("selftest-probe.epub").exists(), "测试书临时文件删掉");
        // 子文件夹建在临时文件夹里
        let folders: Vec<(String, xochitl::Metadata)> = std::fs::read_dir(&lib).unwrap().flatten()
            .filter_map(|e| e.file_name().to_str()?.strip_suffix(".metadata").map(str::to_string))
            .filter_map(|u| xochitl::read_meta(&lib, &u).ok().flatten().map(|m| (u, m)))
            .filter(|(_, m)| m.is_folder())
            .collect();
        let root = folders.iter().find(|(_, m)| m.visible_name.starts_with(PREFIX)).unwrap();
        assert!(folders.iter().any(|(_, m)| m.visible_name == CHILD), "{folders:?}");
        assert_eq!(root.1.parent, "trash");
    }

    #[test]
    fn no_agents_fails_first_step_skips_rest() {
        let t = tempfile::tempdir().unwrap();
        let (s, _lib, _stop) = setup(&t, false, false);
        let run = s.execute();
        assert_eq!(states(&run), st(&[("mkdir-root", "fail"), ("mkdir-child", "skip"), ("upload-claim", "skip"), ("render", "skip"), ("trash", "skip")]));
        assert!(!run.ok && run.steps[0].detail.contains("代理"));
    }

    #[test]
    fn folders_not_trashed_are_reported_as_leftovers() {
        let t = tempfile::tempdir().unwrap();
        let (s, _lib, stop) = setup(&t, true, false);
        let run = s.execute();
        stop.store(true, Ordering::Relaxed);
        assert_eq!(run.steps[4].state, "fail");
        assert!(!run.ok);
        assert_eq!(run.leftovers.len(), 2, "临时文件夹与子文件夹留下了：{:?}", run.leftovers);
        assert!(run.leftovers.contains(&CHILD.to_string()));
    }
}
