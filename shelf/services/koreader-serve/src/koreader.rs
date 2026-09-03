//! KOReader 安装目录模型（appload 外部应用：`$SHELF_KOREADER_ROOT`，缺省 `~/xovi/exthome/appload/koreader`）。
//! 投书=原字节落 `books/`（不转换不改名——KOReader 原生读 EPUB/PDF/AZW3/MOBI/FB2/CBZ）；
//! 运行态=扫 `/proc/*/cmdline` 含 koreader（改其配置必须在它退出后，退出回写会覆盖）。
use serde::Serialize;
use shelf_core::multipart::safe_basename;
use std::path::{Path, PathBuf};

pub struct KoReader {
    root: PathBuf,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct FileItem {
    pub name: String,
    pub bytes: u64,
}

impl KoReader {
    pub fn new(root: &Path) -> KoReader {
        KoReader { root: root.to_path_buf() }
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn installed(&self) -> bool {
        self.root.join("reader.lua").is_file() || self.root.join("koreader.sh").is_file() || self.root.join("books").is_dir()
    }
    pub fn books_dir(&self) -> PathBuf {
        self.root.join("books")
    }
    pub fn fonts_dir(&self) -> PathBuf {
        self.root.join("fonts")
    }
    pub fn dict_dir(&self) -> PathBuf {
        self.root.join("data/dict")
    }
    /// `git-rev` 文件（KOReader 发行包自带）。
    pub fn version(&self) -> Option<String> {
        std::fs::read_to_string(self.root.join("git-rev")).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
    }
    /// 是否有 koreader 进程在跑（linux：/proc 扫 cmdline）。
    pub fn running(&self) -> bool {
        running_by_proc(Path::new("/proc"))
    }

    /// 校验并规范 folder（单段、无路径穿越；空=根）。
    pub fn subdir(&self, folder: &str) -> Result<PathBuf, String> {
        let f = folder.trim().trim_matches('/');
        if f.is_empty() {
            return Ok(self.books_dir());
        }
        if f.contains("..") || f.contains('/') || f.contains('\\') {
            return Err("folder 只允许单层目录名".into());
        }
        Ok(self.books_dir().join(f))
    }

    /// 把 reader 里的字节原样写进 books/[folder]/<name>（先写 .part 再 rename，KOReader 扫目录不会见半成品）。
    pub fn put_book(&self, folder: &str, filename: &str, reader: &mut dyn std::io::Read) -> Result<FileItem, String> {
        let dir = self.subdir(folder)?;
        std::fs::create_dir_all(&dir).map_err(|e| format!("建目录失败: {e}"))?;
        let name = safe_basename(filename, "book.bin");
        let dest = dir.join(&name);
        let tmp = dir.join(format!(".{name}.part"));
        let n = (|| -> Result<u64, String> {
            let mut f = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;
            std::io::copy(reader, &mut f).map_err(|e| e.to_string())
        })();
        match n {
            Ok(0) => {
                let _ = std::fs::remove_file(&tmp);
                Err("空文件".into())
            }
            Ok(bytes) => {
                std::fs::rename(&tmp, &dest).map_err(|e| format!("落盘失败: {e}"))?;
                Ok(FileItem { name, bytes })
            }
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                Err(format!("接收失败: {e}"))
            }
        }
    }

    pub fn list_dir(&self, dir: &Path, exts: &[&str]) -> Vec<FileItem> {
        let mut v = Vec::new();
        if let Ok(rd) = std::fs::read_dir(dir) {
            for e in rd.flatten() {
                let Ok(md) = e.metadata() else { continue };
                let name = e.file_name().to_string_lossy().to_string();
                if !md.is_file() || name.starts_with('.') {
                    continue;
                }
                let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
                if exts.is_empty() || exts.contains(&ext.as_str()) {
                    v.push(FileItem { name, bytes: md.len() });
                }
            }
        }
        v.sort_by(|a, b| a.name.cmp(&b.name));
        v
    }
}

fn running_by_proc(proc_dir: &Path) -> bool {
    let Ok(rd) = std::fs::read_dir(proc_dir) else { return false };
    let me = std::process::id().to_string();
    for e in rd.flatten() {
        let name = e.file_name();
        let Some(pid) = name.to_str() else { continue };
        if !pid.chars().all(|c| c.is_ascii_digit()) || pid == me {
            continue;
        }
        if let Ok(cmd) = std::fs::read(e.path().join("cmdline")) {
            let s = String::from_utf8_lossy(&cmd);
            if s.contains("koreader") && !s.contains("koreader-serve") {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn put_book_keeps_bytes_and_name() {
        let t = tempfile::tempdir().unwrap();
        let k = KoReader::new(t.path());
        let data = "中文 名.azw3".as_bytes().to_vec();
        let mut r = &data[..];
        let item = k.put_book("", "../../中文 名.azw3", &mut r).unwrap();
        assert_eq!(item.name, "中文 名.azw3");
        assert_eq!(std::fs::read(k.books_dir().join("中文 名.azw3")).unwrap(), data);
        assert!(k.installed());
        let mut r2: &[u8] = b"x";
        assert!(k.put_book("a/b", "x.epub", &mut r2).is_err());
        let mut r3: &[u8] = b"";
        assert_eq!(k.put_book("sub", "e.epub", &mut r3).unwrap_err(), "空文件");
        assert!(!k.books_dir().join("sub/.e.epub.part").exists());
        let mut r4: &[u8] = b"pk";
        k.put_book("sub", "e.epub", &mut r4).unwrap();
        assert_eq!(k.list_dir(&k.subdir("sub").unwrap(), &["epub"]).len(), 1);
    }

    #[test]
    fn running_detection_via_fake_proc() {
        let t = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(t.path().join("123")).unwrap();
        std::fs::write(t.path().join("123/cmdline"), b"./luajit\0/home/root/xovi/exthome/appload/koreader/reader.lua\0").unwrap();
        std::fs::create_dir_all(t.path().join("self")).unwrap();
        assert!(running_by_proc(t.path()));
        std::fs::write(t.path().join("123/cmdline"), b"/home/root/.local/bin/koreader-serve\0serve\0").unwrap();
        assert!(!running_by_proc(t.path()));
    }
}
