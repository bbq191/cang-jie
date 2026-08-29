//! 离线：读一个 .rm 的 RootText（打字文本）打印出来。验生词本/卡片内容用。
//! 用法：cargo run --release --example dump_rm -- <path.rm>
fn main() {
    let p = std::env::args().nth(1).expect("用法: dump_rm <path.rm>");
    let bytes = std::fs::read(&p).expect("读 .rm");
    println!("{}", device_core::notebook_rm::read_root_text(&bytes));
}
