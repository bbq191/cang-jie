//! 直接导入的**后台任务队列**（2026-10-07）：`POST /import` 收完请求体、做完快速校验就把剩下的活（建文件夹、上传给 xochitl、
//! 等它排版、认领 uuid、登记页边距 / 原地替换）交到这里，立即回 202 + 任务 id；客户端（电脑上的 sheng-ren `booklib sync`）
//! 用 `GET /import/jobs/{id}` 事后查结果。做法同网页「加入 xochitl」的 `Staging::spawn_deliver`（后台线程做、接口立即返回、
//! 结果事后看），区别是结果不落母版库边车，只记在内存里。
//!
//! - **串行**：只有一个工作线程，任务按提交顺序一次做一个（xochitl 的 `/upload` 本来就得一本一本来，认领也靠"上传前后
//!   书库多了哪份"）。排队中的任务 `state` 是 `running`、`stage` 是「排队」。
//! - **只在内存里**：book-serve 重启任务就没了（查询回 404，客户端据此按"不知道结果"处理——下次同步再导入时，目标文件夹里
//!   已有逐字节相同的书会被认回来，不会重复加入，见 `Importer::same_in_folder`）。任务 id 带进程启动时刻，重启后的新任务
//!   不会和旧 id 撞上。
//! - **保留时长**：做完（成功或失败）的任务至少留 [`JOB_KEEP`]，之后在下一次提交 / 查询时清掉。
//! - **panic 兜住**：任务里 panic 由 `catch_unwind` 转成失败记录，工作线程照常做下一个。
use crate::import::DocState;
use rmsvc_core::sync::lock;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

/// 做完的任务在内存里至少留多久（客户端断线重连、查晚了也查得到）。
pub const JOB_KEEP: Duration = Duration::from_secs(60 * 60);

/// 一个导入任务：参数是"报告当前阶段"的回调，成功回文档状况，失败回给人看的一句话。
pub type Task = Box<dyn FnOnce(&dyn Fn(&str)) -> Result<DocState, String> + Send>;

#[derive(Debug, Clone, PartialEq)]
enum Outcome {
    Running,
    Done(DocState),
    Failed(String),
}

struct Job {
    /// 给人看的当前阶段（「排队」「建文件夹」「上传给 xochitl」……）。
    stage: String,
    outcome: Outcome,
    /// 做完的时刻（清理按它算）；还在做 / 排队 → `None`。
    finished: Option<Instant>,
}

type Jobs = Arc<Mutex<HashMap<String, Job>>>;

pub struct ImportJobs {
    jobs: Jobs,
    /// 交给唯一的工作线程；`ImportJobs` 销毁时发送端跟着没了，工作线程做完手头的就退出。
    tx: Mutex<mpsc::Sender<(String, Task)>>,
    /// 任务 id 前缀：进程启动时刻（毫秒，十六进制）。
    boot: String,
    seq: AtomicU64,
    keep: Duration,
}

impl Default for ImportJobs {
    fn default() -> Self {
        Self::new(JOB_KEEP)
    }
}

