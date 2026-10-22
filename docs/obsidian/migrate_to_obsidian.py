#!/usr/bin/env python3
"""把 cang-jie 仓库的文档导出成 Obsidian 知识库里的一个项目目录。

用法：
    python3 docs/obsidian/migrate_to_obsidian.py <Vault 绝对路径> [--subdir cang-jie] [--dry-run] [--force]

以仓库为准、知识库是生成物：仓库里的 Markdown 原样不动（GitHub 上的相对链接照常能点），每次运行都从仓库
重新生成知识库里的整份副本——
  · 按「00-MOC … 99-附件」的目录结构放文件，文件名全库唯一（6 个 README.md 各自改名）；
  · 每篇顶部注入 YAML frontmatter（title/aliases/date/updated/type/line/status/firmware/tags/source/related），
    date/updated 取 git 首次 / 最后提交日期，tags 按类型、所属线和正文关键词生成；
  · 文档之间的相对链接改成 [[双链]]，带锚点的按 GitHub 的锚点规则找回标题，改成 [[笔记#标题]]；
    指向源码、目录、没导出的文档的链接改成 GitHub 网址；图片改成 ![[附件]]；
  · 每篇里第一次出现的核心组件 / 术语（xochitl、xovi、qmd、母版库、book-serve……）链到对应笔记；
  · 另生成：概念笔记（来自 OVERVIEW 术语表）、API 路由总表（从代码抽）、踩坑索引、待验证清单、
    项目 MOC、按实际文件树生成的 Index_MOC.md。

幂等：输出只由仓库内容决定（不写"生成时间"），内容没变的文件不重写；脚本只管自己写过的文件——清单记在
<Vault>/<subdir>/.cang-jie-export.json，下次运行删掉清单里不再生成的文件，绝不碰你自己在这个目录里加的笔记；
目标位置已有一份不归脚本管、内容又不同的文件时拒绝覆盖（--force 才覆盖）。

只用标准库；仓库内需要 git。
"""
from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
import unicodedata
from dataclasses import dataclass, field
from pathlib import Path
from urllib.parse import unquote

REPO = Path(__file__).resolve().parents[2]
DEFAULT_REPO_URL = "https://github.com/bbq191/cang-jie"
MANIFEST = ".cang-jie-export.json"
MOC_TITLE = "cang-jie MOC"
CURRENT_FW = ("3.28.0.172", "3.29.0.153")

# ── 文档映射：仓库路径 → (知识库目录, 笔记名, type, line) ──────────────────────────────────────────
DOCS: list[tuple[str, str, str, str, str]] = [
    ("README.md", "01-总览", "cang-jie 项目说明", "readme", "全局"),
    ("docs/OVERVIEW.md", "01-总览", "系统全貌", "overview", "全局"),
    ("shelf/README.md", "02-架构设计/书架", "书架 说明", "readme", "shelf"),
    ("shelf/docs/reMarkable书架白皮书.md", "02-架构设计/书架", "书架白皮书", "whitepaper", "shelf"),
    ("shelf/docs/传书EPUB线架构.md", "02-架构设计/书架", "传书EPUB线架构", "architecture", "shelf"),
    ("notes/README.md", "02-架构设计/笔记线", "笔记线 说明", "readme", "notes"),
    ("notes/docs/reMarkable笔记白皮书.md", "02-架构设计/笔记线", "笔记白皮书", "whitepaper", "notes"),
    ("gateway/README.md", "02-架构设计/网关", "网关 说明", "readme", "gateway"),
    ("gateway/docs/reMarkable网关白皮书.md", "02-架构设计/网关", "网关白皮书", "whitepaper", "gateway"),
    ("rmsvc-core/README.md", "02-架构设计/服务基座", "服务基座 说明", "readme", "rmsvc-core"),
    ("rmsvc-core/docs/reMarkable设备端Web服务基座白皮书.md", "02-架构设计/服务基座", "服务基座白皮书", "whitepaper", "rmsvc-core"),
    ("enhance/README.md", "02-架构设计/系统增强", "系统增强 说明", "readme", "enhance"),
    ("enhance/docs/reMarkable系统增强线白皮书.md", "02-架构设计/系统增强", "系统增强线白皮书", "whitepaper", "enhance"),
    ("enhance/hl-snap/README.md", "02-架构设计/系统增强", "hl-snap", "readme", "enhance"),
    ("enhance/ui-font/README.md", "02-架构设计/系统增强", "ui-font", "readme", "enhance"),
    ("enhance/wallpaper-serve/README.md", "02-架构设计/系统增强", "wallpaper-serve", "readme", "enhance"),
    ("enhance/lo-alias/README.md", "02-架构设计/系统增强", "lo-alias", "readme", "enhance"),
    ("docs/INSTALL.md", "04-部署运维", "安装部署指南", "runbook", "packaging"),
    ("packaging/README.md", "04-部署运维", "安装器内部结构", "runbook", "packaging"),
    ("enhance/wallpaper-serve/legacy-bind-mount/README.md", "04-部署运维", "壁纸 bind-mount 旧方案", "runbook", "enhance"),
    ("defw/README.md", "05-固件与逆向", "defw 逆向方法", "reverse", "defw"),
    ("enhance/shared/PROVENANCE.md", "05-固件与逆向", "来源说明 enhance-shared", "reverse", "enhance"),
    ("notes/crates/rmv6/PROVENANCE.md", "05-固件与逆向", "来源说明 rmv6", "reverse", "notes"),
    ("rmsvc-core/vendor/README.md", "05-固件与逆向", "第三方 vendor 说明", "reverse", "rmsvc-core"),
    ("gateway/tools/screenshot-walkthrough/README.md", "04-部署运维", "网页截图走查工具", "runbook", "gateway"),
    ("docs/CHANGELOG.md", "06-演进与决策", "更新记录", "changelog", "全局"),
    ("docs/AUDIT-2026-10-10.md", "06-演进与决策", "审计报告 2026-10-10", "audit", "全局"),
    ("shelf/docs/reMarkable书架白皮书-历史附录.md", "06-演进与决策", "书架白皮书 历史附录", "changelog", "shelf"),
]
# 有意不导出：英文版与中文版内容重复；打赏页与知识无关。这些链接改指 GitHub。
EXCLUDE = {"docs/README.en.md", "docs/INSTALL.en.md", "docs/DONATE.md", "docs/DONATE.en.md"}
HISTORY = {"docs/CHANGELOG.md", "docs/AUDIT-2026-10-10.md", "shelf/docs/reMarkable书架白皮书-历史附录.md",
           "enhance/wallpaper-serve/legacy-bind-mount/README.md"}

