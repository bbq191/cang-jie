//! book-serve —— 书架·原生投递（loopback 8790）。
//! 上传（经网关 `POST /api/books?target=native|annot`）→ 目标 Strategy → 处理链 → xochitl 书库；
//! 自有 spool（XDG state）+ inotify 追平（scp 丢进 inbox 也能进库）。不读写旧项目任何路径。
mod api;
mod config;
mod pipeline;
mod service_state;
mod spool;
mod target;

use shelf_core::paths::Paths;
use shelf_core::service::{self, ServiceSpec};
use std::sync::Arc;

const SPEC: ServiceSpec = ServiceSpec {
    name: "book-serve",
    label: "原生投递",
    version: env!("CARGO_PKG_VERSION"),
    default_bind: "127.0.0.1:8790",
    tab: Some(("传书", 10)),
};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let bind = service::parse_bind(&args, SPEC.default_bind);
    let paths = Paths::from_env();
    if let Err(e) = paths.ensure() {
        eprintln!("[book-serve] 建目录失败: {e}");
        std::process::exit(1);
    }
    let st = Arc::new(service_state::State::new(&paths));
    if let Err(e) = st.spool.ensure() {
        eprintln!("[book-serve] spool 建目录失败: {e}");
        std::process::exit(1);
    }
    let n = st.spool.recover_orphans();
    if n > 0 {
        println!("[book-serve] 恢复 {n} 个上次未完成的文件回 inbox");
    }
    // 追平线程：启动先扫一遍，再 inotify 防抖等待。
    {
        let st = st.clone();
        std::thread::spawn(move || {
            st.process_inbox(None);
            let inbox = st.spool.inbox();
            shelf_core::fswatch::watch_debounced(&inbox, std::time::Duration::from_secs(8), |_| {
                st.process_inbox(None);
            });
        });
    }
    println!("[book-serve] 目标：{:?}；书库文件夹 {:?}；xochitl {}", st.targets.ids(), st.cfg.library_folder, st.cfg.xochitl_host);
    if let Err(e) = service::run(&SPEC, &bind, &paths, api::router(st)) {
        eprintln!("[book-serve] {e}");
        std::process::exit(1);
    }
}
