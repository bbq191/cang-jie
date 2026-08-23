# cang-jie

给 [reMarkable Paper Pro Move](https://remarkable.com/) 的官方阅读/笔记程序 **xochitl** 注入**中文输入法 + 界面汉化**的个人项目。全程**不改 xochitl 本体**——`cangjie-langhook.so` 是合规 [xovi](https://github.com/asivery/xovi) 扩展，放 `xovi/extensions.d/` 被自动加载，运行时动态改行为，磁盘上的原始二进制原封不动。中文化的核心交付都归拢在 **`chinese-ime/`** 子文件夹。

> 本项目是个人设备自用、不对外分发。涉及许可证的数据（rime-ice/iorest 词典等）不编译进 `.so`，只做独立文件运行时 mmap 只读。

## 两条线

| 线 | 内容 | 里程碑 | 状态 |
|---|---|---|---|
| **UI 汉化** | 简体/繁体菜单、翻译文件（Qt `.qm`）、字体、原生 Settings 语言集成 | M0–M2 | 基本收工 |
| **拼音输入法** | 虚拟键盘 hook、拼音/双拼引擎、候选词典、逐字候选、繁体、中英混输、按语言字体 | M3–M7 | 真机全链路验证通过，收尾中 |

## 白皮书（完整设计 + 真机调试记录）

- **[reMarkable 拼音输入法白皮书](chinese-ime/docs/reMarkable拼音输入法白皮书.md)** —— 输入法这条线（M3–M7）。含"从新机到当前进度"的完整复现主线。
- **[reMarkable 中文化白皮书](chinese-ime/docs/reMarkable中文化白皮书.md)** —— UI 汉化这条线（M0–M2）。共享的环境搭建 / xovi 基础设施出处。
- **[功能路线图白皮书](docs/reMarkable功能路线图白皮书.md)** —— 下一步做什么的优先级共识。P0（微信读书深度集成进 xochitl）已立项，设计落在 `weread-client/`（代码 + README + `ATTRIBUTION.md`），不另开方案文档。

## 目录结构

| 路径 | 作用 |
|---|---|
| **`chinese-ime/`** | **中文化核心交付子文件夹**（归拢），下面几项 |
| `chinese-ime/langhook/` | 设备端 hook：编译成 `cangjie-langhook.so`（合规 **xovi 扩展**，放 `extensions.d/` 自动加载），承载语言切换器接入 + 按键拦截/拼音缓冲/候选栏全部逻辑；`deploy/` 含**一键安装包**。见 [langhook/README.md](chinese-ime/langhook/README.md) |
| `chinese-ime/pinyin-engine/` | 拼音引擎离线核心：`src/`（Python 参照）+ `c/`（C 移植 + blob 工具 + 差分测试）+ `data/`（词库 + 许可证留痕）+ `ui/` + `tests/` |
| `chinese-ime/fonts/` · `translations/` · `docs/` | 中文字体（+OFL）· zh `.qm` 翻译 · 两本白皮书 |
| `ghidra-project/` | 反编译工程（顶层）：离线定位 hook 点 / 偏移 / 参数签名 |
| `rmfw/` | 固件镜像 `out/`+`extracted/`（顶层，通用侦查资源；中文字体已移进 `chinese-ime/fonts/`） |
| `xovi-extensions/` | `reading-qol`（点击翻页/快速黑白/键盘 Mono）+ `font-menu`（阅读增强，非中文化本身） |
| `weread-client/` | **设备端工具总仓**（名沿用历史，实含三线）：P0 微信读书深度集成 + PKM ★全局待办引擎（`device-rs/` Rust 生产实现）+ EPUB 优化器。见 [weread-client/README.md](weread-client/README.md) |
| `pkm-semantic/` | PKM ★待办检测算法的 **Python 原型 + 阈值标定**（生产 Rust 移植落在 `weread-client/device-rs/`）。见 [pkm-semantic/README.md](pkm-semantic/README.md) |
| `docs/` | **跨项目白皮书**：功能路线图 + 网络解决方案（中文化/拼音两本就近在 `chinese-ime/docs/`） |
| `assets/` | 项目杂项媒体（logo / 截图 / 演示视频），不参与构建 |
| 工程纪律 | 工程纪律（真机验证再宣称完成、一步一确认、改设备前备份等） |

## 快速开始

### 装到一部新机（SSH 后一键安装）

见 [chinese-ime/langhook/README.md](chinese-ime/langhook/README.md) 的「一键安装」——前置装好开发者模式/SSH + xovi/qt-resource-rebuilder（vellum 装的或官方），然后：

```bash
scp chinese-ime/langhook/deploy/dist/cangjie-ime-installer.tar.gz root@10.11.99.1:/home/root/
ssh root@10.11.99.1 'cd /home/root && tar -xzf cangjie-ime-installer.tar.gz && /home/root/cangjie-ime/install.sh'
```

### 本地开发 / 测试（不需要设备）

```bash
# 拼音引擎：Python 单测 + C 差分测试
cd chinese-ime/pinyin-engine && python3 -m pytest -q
cd chinese-ime/pinyin-engine/c && make test && make diff-check

# 设备端 hook：宿主机单测 + aarch64 交叉编译（xovi 扩展需 xovigen，XOVI_DIR 指向 xovi clone）
cd chinese-ime/langhook && make test && make aarch64 XOVI_DIR=<asivery/xovi clone 路径>
```

## 当前进度

- **M0–M2（UI 汉化）**：交叉编译工具链、xovi、字体 subset、`.qm` 翻译（简/繁/港）、原生 Settings 语言集成——基本收工。
- **M3（虚拟键盘 hook）/ M4（拼音候选可用，含逐字造句/分段提交/退格撤销）/ M6（双拼 + 繁体）**：真机全链路验证通过。
- **M5（全局可用 + 原生入口）**：核心已达成（hook 打在系统级 `VirtualKeyboard` 上、原生入口走键盘语言弹层），收尾打磨中。
- **中英混输（Phase C）**：增量式词典补全（"你好hello"），真机验证。
- **字体**：候选栏 + 整个 UI 按语言用 HarmonyOS Sans SC/TC。
- **M7（长期维护）**：多轮实战——① 固件 OTA 后各 hook 地址位移，做了**韧性重构**（每个目标字节特征码运行期自定位、掩码通配相对跳转、metaobject 靠 static_metacall 指针反查），新固件全命中、不需再推导偏移；② 设备装 vellum 后，`cangjie-langhook.so` 重构成**合规 xovi 扩展**放 `extensions.d/`（/home 持久），弃独立 drop-in；xovi 启动配置放 `/usr/lib`（rootfs，普通重启不丢），界面翻译放 `/usr/share`；固件 OTA 冲掉 rootfs 后重跑 `install.sh` 一键恢复。全部真机验证通过。详见 [langhook/README.md](chinese-ime/langhook/README.md)。