impl ImportJobs {
    /// 起工作线程。`keep` 是做完的任务保留多久（线上 [`JOB_KEEP`]，单测改短）。
    pub fn new(keep: Duration) -> ImportJobs {
        let jobs: Jobs = Arc::default();
        let (tx, rx) = mpsc::channel::<(String, Task)>();
        let j2 = jobs.clone();
        std::thread::spawn(move || {
            for (id, task) in rx {
                let report = |stage: &str| {
                    if let Some(j) = lock(&j2).get_mut(&id) {
                        j.stage = stage.to_string();
                    }
                };
                report("开始");
                let outcome = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| task(&report))) {
                    Ok(Ok(d)) => Outcome::Done(d),
                    Ok(Err(m)) => Outcome::Failed(m),
                    Err(_) => Outcome::Failed("导入过程内部异常（已捕获，不影响其他任务）".into()),
                };
                if let Some(j) = lock(&j2).get_mut(&id) {
                    j.stage = if matches!(outcome, Outcome::Done(_)) { "完成" } else { "失败" }.to_string();
                    j.outcome = outcome;
                    j.finished = Some(Instant::now());
                }
            }
        });
        ImportJobs { jobs, tx: Mutex::new(tx), boot: format!("{:x}", rmsvc_core::clock::now_ms()), seq: AtomicU64::new(1), keep }
    }

    /// 排进队尾，回任务 id。
    pub fn submit(&self, task: Task) -> String {
        self.prune();
        let id = format!("{}-{}", self.boot, self.seq.fetch_add(1, Ordering::Relaxed));
        lock(&self.jobs).insert(id.clone(), Job { stage: "排队".into(), outcome: Outcome::Running, finished: None });
        if lock(&self.tx).send((id.clone(), task)).is_err() {
            // 工作线程没了（不该发生）：照实记成失败，别让客户端一直等
            if let Some(j) = lock(&self.jobs).get_mut(&id) {
                j.stage = "失败".into();
                j.outcome = Outcome::Failed("导入任务队列已停止，请重启 book-serve".into());
                j.finished = Some(Instant::now());
            }
        }
        id
    }

    /// 查任务：`{job, state: running|done|failed, stage, uuid?, name?, folder?, message?}`（done 带 uuid/name/folder，failed 带
    /// message）。没有这个任务（含已清掉、book-serve 重启过）→ `None`。
    pub fn get(&self, id: &str) -> Option<serde_json::Value> {
        self.prune();
        let jobs = lock(&self.jobs);
        let j = jobs.get(id)?;
        let mut v = serde_json::json!({"job": id, "stage": j.stage});
        match &j.outcome {
            Outcome::Running => v["state"] = "running".into(),
            Outcome::Done(d) => {
                v["state"] = "done".into();
                v["uuid"] = d.uuid.clone().into();
                v["name"] = d.name.clone().into();
                v["folder"] = d.folder.clone().into();
            }
            Outcome::Failed(m) => {
                v["state"] = "failed".into();
                v["message"] = m.clone().into();
            }
        }
        Some(v)
    }

    /// 清掉做完超过 `keep` 的任务。
    fn prune(&self) {
        let keep = self.keep;
        lock(&self.jobs).retain(|_, j| j.finished.is_none_or(|t| t.elapsed() < keep));
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn doc(uuid: &str) -> DocState {
        DocState { uuid: uuid.into(), name: "书".into(), folder: "小说".into(), deleted: false, replacing: false }
    }

    /// 等任务做完（最多 5 秒），回最后一次查询的结果。
    pub(crate) fn wait_done(jobs: &ImportJobs, id: &str) -> serde_json::Value {
        let t0 = Instant::now();
        loop {
            let v = jobs.get(id).expect("任务应该在");
            if v["state"] != "running" || t0.elapsed() > Duration::from_secs(5) {
                return v;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn done_failed_panic_and_unknown() {
        let jobs = ImportJobs::default();
        let ok = jobs.submit(Box::new(|stage| {
            stage("上传给 xochitl");
            Ok(doc("u1"))
        }));
        let bad = jobs.submit(Box::new(|_| Err("上传给 xochitl 失败: 连不上".into())));
        let boom = jobs.submit(Box::new(|_| panic!("boom")));
        assert_eq!(wait_done(&jobs, &ok), serde_json::json!({"job": ok, "state": "done", "stage": "完成", "uuid": "u1", "name": "书", "folder": "小说"}));
        assert_eq!(wait_done(&jobs, &bad), serde_json::json!({"job": bad, "state": "failed", "stage": "失败", "message": "上传给 xochitl 失败: 连不上"}));
        assert_eq!(wait_done(&jobs, &boom)["state"], "failed", "panic 转成失败");
        // panic 之后工作线程照常做下一个
        let after = jobs.submit(Box::new(|_| Ok(doc("u2"))));
        assert_eq!(wait_done(&jobs, &after)["uuid"], "u2");
        assert_eq!(jobs.get("nope"), None);
        assert_ne!(ok, bad, "任务 id 不重复");
    }

    /// 一次一个、按提交顺序；后面的在排队时 state=running、stage=排队；阶段随任务报告变化。
    #[test]
    fn serial_in_submit_order_with_queued_stage() {
        let jobs = ImportJobs::default();
        let (go_tx, go_rx) = mpsc::channel::<()>();
        let order = Arc::new(Mutex::new(Vec::new()));
        let (o1, o2) = (order.clone(), order.clone());
        let first = jobs.submit(Box::new(move |stage| {
            stage("等 xochitl 排版");
            go_rx.recv().unwrap();
            o1.lock().unwrap().push(1);
            Ok(doc("a"))
        }));
        let second = jobs.submit(Box::new(move |_| {
            o2.lock().unwrap().push(2);
            Ok(doc("b"))
        }));
        let t0 = Instant::now();
        while jobs.get(&first).unwrap()["stage"] != "等 xochitl 排版" {
            assert!(t0.elapsed() < Duration::from_secs(5));
            std::thread::sleep(Duration::from_millis(10));
        }
        std::thread::sleep(Duration::from_millis(50));
        let v = jobs.get(&second).unwrap();
        assert_eq!((v["state"].as_str(), v["stage"].as_str()), (Some("running"), Some("排队")), "前一个没做完，后一个不开始");
        assert!(order.lock().unwrap().is_empty());
        go_tx.send(()).unwrap();
        assert_eq!(wait_done(&jobs, &second)["state"], "done");
        assert_eq!(*order.lock().unwrap(), [1, 2]);
    }

    #[test]
    fn finished_jobs_are_dropped_after_keep() {
        let jobs = ImportJobs::new(Duration::from_millis(100));
        let id = jobs.submit(Box::new(|_| Ok(doc("u"))));
        assert_eq!(wait_done(&jobs, &id)["state"], "done");
        std::thread::sleep(Duration::from_millis(150));
        assert_eq!(jobs.get(&id), None, "做完超过保留时长就清掉");
    }
}
