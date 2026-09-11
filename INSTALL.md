# 安装部署指南

**[English](INSTALL.en.md)**

## 适用范围

**reMarkable Paper Pro Move（imx93-chiappa），固件 3.28.0.172**——这是目前唯一经过真机验证的
固件版本，装之前脚本会自动核对（见下「固件安全门」）。其它固件版本/其它 reMarkable 型号未验证，
贸然强装有 QML 注入定位错位的风险（轻则某个功能不生效，重则影响设备正常使用）。

## 装之前：这几样需要你自己手动做一遍

以下都是 reMarkable 官方 / 第三方生态自己的基础设施，不属于这个仓库，`install-all.sh` **不会**
替你装——缺了会在对应步骤给出清楚的报错，告诉你该跑哪条命令：

1. **`vellum add xovi`** —— [xovi](https://github.com/asivery/xovi) 本体（第三方扩展加载框架）。
   本仓库大部分功能都是以 xovi 扩展的形式运行的，这是最基础的前提。
2. **`vellum add qt-resource-rebuilder`** —— QML 资源热替换。`shelf`/`gateway` 里少数可选特性
   （字体菜单、回收站/建夹网页代理）依赖它；没装这两个特性会自动跳过，不影响其它功能安装。
3. **`vellum add appload`** —— 第三方 App 加载器。
4. 通过 appload **侧载 KOReader**。

vellum 本身怎么安装、appload/KOReader 侧载的具体步骤，请参考 vellum 与 reMarkable 社区自己的
文档——这些是设备通用基础设施，不是本项目维护的内容，这里不重复。

## 一条命令装完

电脑通过 USB 线连上设备（reMarkable 默认在这条链路上把自己配成 `10.11.99.1`）：

```sh
git clone git@github.com:bbq191/cang-jie.git
cd cang-jie/packaging
sh install-all.sh 10.11.99.1
```

这条命令会依次做这些事（任何一步都可以单独运行，见下「只装一部分」）：

| 步骤 | 做什么 | 前置 |
|---|---|---|
| 固件安全门 | 核对设备固件跟已验证版本是否一致 | — |
| 国内 NTP | chrony 服务器换成国内可达的（阿里云/腾讯云等） | — |
| 默认时区 | 设为 Asia/Shanghai | — |
| 电池诊断 | 常驻采样服务，网页「管理→电池刺客」可查看 | — |
| xovi 开机持久化 | 装一个开机自动重跑 `xovi/start` 的单元，往后重启不用再手动补 | 已 `vellum add xovi` |
| 荧光笔精确吸附 | CJK 划词精确吸附（划哪吸哪，不再"划一小段吸整行"） | 同上 |
| 手写笔锋渲染 | 按运笔角度/速度优化手写笔画粗细 | 同上 |
| 书架 + 网关 + 笔记线 + 字体/壁纸 | 九个 Web 服务：书籍管理、笔记摄取转写、字体/壁纸上传即用 | — |
| 统一应用 | 上面几个 xovi 扩展落盘完，最后统一跑一次 `xovi/start` 让它们生效 | — |

跑完打印一份汇总：装了什么、（如果有）哪一步失败了、还有哪些步骤需要你手动做（vellum 引导、
KOReader 侧载这些）。任何一步失败都不会自动重试、不会静默跳过——照着报错原样处理即可，脚本
全部幂等，重新跑一遍整条命令是安全的。

## 装完之后

浏览器打开 `https://10.11.99.1/`（或同一网段内 `https://shelf.local/`，注意安卓系统不支持
`.local` 域名解析）：

- 默认密码 `shelf`，**首次登录后必须改密码**（系统会强制跳转改密页）。
- 会提示证书不受信任（自签证书）——登录页有「下载 CA 证书」，装进浏览器/系统信任库一次即可
  不再提示；临时用途也可以直接点「高级 → 继续访问」跳过。

## 固件安全门

`install-all.sh` 装之前会 ssh 到设备，拉 `/usr/bin/xochitl` 的 sha256，跟仓库里
`packaging/firmware-allowlist.txt` 记录的已验证哈希比对——命中才继续装，不命中默认拒绝。这么
严格是因为字体菜单、回收站代理这类功能靠**字节级 QML 注入定位**，哪怕版本号相同、固件的热修
补丁也可能挪动内部布局，版本号不足以保证安全。

确认这台设备的固件就是你自己验证过没问题的、只是哈希没登记进白名单：加 `--force`（会自动把
当前哈希追加进白名单文件）。

```sh
sh install-all.sh 10.11.99.1 --force
```

## 只装一部分 / 跳过某几步

```sh
sh install-all.sh 10.11.99.1 --skip chrony-cn,timezone-cn,xovi-persist
```

可跳过的步骤名：`chrony-cn`、`timezone-cn`、`battop`、`xovi-persist`、`hl-snap`、
`handwriting-stroke`、`shelf`、`xovi-apply`。每一步对应的脚本
（`packaging/deploy-<step>.sh <host>`）也都可以脱离 `install-all.sh` 单独运行。

## 这套安装器不做什么

- **不装 vellum/xovi/qt-resource-rebuilder/appload 本体，不侧载 KOReader**——见上面「装之前」，
  这些是手动前置条件。
- **不装中文输入法**——这条功能线已经从本仓库归档（见顶层 [README](README.md)「历史与范围」），
  当前不随本安装器分发。
- **不装 wifi-watch 常驻看护**（WiFi 载波异常自动重连）。
- **没有对称的一键卸载**——`shelf/uninstall.sh` 能卸掉书架那部分，其余组件靠手动
  `systemctl disable --now <单元>` 清理。

完整的架构决策、每一步踩过的坑、真机验证现状，见 [`packaging/README.md`](packaging/README.md)——
这是面向工程细节的参考文档，本文件只是面向"第一次装"的快速上手指南。

## 固件升级（OTA）之后

OTA 只会冲掉设备的系统分区（`/usr` + `/etc`），`/home` 下的数据（书库母版、配置、证书等）原样
保留。升级后重新跑一遍：

```sh
cd packaging && sh install-all.sh 10.11.99.1
```

即可恢复全部功能——所有脚本设计为幂等，重复运行不会出问题。更详细的 OTA 影响范围表格见
[`shelf/README.md`](shelf/README.md)「固件升级（OTA）与恢复」一节。

## 遇到问题

先看 `install-all.sh` 跑完的汇总输出，找到具体是哪一步失败；对应的 `packaging/deploy-*.sh`
脚本内部都有详细注释说明这一步在做什么、常见失败原因。仍然没头绪，`packaging/README.md`「验证
现状」一节记录了目前已知的真机踩坑和修法。