# ── 术语 / 组件 → 笔记。每篇只链第一次出现；拉丁词要求词边界（xovi.so、xovi-persist、/usr/bin/xochitl 不算）。
# 目标是概念笔记（CONCEPTS 里定义或来自 OVERVIEW 术语表）或已有文档。
TERMS: list[tuple[str, str]] = [
    ("xochitl", "xochitl"), ("xovi", "xovi"), ("qmldiff", "qmd"), ("qmd", "qmd"), ("vellum", "vellum"),
    ("qt-resource-rebuilder", "qt-resource-rebuilder"), ("dm-verity", "dm-verity"), ("OTA", "OTA"),
    ("SSE", "SSE"), ("hook", "hook"), ("sheng-ren", "sheng-ren"),
    ("母版库", "母版库"), ("条目库", "条目库"), ("占位文档", "占位文档"), ("边车", "边车"), ("私有 CA", "私有 CA"),
    ("服务注册表", "注册表"), ("认领", "认领"),
    ("hl-snap", "hl-snap"), ("ui-font", "ui-font"), ("wallpaper-serve", "wallpaper-serve"), ("lo-alias", "lo-alias"),
    ("font-serve", "系统增强 说明"), ("book-serve", "书架 说明"), ("rmsvc-core", "服务基座 说明"),
    ("ink-serve", "笔记线 说明"), ("transcribe-serve", "笔记线 说明"), ("mind-serve", "笔记线 说明"), ("note-serve", "笔记线 说明"),
    ("install-all.sh", "安装器内部结构"), ("verify-on-device.sh", "安装器内部结构"),
]
TERM_TARGET = dict(TERMS)
# OVERVIEW 术语表之外补的三个概念（释义抄自 INSTALL.md「本文会反复出现的几个词」与 README）。
EXTRA_CONCEPTS = {
    "qt-resource-rebuilder": ("第三方组件，xochitl 启动时把 qmd 界面补丁套到 xochitl 的 QML 上；装法 `vellum add qt-resource-rebuilder`。", ["界面补丁加载器"]),
    "dm-verity": ("系统分区的只读校验。开着时安装脚本一律不写 `/usr`。", []),
    "sheng-ren": ("姊妹项目（booklib，`xochitl` 阅读模式），在电脑上把书优化好；书架只把优化好的书原样投进 xochitl。https://github.com/bbq191/sheng-ren", ["booklib"]),
}

TOPIC_RULES: list[tuple[str, str, int]] = [
    ("topic/api", r"`(?:GET|POST|PUT|DELETE) /", 3),
    ("topic/backend", r"\bcrate\b|cargo|loopback|127\.0\.0\.1|服务端", 4),
    ("topic/frontend", r"前端|\.js`|i18n|网页脚本|浏览器", 4),
    ("topic/deployment", r"install-all|deploy[-.]|部署|整机重启", 8),
    ("topic/reverse-engineering", r"objdump|Ghidra|反汇编|反编译|特征码", 3),
    ("topic/security", r"TLS|私有 CA|证书|登录|限速|权限", 5),
    ("topic/bugfix", r"修复|修掉|改正|误报|\bbug\b", 5),
    ("lang/rust", r"\bcargo\b|\bcrate\b|\.rs\b", 3),
    ("lang/c", r"\b[\w-]+\.[ch]\b|trampoline|xovi_glue|make aarch64|交叉编译器", 3),
    ("lang/qml", r"\bqmd\b|QML", 5),
    ("lang/shell", r"\b[\w-]+\.sh\b", 5),
]
PITFALL_HEAD = re.compile(r"踩坑|已知限制|排错|常见问题")
VERIFY_HEAD = re.compile(r"验证现状|待手测|没验证|还没手测|没手测")


