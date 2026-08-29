# B1 · 手写笔记 → 待校对 Markdown → PKM 库

reMarkable 上的中文手写笔记，host 侧转成 Markdown 进笔记生态/PKM。**零设备风险**（只读设备文件、逻辑全在 host），定位是**辅助转写 + 人工校对**——不是无人值守 OCR。

路线图 P1 的落地。识别质量 de-risk 结论（`docs/reMarkable功能路线图白皮书.md` §06）：**工整中文手写≈100%，快写连笔~60%（局部整段崩）**；传统印刷 OCR（tesseract）判死；喂原生缩略图优于自制渲染。所以本工具默认收割 xochitl 缩略图 + 多模态 vision + 人工校对。

## 管线

```
取数据 → 取图 → 识别 → 待校对 bundle → （你）校对 → PKM
scp .rm/    缩略图    vision      vault/<书名>.md      改错字      笔记生态
缩略图/元数据 优先，   四家可选     每页图内嵌+草稿       + status     库
（切页/退出   缺则                 +⚠待校对标记         置已校对
 后才落盘）   hw_render 兜底
```

## 用法

```bash
# 默认：从设备拉「笔记本」这本，gemini 识别，写到 ./handwriting-vault
uv run python export.py --name 笔记本

# 指定 UUID + 换后端（省钱走 deepseek）
uv run python export.py --doc <uuid> --provider deepseek --out ~/notes/inbox

# 不碰设备，跑已拉好的本地镜像
uv run python export.py --src /path/to/xochitl-mirror --doc <uuid>

# 只出带图 bundle、不调 API（自己对着图手填，或没配 key 时）
uv run python export.py --name 笔记本 --no-vision
```

缩略图缺、要 `hw_render` 兜底反解渲染时：`uv run --with rmscene --with pillow python export.py ...`。

## vision 后端（`--provider`，默认 gemini）

一个 OpenAI-兼容路径覆盖 gemini/deepseek/openai，另加原生 anthropic。key 走各自环境变量，零锁定；模型名会漂移，用 `--model` 覆盖 `vision.py:PROVIDERS` 里的默认值。

| provider | 默认模型 | key 环境变量 | 备注 |
| --- | --- | --- | --- |
| `gemini`（默认） | gemini-3.6-flash | `GEMINI_API_KEY` / `GOOGLE_API_KEY` | 手写最强，有免费额度；活体实测工整100%/快写91% |
| `deepseek` | deepseek-v4-flash-vision-exp | `DEEPSEEK_API_KEY` | 极便宜；实验；每图≤384token（密页可能掉质） |
| `openai` | gpt-4o | `OPENAI_API_KEY` | 同级备选 |
| `anthropic` | claude-sonnet-4-5 | `ANTHROPIC_API_KEY` | de-risk 亲测 100%/65% |

单图冒烟：`GEMINI_API_KEY=... python vision.py <图.png> gemini`。

## 文件

| 文件 | 作用 |
| --- | --- |
| `export.py` | 主管线（取数据→取图→识别→bundle→写 vault） |
| `vision.py` | 可插拔多模态适配器（四家后端，urllib 不绑 SDK） |
| `requirements.txt` | 依赖（正常路径仅标准库；兜底渲染才需 rmscene+pillow） |

反解/渲染复用 `../proto/rm_strokes.py`（笔划）与 `../proto/hw_render.py`（栅格兜底）。

## 边界（de-risk 已划走，不进 MVP）

- 传统印刷 OCR（tesseract）——中文手写判死。
- 无人值守直接入库——快写 60% 不可信，必须人工校对。
- 自训 online-HWR（吃 `.rm` 笔顺时序拔高快写）——无现成中文离线引擎，留作后续可选增强。
