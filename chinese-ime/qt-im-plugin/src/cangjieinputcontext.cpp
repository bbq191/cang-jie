// Qt6 platforminputcontext 插件——装饰器版(承接真机命门验证)。
//
// 背景:上一版是"裸桩"——直接替换原生 "xochitl" IC,真机验证证明整条注入链能执行
// (hook 改 QT_IM_MODULES→plugin 加载→构造→commitString 你好),但 xochitl 随即崩溃;
// 对照实验(原生手动启动稳定)锁定根因=裸替换原生 IC、缺装饰器(xochitl 依赖自己的 IC 子类)。
//
// 本版:CangjieInputContext 内部 QPlatformInputContextFactory::create("xochitl") 拿到原生
// 静态 IC,**把 15 个虚函数全部转发给它**(保 xochitl 不崩 + 原生键盘/面板照常),只在真正
// ImEnabled 的焦点对象上额外注入"你好"验证可见命门。引擎接线在命门确认后再做。
//
// 机制/ABI/部署见 DESIGN.md;factory create 重入不死锁(qLoadPlugin 调插件 create 前已释放锁)。
#include <qpa/qplatforminputcontext.h>
#include <qpa/qplatforminputcontextplugin_p.h>
#include <qpa/qplatforminputcontextfactory_p.h>
#include <QInputMethodEvent>
#include <QGuiApplication>
#include <QObject>
#include <QString>
#include <cstdio>

static void logmsg(const char *m) {
    fprintf(stderr, "[cangjie-im] %s\n", m); fflush(stderr);
    FILE *f = fopen("/tmp/cangjie-im.log", "a");   // stderr 被 xochitl 吞了,落文件确认调用
    if (f) { fprintf(f, "[cangjie-im] %s\n", m); fclose(f); }
}

class CangjieInputContext : public QPlatformInputContext {
    Q_OBJECT
    QPlatformInputContext *m_native = nullptr;   // 原生 xochitl IC(被装饰对象)

public:
    CangjieInputContext() {
        // 装饰器核心:拿原生 "xochitl" 静态 IC。QT_IM_MODULES 已被 hook 改成 cangjie,
        // 这里显式按 key 再取原生的,包起来转发——保 xochitl 依赖不落空、键盘还在。
        m_native = QPlatformInputContextFactory::create(QStringLiteral("xochitl"));
        if (m_native) {
            m_native->setParent(this);           // 生命周期挂到本对象
            logmsg("装饰器:已获取并包裹原生 xochitl IC");
        } else {
            logmsg("!! 装饰器:拿不到原生 xochitl IC(native=null)——转发将退回基类默认");
        }
    }

    // ---- 15 个虚函数:有原生就转发,没有就回退基类默认(逐字对齐基类签名) ----
    bool isValid() const override { return m_native ? m_native->isValid() : true; }
    bool hasCapability(Capability c) const override
        { return m_native ? m_native->hasCapability(c) : QPlatformInputContext::hasCapability(c); }
    void reset() override { if (m_native) m_native->reset(); }
    void commit() override { if (m_native) m_native->commit(); }
    void update(Qt::InputMethodQueries q) override { if (m_native) m_native->update(q); }
    void invokeAction(QInputMethod::Action a, int p) override { if (m_native) m_native->invokeAction(a, p); }
    bool filterEvent(const QEvent *e) override { return m_native ? m_native->filterEvent(e) : false; }
    QRectF keyboardRect() const override { return m_native ? m_native->keyboardRect() : QRectF(); }
    bool isAnimating() const override { return m_native ? m_native->isAnimating() : false; }
    void showInputPanel() override { if (m_native) m_native->showInputPanel(); }
    void hideInputPanel() override { if (m_native) m_native->hideInputPanel(); }
    bool isInputPanelVisible() const override { return m_native ? m_native->isInputPanelVisible() : false; }
    QLocale locale() const override { return m_native ? m_native->locale() : QPlatformInputContext::locale(); }
    Qt::LayoutDirection inputDirection() const override
        { return m_native ? m_native->inputDirection() : QPlatformInputContext::inputDirection(); }

    void setFocusObject(QObject *object) override {
        if (m_native) m_native->setFocusObject(object);   // 先转发,让原生正常处理焦点/键盘
        // 命门可见性验证:只对真正接受输入法(ImEnabled)的对象注入,避开启动期非文本对象
        // (裸桩崩溃教训:无差别注入 + 无原生 IC 才炸;此处有原生兜底 + ImEnabled 守卫)。
        if (object) {
            QInputMethodQueryEvent q(Qt::ImEnabled);
            QGuiApplication::sendEvent(object, &q);
            if (q.value(Qt::ImEnabled).toBool()) {
                QInputMethodEvent ev;
                ev.setCommitString(QString::fromUtf8("\xe4\xbd\xa0\xe5\xa5\xbd")); // 你好
                QGuiApplication::sendEvent(object, &ev);
                logmsg("命门:向 ImEnabled 焦点对象提交 commitString=你好");
            }
        }
    }
};

class CangjieInputContextPlugin : public QPlatformInputContextPlugin {
    Q_OBJECT
    Q_PLUGIN_METADATA(IID QPlatformInputContextFactoryInterface_iid FILE "cangjie.json")
public:
    QPlatformInputContext *create(const QString &key, const QStringList &) override {
        fprintf(stderr, "[cangjie-im] plugin create() key=%s\n", key.toUtf8().constData());
        fflush(stderr);
        // 注:真机验证已证明"安装我们自己的 IC(即便装饰转发)会崩",xochitl 要求安装的 IC
        // 必须是它自己的具体类 KeyboardInputContext。此 create 保留作过程记录,方向③已判不通(见 DESIGN.md §7)。
        if (key.compare(QLatin1String("cangjie"), Qt::CaseInsensitive) == 0)
            return new CangjieInputContext;
        return nullptr;
    }
};

#include "cangjieinputcontext.moc"