def git(*args: str) -> str:
    return subprocess.run(["git", "-C", str(REPO), "-c", "core.quotepath=false", *args], capture_output=True, text=True, check=True).stdout


# ── Markdown 小工具 ───────────────────────────────────────────────────────────────────────────────

FENCE = re.compile(r"^\s*(```|~~~)")
HEADING = re.compile(r"^(#{1,6})\s+(.+?)\s*#*\s*$")


def iter_lines(text: str):
    """逐行给出 (行, 是否在代码块里)。"""
    fence = None
    for line in text.split("\n"):
        m = FENCE.match(line)
        if m:
            if fence is None:
                fence = m.group(1)
                yield line, True
                continue
            if m.group(1) == fence:
                fence = None
                yield line, True
                continue
        yield line, fence is not None


def headings(text: str) -> list[str]:
    return [m.group(2) for line, code in iter_lines(text) if not code and (m := HEADING.match(line))]


def gh_slug(text: str) -> str:
    """GitHub 标题锚点：小写，去掉除字母/数字/连字符/下划线/空格以外的字符，空格变连字符。"""
    out = []
    for ch in text.lower():
        cat = unicodedata.category(ch)
        if ch in "-_" or cat[0] in "LNM":
            out.append(ch)
        elif ch == " ":
            out.append("-")
    return "".join(out)


def slug_map(text: str) -> dict[str, str]:
    seen: dict[str, int] = {}
    result: dict[str, str] = {}
    for h in headings(text):
        base = gh_slug(h)
        n = seen.get(base, 0)
        seen[base] = n + 1
        result[base if n == 0 else f"{base}-{n}"] = h
    return result


BAD_HEADING_CHARS = set("#|^[]`")   # 这些字符在标题链接里不可靠（反引号：Obsidian 是否按原文匹配没核实），退回整篇链接


def wikilink(target: str, heading: str | None = None, alias: str | None = None, in_table: bool = False, embed: bool = False) -> str:
    inner = target
    if heading and not (BAD_HEADING_CHARS & set(heading)):
        inner += "#" + heading
    if alias and alias != inner:
        inner += ("\\|" if in_table else "|") + alias
    return ("!" if embed else "") + "[[" + inner + "]]"


class Masker:
    """把不该动的片段换成占位符，改完再换回来。"""
    PAT = re.compile(r"``[^`].*?``|`[^`\n]+`|\[\[[^\]]+\]\]|<https?://[^>]+>|https?://[^\s)>\]|]+|<[^>\n]+>")

    def __init__(self) -> None:
        self.parts: list[str] = []

    def put(self, s: str) -> str:
        self.parts.append(s)
        return f"\x00{len(self.parts) - 1}\x00"

    def mask(self, s: str, pat: re.Pattern | None = None) -> str:
        return (pat or self.PAT).sub(lambda m: self.put(m.group(0)), s)

    def unmask(self, s: str) -> str:
        for _ in range(4):  # 占位符可能嵌套
            s2 = re.sub(r"\x00(\d+)\x00", lambda m: self.parts[int(m.group(1))], s)
            if s2 == s:
                break
            s = s2
        return s


MD_LINK = re.compile(r"(!?)\[((?:[^\[\]\n]|\[[^\]\n]*\])*)\]\(([^)\s]+)(?:\s+\"[^\"]*\")?\)")
INLINE_CODE = re.compile(r"``[^`].*?``|`[^`\n]+`")


# ── 导出 ───────────────────────────────────────────────────────────────────────────────────────

@dataclass
class Note:
    src: str | None
    rel: str              # 知识库目录内的相对路径
    title: str
    type: str
    line: str
    body: str = ""
    aliases: list[str] = field(default_factory=list)
    status: str = "现行"
    date: str = ""
    updated: str = ""
    links: list[str] = field(default_factory=list)
    origin: str = ""      # 生成的笔记：内容出自哪里（写进 frontmatter 的 source）


