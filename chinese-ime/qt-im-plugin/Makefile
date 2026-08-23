# chinese-ime/qt-im-plugin —— 标准 Qt6 platforminputcontext 插件 + qputenv hook
# 白皮书方向③(输入法架构重定向)的离线可复现构建。
#
# 机制备忘(反汇编/头文件核实,勿凭记忆):
#  - qputenv(const char* x0, QByteArrayView{x1=size, x2=const char* data})
#    6.10.3 QByteArrayView 布局 {qsizetype m_size; const char* m_data}(QT_VERSION<7 分支)。
#  - hook 走独立 LD_PRELOAD .so(参与符号插桩)——不是折进 dlopen 的 xovi 扩展(拦不到)、
#    也不 patch qputenv 序言(序言 off 0x08 是 adrp,20 字节 trampoline 搬走即 SEGV)。
#  - 插件用本机 Qt6.11 头编译 + 链设备 Qt6.10.3 .so + metadata 降版 06 0b→06 0a 过设备版本门。
#
# 依赖:交叉编译器 aarch64-linux-gnu-g++、Qt6 头(本机)、qt6 moc、sdk/ 下设备 .so。

CXX        := aarch64-linux-gnu-g++
CC         := aarch64-linux-gnu-gcc
MOC        := /usr/lib/qt6/moc
QTINC      := /usr/include/qt6
QTVER      := 6.11.1

# metadata 头 4 字节 = {格式版 01, Qt主 06, Qt次 0b/0a, arch_req}。
# arch_req 第7位(0x80)=debug 构建标志(QPluginMetaData::archRequirements),
# 靠 -DQT_NO_DEBUG 编成 release(0x00),别 hex patch;剩 Qt次 0b→0a 才 patch。
# 设备验证过的前作头 = 01 06 0a 00。
HDR_HOST   := 01060b00
HDR_DEV    := 01060a00

INCLUDES := -I$(QTINC) \
            -I$(QTINC)/QtCore -I$(QTINC)/QtCore/$(QTVER) -I$(QTINC)/QtCore/$(QTVER)/QtCore \
            -I$(QTINC)/QtGui  -I$(QTINC)/QtGui/$(QTVER)  -I$(QTINC)/QtGui/$(QTVER)/QtGui

# -DQT_NO_VERSION_TAGGING: 别把本机 6.11 的符号版本依赖(@Qt_6.11)编进去,否则设备 6.10 缺失符号版本加载失败。
CXXFLAGS := -O2 -fPIC -std=c++17 -Wall -Wextra -DQT_NO_VERSION_TAGGING -DQT_NO_DEBUG $(INCLUDES)
LDFLAGS  := -shared -Lsdk -l:libQt6Core.so.6.10.3 -l:libQt6Gui.so.6.10.3 \
            -Wl,--no-as-needed -Wl,--unresolved-symbols=ignore-in-shared-libs

PLUGIN  := libcangjieinputcontextplugin.so
HOOK    := cangjie-qputenv-hook.so

.PHONY: all plugin hook clean verify-repro

all: plugin hook

# ---- 插件 ----
src/cangjieinputcontext.moc: src/cangjieinputcontext.cpp src/cangjie.json
	$(MOC) $(INCLUDES) $< -o $@

$(PLUGIN): src/cangjieinputcontext.cpp src/cangjieinputcontext.moc
	cd src && $(CXX) $(CXXFLAGS) cangjieinputcontext.cpp -o ../$(PLUGIN) \
		$(subst sdk,../sdk,$(LDFLAGS))
	@echo ">> metadata 头降版 $(HDR_HOST) → $(HDR_DEV)(Qt次 11→10,过设备 6.10 版本门)"
	python3 tools/patch_metadata.py $(PLUGIN) $(HDR_HOST) $(HDR_DEV)

plugin: $(PLUGIN)

# ---- qputenv hook(纯 C,无 Qt) ----
$(HOOK): src/qputenv-hook.c
	$(CC) -O2 -fPIC -shared -Wall -Wextra $< -o $@ -ldl

hook: $(HOOK)

clean:
	rm -f $(PLUGIN) $(HOOK) src/cangjieinputcontext.moc
