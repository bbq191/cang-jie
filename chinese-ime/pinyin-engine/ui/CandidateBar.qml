// 候选栏组件——对应白皮书 9.6 节的 QML 骨架，这版是真的接了 Python 拼音引擎、
// 能跑起来的原型（不再是伪代码），配色刻意做成黑白高对比，模拟 e-ink 的
// 单色显示效果（9.6 节强调墨水屏没有真彩色/灰阶动画，UI 逻辑先按黑白设计）。
import QtQuick

Rectangle {
    id: root
    width: 760
    height: 220
    color: "white"

    Column {
        anchors.fill: parent
        anchors.margins: 16
        spacing: 12

        Text {
            text: "拼音候选栏原型 · e-ink 单色配色 (reMarkable Paper Pro Move 中文化方案 9.6 节)"
            font.pixelSize: 14
            color: "#666666"
        }

        Rectangle {
            width: parent.width
            height: 1
            color: "#cccccc"
        }

        // 输入行：显示当前音节切分结果
        Row {
            spacing: 6
            Text {
                text: "拼音输入："
                font.pixelSize: 22
                color: "black"
            }
            Text {
                text: bridge.inputRaw
                font.pixelSize: 22
                font.family: fontLoader.name
                color: "black"
            }
            Text {
                text: "  [ " + bridge.segmentationDisplay + " ]"
                font.pixelSize: 16
                color: "#888888"
            }
        }

        // 候选栏本体：横向排列，局部刷新区域（9.6 节"只重绘候选栏矩形区域"对应的就是这一块）
        Row {
            id: candidateBar
            spacing: 8
            Repeater {
                model: bridge.candidates
                Rectangle {
                    width: candidateText.width + 24
                    height: 44
                    border.color: "black"
                    border.width: index === 0 ? 2 : 1  // 默认上屏项加粗边框，呼应 9.2 节"候选排序"
                    color: index === 0 ? "#f0f0f0" : "white"
                    radius: 4

                    Row {
                        anchors.centerIn: parent
                        spacing: 4
                        Text {
                            text: (index + 1) + "."
                            font.pixelSize: 13
                            color: "#999999"
                            anchors.verticalCenter: parent.verticalCenter
                        }
                        Text {
                            id: candidateText
                            text: modelData
                            font.pixelSize: 26
                            font.family: fontLoader.name
                            color: "black"
                            anchors.verticalCenter: parent.verticalCenter
                        }
                    }
                }
            }
        }

        Text {
            visible: bridge.candidates.length === 0
            text: "（词典查不到候选——原样透传拼音，不瞎编字，见 pinyin-engine README）"
            font.pixelSize: 14
            color: "#aa4422"
        }
    }

    FontLoader {
        id: fontLoader
        source: bridge.fontUrl
    }
}
