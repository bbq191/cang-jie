# reading-qol 资源附件

- `ko-icon.png`：Sidebar「KOReader」入口图标，"Ko" 字母 alpha 蒙版（Noto Sans Bold 192px，
  字形不透明/底全透明——ArkControls.Icon 染色管线要求）。重新生成：PIL 画 "Ko" 于透明底即可。
- `ko-icon.qrc`：资源清单，注册为 `qrc:/cangjie/icons/koreader`。
- 编译部署：`/usr/lib/qt6/rcc --binary -o cangjie-icons.rcc ko-icon.qrc`，产物放设备
  `xovi/exthome/qt-resource-rebuilder/`（qt-resource-rebuilder 的 .rcc 通道，启动时注册进 qrc）。
