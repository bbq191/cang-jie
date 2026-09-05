//! host/设备通用 CLI：CBZ（漫画 zip）→ 固定版式 PDF，与设备端母版库「转 PDF」**同一函数** `convert::cbz::cbz_to_pdf`。
//! 每页按 Move 屏降采样（竖 954×1696 / 横 1696×954，达标即原样直嵌零重编码）；缺省**原图**（JPEG 直嵌、彩页保色），
//! `--mono` 黑白页转 1-bit 抖动（省刷新波形、体积 ~1/8，网点会变抖动点）。`shelf push` 漫画通道用它。
//!
//! 用法: cbz2pdf [--mono] 输入.cbz 输出.pdf    退出码: 0 成功；1 用法错；2 转换失败。

use bookconv::convert::{cbz::cbz_to_pdf, EinkTone};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flags: Vec<&str> = args.iter().filter(|a| a.starts_with("--")).map(|s| s.as_str()).collect();
    let files: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();
    if files.len() != 2 || flags.iter().any(|f| *f != "--mono") {
        eprintln!("用法: cbz2pdf [--mono] 输入.cbz 输出.pdf");
        std::process::exit(1);
    }
    let tone = if flags.contains(&"--mono") { EinkTone::Mono } else { EinkTone::Off };
    let data = match std::fs::read(files[0]) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("读 {}: {e}", files[0]);
            std::process::exit(2);
        }
    };
    match cbz_to_pdf(&data, tone).and_then(|pdf| std::fs::write(files[1], &pdf).map(|_| pdf.len()).map_err(|e| format!("写 {}: {e}", files[1]))) {
        Ok(n) => println!("cbz2pdf: {} → {} 字节（{}）", data.len(), n, if tone == EinkTone::Mono { "1-bit 抖动" } else { "原图" }),
        Err(e) => {
            eprintln!("转换失败: {e}");
            std::process::exit(2);
        }
    }
}