class Exporter:
    def __init__(self, subdir: str, repo_url: str) -> None:
        self.subdir = subdir
        self.repo_url = repo_url.rstrip("/")
        self.head_date = git("log", "-1", "--format=%ad", "--date=short").strip()
        self.notes: dict[str, Note] = {}
        self.by_src: dict[str, Note] = {}
        self.assets: dict[str, str] = {}   # 知识库相对路径 → 仓库路径
        self.src_text: dict[str, str] = {}
        self.slugs: dict[str, dict[str, str]] = {}
        self.warnings: list[str] = []

    # 路径与链接 ------------------------------------------------------------------------------------
    def gh(self, path: str) -> str:
        return f"{self.repo_url}/blob/master/{path}"

    def asset_rel(self, repo_path: str) -> str:
        return f"99-附件/{repo_path}"

    def add_note(self, n: Note) -> None:
        assert n.title not in self.notes, n.title
        self.notes[n.title] = n
        if n.src:
            self.by_src[n.src] = n

    def load_docs(self) -> None:
        tracked = set(git("ls-files", "*.md").split("\n")) - {""}
        mapped = {d[0] for d in DOCS}
        for p in sorted(tracked - mapped - EXCLUDE):
            if p.startswith("docs/obsidian/"):
                continue
            self.warnings.append(f"仓库里的 {p} 没有出现在 DOCS / EXCLUDE 里，未导出（新文档请登记到脚本）")
        for src, d, title, typ, line in DOCS:
            if src not in tracked:
                self.warnings.append(f"DOCS 里的 {src} 不在仓库里，跳过")
                continue
            text = (REPO / src).read_text(encoding="utf-8")
            self.src_text[src] = text
            self.slugs[src] = slug_map(text)
            dates = git("log", "--follow", "--format=%ad", "--date=short", "--", src).split()
            stem = Path(src).stem
            aliases = [] if stem in (title, "README") else [stem]
            if stem == "README":
                aliases.append(str(Path(src).parent) + "/README")
            n = Note(src=src, rel=f"{d}/{title}.md", title=title, type=typ, line=line, aliases=aliases,
                     status="历史" if src in HISTORY else "现行",
                     date=dates[-1] if dates else self.head_date, updated=dates[0] if dates else self.head_date)
            self.add_note(n)

    def resolve(self, from_src: str, target: str) -> tuple[str, str | None]:
        """相对链接 → (仓库路径, 锚点)。"""
        path, _, anchor = target.partition("#")
        path = unquote(path)
        if not path:
            return from_src, anchor or None
        full = os.path.normpath(os.path.join(os.path.dirname(from_src), path))
        return full.replace(os.sep, "/"), (unquote(anchor) if anchor else None)

    def convert_links(self, src: str, line: str, in_table: bool, m: Masker) -> str:
        def repl(mt: re.Match) -> str:
            bang, text, target = mt.group(1), mt.group(2), mt.group(3)
            if re.match(r"^[a-z]+:", target):            # http(s)/mailto 原样，整段保护起来不再链术语
                return m.put(mt.group(0))
            repo_path, anchor = self.resolve(src, target)
            alias = m.unmask(text).replace("`", "").strip() or None
            note = self.by_src.get(repo_path)
            if note:
                heading = None
                if anchor:
                    heading = self.slugs[repo_path].get(anchor.lower())
                    if heading is None:
                        self.warnings.append(f"{src}: 锚点 #{anchor} 在 {repo_path} 里找不到标题，改成整篇链接")
                if repo_path == src and heading:
                    return m.put("[[#" + heading + (("\\|" if in_table else "|") + alias if alias else "") + "]]")
                self.by_src[src].links.append(note.title)
                return m.put(wikilink(note.title, heading, alias, in_table))
            if (REPO / repo_path).is_file() and repo_path.lower().endswith((".svg", ".png", ".jpg", ".jpeg", ".gif", ".webp")):
                vault_path = f"{self.subdir}/{self.asset_rel(repo_path)}"
                self.assets[self.asset_rel(repo_path)] = repo_path
                return m.put(wikilink(vault_path, None, None if bang else alias, in_table, embed=bool(bang)))
            if (REPO / repo_path).exists():
                return m.put(f"[{text}]({self.gh(repo_path)}{'#' + anchor if anchor else ''})")
            self.warnings.append(f"{src}: 链接目标 {target} 在仓库里不存在，原样保留")
            return mt.group(0)
        return MD_LINK.sub(repl, line)

    def link_terms(self, line: str, in_table: bool, done: set[str], self_title: str, m: Masker) -> str:
        for term, target in TERMS:
            if target in done or target == self_title or target not in self.notes:
                continue
            if re.match(r"[A-Za-z]", term):
                pat = re.compile(r"(?<![A-Za-z0-9_\-./~$@])" + re.escape(term) + r"(?![A-Za-z0-9_\-/])(?!\.[A-Za-z0-9])")
            else:
                pat = re.compile(re.escape(term))
            mt = pat.search(line)
            if not mt:
                continue
            word = mt.group(0)
            link = wikilink(target, None, None if word == target else word, in_table)
            line = line[: mt.start()] + m.put(link) + line[mt.end():]
            done.add(target)
            self.by_src_title(self_title).links.append(target)
        return line

    def by_src_title(self, title: str) -> Note:
        return self.notes[title]

    def transform(self, note: Note) -> str:
        assert note.src
        out = []
        done: set[str] = set()
        for line, code in iter_lines(self.src_text[note.src]):
            if code:
                out.append(line)
                continue
            m = Masker()
            in_table = line.lstrip().startswith("|")
            s = m.mask(line, INLINE_CODE)
            s = self.convert_links(note.src, s, in_table, m)
            if not HEADING.match(line):
                s = m.mask(s)
                s = self.link_terms(s, in_table, done, note.title, m)
            out.append(m.unmask(s))
        return "\n".join(out)

    # 生成的笔记 ------------------------------------------------------------------------------------
    def concepts(self) -> None:
        text = self.src_text.get("docs/OVERVIEW.md", "")
        section = text.split("## 9. 术语表", 1)[-1].split("\n## ", 1)[0]
        for row in section.split("\n"):
            cells = [c.strip() for c in row.strip().strip("|").split("|")]
            if len(cells) != 2 or cells[0] in ("术语", "---"):
                continue
            names = [re.sub(r"[`（）()]", " ", x).strip() for x in re.split(r"\s*/\s*", cells[0])]
            names = [x.split()[0] if x.startswith("边车") else x for x in names if x]
            title, aliases = names[0], names[1:]
            if title in self.notes or TERM_TARGET.get(title, title) != title:   # rmsvc-core 这类已有文档的不另建
                continue
            if "边车" in cells[0]:
                aliases = ["sidecar"]
            ov = self.by_src["docs/OVERVIEW.md"]
            self.add_note(Note(src=None, rel=f"08-概念/{title}.md", title=title, type="concept", line="全局",
                               aliases=aliases, date=ov.updated, updated=ov.updated, origin="docs/OVERVIEW.md#9. 术语表",
                               body=f"# {title}\n\n{cells[1]}\n\n> 释义来自 [[系统全貌#9. 术语表]]。哪些文档提到它，看反向链接面板。\n"))
        for title, (desc, aliases) in EXTRA_CONCEPTS.items():
            ins = self.by_src["docs/INSTALL.md"]
            self.add_note(Note(src=None, rel=f"08-概念/{title}.md", title=title, type="concept", line="全局",
                               aliases=aliases, date=ins.updated, updated=ins.updated, origin="docs/INSTALL.md",
                               body=f"# {title}\n\n{desc}\n\n> 哪些文档提到它，看反向链接面板。\n"))

    def api_table(self) -> None:
        manage = (REPO / "gateway/src/manage.rs").read_text(encoding="utf-8")
        segs = dict((svc, seg) for seg, svc in re.findall(r'seg: "(\w+)", service: "([\w-]+)"', manage))
        dirs = {"gateway": "gateway", "book-serve": "shelf/services/book-serve", "ink-serve": "notes/services/ink-serve",
                "transcribe-serve": "notes/services/transcribe-serve", "mind-serve": "notes/services/mind-serve",
                "note-serve": "notes/services/note-serve", "font-serve": "enhance/font-serve", "wallpaper-serve": "enhance/wallpaper-serve"}
        doc_of = {"gateway": "网关白皮书", "book-serve": "传书EPUB线架构", "font-serve": "系统增强线白皮书",
                  "wallpaper-serve": "wallpaper-serve"}
        code_head, code_date = git("log", "-1", "--format=%h %ad", "--date=short", "--", *dirs.values(), "gateway/src/manage.rs").split()
        lines = [f"# API 路由总表\n",
                 f"> 从代码自动生成（代码最后改动：提交 `{code_head}`，{code_date}）：各服务非测试代码里 `.get/.post/.put/.delete(\"/…\")` 注册的路由。"
                 "外部只能经网关访问，网关把 `/api/<段>/*` 剥掉段名转给对应服务；参数、请求体与错误码看各线白皮书的「接口与配置参考」。\n"]
        for svc, d in dirs.items():
            src_files = sorted(p for p in (REPO / d / "src").rglob("*.rs") if p.name != "tests.rs" and "tests" not in p.parts)
            bind, routes = "", set()
            for p in src_files:
                t = p.read_text(encoding="utf-8").split("#[cfg(test)]", 1)[0]
                bind = bind or (re.search(r'default_bind: "([^"]+)"', t) or [None, ""])[1]
                for meth, path in re.findall(r'\.(get|post|put|delete|patch)\("(/[^"]*)"', t):
                    routes.add((path, meth.upper(), p.relative_to(REPO).as_posix()))
            unit = next((u for u in (REPO / d.split("/services/")[0]).rglob(f"{svc}.service")), None) or next(iter((REPO / d).rglob(f"{svc}.service")), None)
            if unit:   # 设备上实际监听的是单元里 --bind 的值，代码里的 default_bind 只是缺省
                bind = (re.search(r"ExecStart=\S+ serve --bind (\S+)", unit.read_text(encoding="utf-8")) or [None, bind])[1]
            seg = segs.get(svc)
            doc = doc_of.get(svc) or {"ink-serve": "笔记白皮书", "transcribe-serve": "笔记白皮书", "mind-serve": "笔记白皮书", "note-serve": "笔记白皮书"}.get(svc)
            head = f"## {svc}\n\n监听 `{bind or '?'}`" + (f"；网关前缀 `/api/{seg}`" if seg else "；对外入口本身") + (f"；细节见 [[{doc}]]" if doc else "") + "\n"
            lines += [head, "| 方法 | 路径 | 经网关 | 代码 |", "|---|---|---|---|"]
            for path, meth, f in sorted(routes):
                via = f"`/api/{seg}{path if path != '/' else ''}`" if seg else "—"
                lines.append(f"| {meth} | `{path}` | {via} | [{Path(f).name}]({self.gh(f)}) |")
            lines.append("")
        self.add_note(Note(src=None, rel="03-接口与数据/API 路由总表.md", title="API 路由总表", type="api", line="全局",
                           aliases=["API 总表", "路由表"], date=code_date, updated=code_date, body="\n".join(lines),
                           links=["网关白皮书", "传书EPUB线架构", "笔记白皮书"]))

    def heading_index(self, title: str, rel: str, typ: str, pat: re.Pattern, intro: str) -> None:
        lines = [f"# {title}\n", f"> {intro}\n"]
        for src, _, ntitle, _, line in DOCS:
            if src not in self.src_text or src in HISTORY:
                continue
            hs = [h for h in headings(self.src_text[src]) if pat.search(h) and not (BAD_HEADING_CHARS & set(h))]
            if hs:
                lines.append(f"- **[[{ntitle}]]**（{line}）")
                lines += [f"  - [[{ntitle}#{h}|{h}]]" for h in hs]
        self.add_note(Note(src=None, rel=rel, title=title, type=typ, line="全局", date=self.docs_date,
                           updated=self.docs_date, body="\n".join(lines) + "\n", origin="各篇标题"))

    def moc(self) -> None:
        body = f"""# cang-jie

> reMarkable Paper Pro Move 增强套件：不改 xochitl 本体，靠网页服务、xovi 扩展、qmd 补丁三种旁路补功能。
> 固件 {" / ".join(CURRENT_FW)}。全部文件见 [[Index_MOC]]。

## 1. 从这里开始
- [[cang-jie 项目说明]] · [[系统全貌]]
- [[系统全貌#3. 核心架构|核心架构图]] · [[系统全貌#9. 术语表|术语表]]

## 2. 三条业务线
| 线 | 入口 | 深入 |
|---|---|---|
| 书架 | [[书架 说明]] | [[书架白皮书]] · [[传书EPUB线架构]] |
| 笔记 | [[笔记线 说明]] | [[笔记白皮书]] |
| 系统增强 | [[系统增强 说明]] | [[系统增强线白皮书]] · [[hl-snap]] · [[ui-font]] · [[wallpaper-serve]] · [[lo-alias]] |

## 3. 公共底座
- 网关：[[网关 说明]] · [[网关白皮书]]（登录、反向代理、事件、批量队列、设备健康）
- 服务基座：[[服务基座 说明]] · [[服务基座白皮书]]
- 让 xochitl 自己动手：[[传书EPUB线架构#4｜设备端代理队列：为什么不能直接建文件夹 / 删文档|qmd 代理队列]] · [[传书EPUB线架构#6｜超限书籍的落库：大文件通道|大文件通道]]

## 4. 接口与数据
- [[API 路由总表]]
- 各线接口：[[书架白皮书#5｜接口与配置参考|书架]] · [[笔记白皮书#5 接口与配置参考|笔记]] · [[网关白皮书#5 接口与配置参考|网关]] · [[服务基座白皮书#5 接口参考|服务基座]] · [[系统增强线白皮书#4. 接口与配置参考|系统增强]]
- 数据放在哪：[[系统全貌#6. 东西装在设备的哪里]]

## 5. 部署运维
- [[安装部署指南]]（第一次装 / 更新 / 卸载）· [[安装器内部结构]]
- 固件升级：[[安装部署指南#固件升级（OTA）之后]]
- 排障：[[安装部署指南#常见问题]] · [[网页截图走查工具]]

## 6. 固件与逆向
- [[defw 逆向方法]] · [[系统增强线白皮书#3.2 hl-snap：荧光笔汉字吸附|hl-snap 挂钩点]] · [[系统增强线白皮书#附录 B　xochitl 笔画渲染链（逆向笔记）|笔画渲染链]]
- 来源说明：[[来源说明 enhance-shared]] · [[来源说明 rmv6]] · [[第三方 vendor 说明]]

## 7. 踩坑与限制
- [[踩坑索引]]（汇总各篇「已知限制 / 踩坑 / 排错 / 常见问题」章节）

## 8. 设计决策与演进
- [[网关白皮书#附录 A　设计决策与历史演进|网关]] · [[笔记白皮书#附录 A 设计原则与架构决策|笔记]] · [[服务基座白皮书#附录 A　设计决策与历史演进|服务基座]] · [[系统增强线白皮书#附录 A　设计决策与历史演进|系统增强]]
- [[更新记录]] · [[审计报告 2026-10-10]] · [[书架白皮书 历史附录]]
- 已移除的功能：[[系统全貌#8. 已移除的功能]]

## 9. 验证现状
- [[待验证清单]]（汇总各篇「验证现状 / 待手测」章节）

## 10. 文档索引（需要 Dataview 插件）
```dataview
TABLE type, line, status, updated
FROM #cang-jie
WHERE type != "moc"
SORT line ASC, type ASC
```
"""
        self.add_note(Note(src=None, rel=f"00-MOC/{MOC_TITLE}.md", title=MOC_TITLE, type="moc", line="全局",
                           aliases=["cang-jie", "仓颉"], date=self.docs_date, updated=self.docs_date, body=body, origin="脚本内模板"))

    def index_moc(self, files: list[str]) -> str:
        """按实际要写出的文件树生成 Index_MOC.md（附件只计数）。"""
        tree: dict[str, list[str]] = {}
        for rel in sorted(files):
            d, _, name = rel.rpartition("/")
            if d.startswith("99-附件") or rel == "Index_MOC.md":
                continue
            tree.setdefault(d, []).append(name)
        n_md = sum(1 for f in files if f.endswith(".md") and not f.startswith("99-附件"))
        n_assets = sum(1 for f in files if f.startswith("99-附件/"))
        out = ["# Index_MOC\n", f"> 按导出的文件树自动生成：{n_md} 篇笔记、{n_assets} 个附件（文档最后更新 {self.docs_date}）。"
               f"从 [[{MOC_TITLE}]] 进入更有条理。\n"]
        shown: set[str] = set()
        for d in sorted(tree):
            parts = d.split("/")
            for i in range(len(parts)):          # 父目录没有直接文件也要出标题，层级才对
                anc = "/".join(parts[: i + 1])
                if anc and anc not in shown:
                    shown.add(anc)
                    out.append(f"{'#' * min(2 + i, 6)} {parts[i]}\n")
            for name in tree[d]:
                if name.endswith(".md"):
                    n = self.notes.get(name[:-3])
                    meta = f"（{n.type} · {n.line}{' · 历史' if n.status == '历史' else ''}）" if n else ""
                    out.append(f"- [[{name[:-3]}]]{meta}")
            out.append("")
        out.append(f"## 99-附件\n\n{n_assets} 个示意图，按仓库原路径放在 `99-附件/` 下，由各篇以 `![[…]]` 嵌入。\n")
        return "\n".join(out)

    # frontmatter ---------------------------------------------------------------------------------
    def tags(self, n: Note, text: str) -> list[str]:
        tags = ["cang-jie", f"type/{n.type}"]
        if n.line != "全局":
            tags.append(f"cang-jie/{n.line}")
        if n.type in ("whitepaper", "architecture", "overview"):
            tags.append("topic/architecture")
        if any(PITFALL_HEAD.search(h) for h in headings(text)):
            tags.append("topic/pitfall")
        scale = max(1.0, len(text) / 8000)      # 长文按篇幅放大阈值，免得长白皮书什么标签都沾
        for tag, pat, k in TOPIC_RULES:
            if len(re.findall(pat, text)) >= k * scale:
                tags.append(tag)
        return tags

    @staticmethod
    def q(s: str) -> str:
        return json.dumps(s, ensure_ascii=False)

    def frontmatter(self, n: Note, text: str) -> str:
        fw = [v for v in CURRENT_FW if v in text]
        related = [t for t in dict.fromkeys(
            [x.title for x in self.notes.values() if x.line == n.line and x.title != n.title and x.src and n.line != "全局"]
            + n.links + [MOC_TITLE]) if t != n.title and t in self.notes][:12]
        y = ["---", f"title: {self.q(n.title)}"]
        y += ["aliases:"] + [f"  - {self.q(a)}" for a in n.aliases] if n.aliases else ["aliases: []"]
        y += [f"date: {n.date}", f"updated: {n.updated}", f"type: {n.type}", f"line: {self.q(n.line)}", f"status: {self.q(n.status)}"]
        if fw:
            y += ["firmware:"] + [f"  - {self.q(v)}" for v in fw]
        y += ["tags:"] + [f"  - {t}" for t in self.tags(n, text)]
        y.append(f"source: {self.q(n.src or ('generated: ' + n.origin if n.origin else 'generated'))}")
        y += ["related:"] + [f"  - {self.q('[[' + t + ']]')}" for t in related] if related else ["related: []"]
        y.append("---")
        return "\n".join(y) + "\n"

    # 汇总 ----------------------------------------------------------------------------------------
    def build(self) -> dict[str, bytes]:
        self.load_docs()
        self.docs_date = max(n.updated for n in self.notes.values())
        self.concepts()
        self.api_table()
        self.heading_index("踩坑索引", "07-踩坑记录/踩坑索引.md", "index", PITFALL_HEAD,
                           "各篇「已知限制 / 踩坑 / 排错 / 常见问题」章节的入口，只放链接、不复制正文，以原文为准。")
        self.heading_index("待验证清单", "00-MOC/待验证清单.md", "index", VERIFY_HEAD,
                           "各篇「验证现状 / 待手测」章节的入口。\"已部署\"不等于\"真机验证过\"，以原文的写法为准。")
        self.moc()
        bodies = {t: (self.transform(n) if n.src else n.body) for t, n in self.notes.items()}
        out: dict[str, bytes] = {}
        for t, n in self.notes.items():
            out[n.rel] = (self.frontmatter(n, bodies[t]) + bodies[t].rstrip("\n") + "\n").encode()
        for svg in git("ls-files", "*/docs/diagrams/*.svg", "docs/diagrams/*.svg").split():
            self.assets.setdefault(self.asset_rel(svg), svg)
        for rel, src in self.assets.items():
            out[rel] = (REPO / src).read_bytes()
        idx = Note(src=None, rel="Index_MOC.md", title="Index_MOC", type="moc", line="全局", aliases=["项目知识导航"],
                   date=self.docs_date, updated=self.docs_date, origin="导出的文件树")
        self.notes["Index_MOC"] = idx
        idx_body = self.index_moc(list(out) + ["Index_MOC.md"])
        out["Index_MOC.md"] = (self.frontmatter(idx, idx_body) + idx_body).encode()
        self.validate(out)
        return out

    def validate(self, out: dict[str, bytes]) -> None:
        """所有 [[双链]] 都要能落到一篇笔记（或附件）、带的标题要真在那篇里。"""
        heads = {t: set(headings(out[n.rel].decode())) for t, n in self.notes.items()}
        for rel, data in out.items():
            if not rel.endswith(".md"):
                continue
            body = data.decode()
            for line, code in iter_lines(body):
                if code:
                    continue
                for raw in re.findall(r"\[\[([^\]]+)\]\]", INLINE_CODE.sub("", line)):
                    target = raw.replace("\\|", "|").split("|", 1)[0]
                    if target.startswith(f"{self.subdir}/"):
                        if target[len(self.subdir) + 1:] not in out:
                            self.warnings.append(f"{rel}: 附件 {target} 不存在")
                        continue
                    note, _, h = target.partition("#")
                    note = note or Path(rel).stem
                    if note not in heads:
                        self.warnings.append(f"{rel}: [[{target}]] 指向不存在的笔记")
                    elif h and h not in heads[note]:
                        self.warnings.append(f"{rel}: [[{target}]] 的标题在「{note}」里找不到")


