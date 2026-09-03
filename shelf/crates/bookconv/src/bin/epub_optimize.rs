//! host/设备通用 CLI：对一本 EPUB 跑 `optimize::optimize_epub`（与设备端导入优化器**同一函数**）。
//! 用途：host 侧 Calibre 清洗流（`shelf/host/calibre/wash_epub.sh`）末尾叠加设备优化——脚注拆环/
//! duokan 标记/远程图内联/双 id 去重/图片降采样/e-ink 提对比，产物自带 `META-INF/com.cangjie.optimized`
//! 标记，设备 autoopt 不会再优化一遍。
//!
//! 用法: epub-optimize 输入.epub 输出.epub    （输入输出可同路径=就地覆盖，先整本写内存再落盘）
//! 退出码: 0 成功；1 用法错；2 优化失败（输入原样不动）。

use bookconv::optimize;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("用法: epub-optimize 输入.epub 输出.epub");
        std::process::exit(1);
    }
    let epub = match std::fs::read(&args[1]) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("读 {}: {e}", args[1]);
            std::process::exit(2);
        }
    };
    let (out, rep) = match optimize::optimize_epub(&epub) {
        Ok(x) => x,
        Err(e) => {
            eprintln!("优化失败: {e}");
            std::process::exit(2);
        }
    };
    if let Err(e) = std::fs::write(&args[2], &out) {
        eprintln!("写 {}: {e}", args[2]);
        std::process::exit(2);
    }
    println!(
        "epub-optimize v{}: {} 文件/{} 章, {} → {} 字节",
        optimize::OPTIMIZE_VERSION, rep.total_files, rep.html_files, rep.bytes_before, rep.bytes_after
    );
}
