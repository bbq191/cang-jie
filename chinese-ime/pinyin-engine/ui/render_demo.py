"""
把 CandidateBar.qml 接上真实的 PinyinEngine，离屏渲染成截图。

用系统 python3（装了 PySide6）跑，不是 rmfw/.venv（那边为了跑
pytest 单元测试没装 PySide6，两边用途不同，没必要合并）：

    QT_QPA_PLATFORM=offscreen python3 ui/render_demo.py <拼音输入> <输出图片路径>

例：
    QT_QPA_PLATFORM=offscreen python3 ui/render_demo.py nihao /tmp/candidate_bar.png
"""
from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))  # 让 `import src.engine` 能找到

from PySide6.QtCore import QObject, Property, Signal, QUrl
from PySide6.QtGui import QGuiApplication
from PySide6.QtQuick import QQuickView

from src.engine import PinyinEngine

FONT_PATH = Path(__file__).resolve().parent.parent.parent / "rmfw" / "fonts" / "SourceHanSans-SC-subset.otf"


class Bridge(QObject):
    def __init__(self, engine: PinyinEngine, raw: str):
        super().__init__()
        self._engine = engine
        self._result = engine.query(raw, limit=8)

    inputRawChanged = Signal()
    candidatesChanged = Signal()

    def _get_input_raw(self) -> str:
        return self._result.input_raw

    def _get_segmentation_display(self) -> str:
        return str(self._result.segmentation)

    def _get_candidates(self) -> list[str]:
        return self._result.candidates

    def _get_font_url(self) -> str:
        return QUrl.fromLocalFile(str(FONT_PATH)).toString()

    inputRaw = Property(str, _get_input_raw, notify=inputRawChanged)
    segmentationDisplay = Property(str, _get_segmentation_display, notify=inputRawChanged)
    candidates = Property(list, _get_candidates, notify=candidatesChanged)
    fontUrl = Property(str, _get_font_url, notify=inputRawChanged)


def main() -> None:
    raw = sys.argv[1] if len(sys.argv) > 1 else "nihao"
    out_path = sys.argv[2] if len(sys.argv) > 2 else "/tmp/candidate_bar.png"

    app = QGuiApplication(sys.argv)
    engine = PinyinEngine()
    bridge = Bridge(engine, raw)

    view = QQuickView()
    view.rootContext().setContextProperty("bridge", bridge)
    qml_path = Path(__file__).resolve().parent / "CandidateBar.qml"
    view.setSource(QUrl.fromLocalFile(str(qml_path)))

    if view.status() == QQuickView.Error:
        for err in view.errors():
            print("QML error:", err.toString(), file=sys.stderr)
        sys.exit(1)

    view.setResizeMode(QQuickView.SizeViewToRootObject)
    view.show()
    app.processEvents()

    image = view.grabWindow()
    image.save(out_path)
    print(f"输入: {raw!r}  切分: {bridge.segmentationDisplay}  候选: {bridge.candidates}")
    print(f"截图已保存: {out_path} ({image.width()}x{image.height()})")


if __name__ == "__main__":
    main()