# ── 写入（幂等、只管自己的文件）──────────────────────────────────────────────────────────────────

def write_all(target: Path, out: dict[str, bytes], dry: bool, force: bool) -> int:
    man_path = target / MANIFEST
    old = set(json.loads(man_path.read_text())["files"]) if man_path.is_file() else set()
    conflicts = [r for r in out if r not in old and (target / r).is_file() and (target / r).read_bytes() != out[r]]
    if conflicts and not force:
        print("!! 这些文件已在目标位置、不归本脚本管、内容又不同，拒绝覆盖（确认后加 --force）：", file=sys.stderr)
        for r in conflicts:
            print("   ", target / r, file=sys.stderr)
        return 3
    written = same = 0
    for rel, data in sorted(out.items()):
        p = target / rel
        if p.is_file() and p.read_bytes() == data:
            same += 1
            continue
        written += 1
        if dry:
            print("  写", rel)
            continue
        p.parent.mkdir(parents=True, exist_ok=True)
        tmp = p.with_name(f".{p.name}.tmp{os.getpid()}")
        tmp.write_bytes(data)
        os.replace(tmp, p)
    gone = sorted(old - set(out))
    for rel in gone:
        if dry:
            print("  删", rel)
            continue
        (target / rel).unlink(missing_ok=True)
        d = (target / rel).parent
        while d != target and d.is_dir() and not any(d.iterdir()):
            d.rmdir()
            d = d.parent
    if not dry:
        man = json.dumps({"tool": "cang-jie migrate_to_obsidian.py", "files": sorted(out)}, ensure_ascii=False, indent=1) + "\n"
        if not man_path.is_file() or man_path.read_text() != man:
            man_path.write_text(man)
    print(f"{'(dry-run) ' if dry else ''}写入 {written}，未变 {same}，删除 {len(gone)} → {target}")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description="把 cang-jie 文档导出到 Obsidian 知识库（幂等，可重复运行）")
    ap.add_argument("vault", help="Obsidian Vault 的绝对路径")
    ap.add_argument("--subdir", default="cang-jie", help="项目放在 Vault 里的子目录（缺省 cang-jie）")
    ap.add_argument("--repo-url", default=DEFAULT_REPO_URL, help="源码 / 未导出文档的链接指向的仓库网址")
    ap.add_argument("--dry-run", action="store_true", help="只打印要写 / 删的文件")
    ap.add_argument("--force", action="store_true", help="目标位置已有不归脚本管的同名文件时也覆盖")
    a = ap.parse_args()

    vault = Path(a.vault)
    if not vault.is_absolute() or not vault.is_dir():
        print(f"!! Vault 必须是已存在目录的绝对路径：{a.vault}", file=sys.stderr)
        return 2
    if not re.fullmatch(r"[^/\\]+", a.subdir) or a.subdir in (".", ".."):
        print(f"!! --subdir 只能是一层目录名：{a.subdir}", file=sys.stderr)
        return 2
    vault = vault.resolve()
    target = vault / a.subdir
    if REPO == target or REPO in target.parents or target in REPO.parents:
        print(f"!! 目标 {target} 与仓库 {REPO} 重叠，拒绝写入", file=sys.stderr)
        return 2
    if not (vault / ".obsidian").is_dir():
        print(f"-- 提示：{vault} 下没有 .obsidian/，可能还没在 Obsidian 里打开过（不影响导出）")

    ex = Exporter(a.subdir, a.repo_url)
    out = ex.build()
    for w in ex.warnings:
        print("⚠", w)
    return write_all(target, out, a.dry_run, a.force)


if __name__ == "__main__":
    sys.exit(main())
