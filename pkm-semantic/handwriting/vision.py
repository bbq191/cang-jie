#!/usr/bin/env python3
"""B1 手写识别 · 可插拔多模态 vision 适配器（host 侧）。

四家后端，`--provider` 选，key 走各自环境变量，零锁定：
  - gemini    （默认）Google，手写最强；OpenAI-兼容端点；key=GEMINI_API_KEY / GOOGLE_API_KEY
  - deepseek  极便宜；deepseek-v4-flash-vision-exp（实验）；每图≤384token；key=DEEPSEEK_API_KEY
  - openai    GPT 视觉；key=OPENAI_API_KEY
  - anthropic Claude 视觉（de-risk 亲测 100%/65%）；原生 messages API；key=ANTHROPIC_API_KEY

Gemini/DeepSeek/OpenAI 都吃 OpenAI-兼容的 chat/completions + image_url，共用一条代码路；
Anthropic 用原生 messages 格式，单独一条。仅依赖 requests（不绑各家 SDK）。

⚠ 模型名会随时间漂移：各家默认值见 PROVIDERS，命令行 `--model` 可覆盖。识别质量与成本
的 de-risk 结论见 docs 路线图白皮书 §06（工整≈100%/快写~60%，tesseract 判死）。
"""
from __future__ import annotations

import base64
import json
import mimetypes
import os
import sys
import urllib.request
import urllib.error

# 手写转写提示词（de-risk 验证有效：逐行、不留空、只出转写）。
DEFAULT_PROMPT = (
    "这是一页 reMarkable 中文手写笔记的渲染图。请逐行辨认并原样转写上面的手写内容。"
    "这是可能潦草的快写连笔，请尽力逐字识别：认不准的字写你觉得最可能的那个，不要留空、"
    "不要用问号或方括号占位。带圈数字（①②③④…）照写。只输出转写正文本身，一行一项，"
    "不要任何解释、不要描述图片、不要评论清晰度。"
)

# 卡片模式提示词：整页卡片（黑打印槽行 + 红手写批注）→ 结构化 {槽→转写} JSON。
# de-risk 验证：vision 能准确把红手写空间关联到对应黑行（4/4）。
CARD_PROMPT = (
    "这是一张 reMarkable「总结卡片」的整页渲染。**黑色打印**是卡片固定结构（若干带图标的槽，"
    "如 🟡金句、🔵洞见、🩷疑问、🟠主题、🟢可复用、⚪人物）；**红色（或其它彩色）手写**是用户"
    "在某些槽旁边加的批注。\n"
    "任务：找出所有有手写批注的槽，把每段手写转写出来，按它空间上贴着哪个槽归类。\n"
    "只输出一个 JSON 数组，每元素 {\"slot\":\"槽的关键词\",\"note\":\"手写转写\"}；"
    "slot 用槽名关键词之一（金句/洞见/疑问/主题/可复用/人物），note 是手写逐字转写"
    "（潦草认不准写最可能的字，不留空、不加问号占位）。没有手写的槽不要列。"
    "不要输出 JSON 以外的任何字符（不要 markdown 代码围栏、不要解释）。"
)

# style=openai 走 chat/completions+image_url；style=anthropic 走原生 messages。
PROVIDERS: dict[str, dict] = {
    "gemini": {
        "style": "openai",
        "base_url": "https://generativelanguage.googleapis.com/v1beta/openai",
        "default_model": "gemini-3.6-flash",
        "key_envs": ["GEMINI_API_KEY", "GOOGLE_API_KEY"],
    },
    "deepseek": {
        "style": "openai",
        "base_url": "https://api.deepseek.com",
        "default_model": "deepseek-v4-flash-vision-exp",
        "key_envs": ["DEEPSEEK_API_KEY"],
    },
    "openai": {
        "style": "openai",
        "base_url": "https://api.openai.com/v1",
        "default_model": "gpt-4o",
        "key_envs": ["OPENAI_API_KEY"],
    },
    "anthropic": {
        "style": "anthropic",
        "base_url": "https://api.anthropic.com/v1",
        "default_model": "claude-sonnet-4-5",
        "key_envs": ["ANTHROPIC_API_KEY"],
    },
}


class VisionError(RuntimeError):
    pass


def _resolve_key(cfg: dict, provider: str) -> str:
    for env in cfg["key_envs"]:
        v = os.environ.get(env)
        if v:
            return v
    raise VisionError(
        f"后端 {provider} 缺 API key：请设置环境变量 {' 或 '.join(cfg['key_envs'])}。"
    )


def _data_uri(image_path: str) -> tuple[str, str, bytes]:
    mime = mimetypes.guess_type(image_path)[0] or "image/png"
    raw = open(image_path, "rb").read()
    b64 = base64.b64encode(raw).decode("ascii")
    return mime, b64, raw


def _post(url: str, headers: dict, payload: dict, timeout: int = 120) -> dict:
    data = json.dumps(payload).encode("utf-8")
    req = urllib.request.Request(url, data=data, headers=headers, method="POST")
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            return json.loads(resp.read().decode("utf-8"))
    except urllib.error.HTTPError as e:
        body = e.read().decode("utf-8", "replace")[:500]
        raise VisionError(f"HTTP {e.code} 从 {url}：{body}") from e
    except urllib.error.URLError as e:
        raise VisionError(f"连不上 {url}：{e.reason}（检查网络/端点）") from e


def _call_openai_style(cfg, model, key, image_path, prompt) -> str:
    mime, b64, _ = _data_uri(image_path)
    payload = {
        "model": model,
        "messages": [{
            "role": "user",
            "content": [
                {"type": "text", "text": prompt},
                {"type": "image_url", "image_url": {"url": f"data:{mime};base64,{b64}"}},
            ],
        }],
        "temperature": 0,
    }
    headers = {"Authorization": f"Bearer {key}", "Content-Type": "application/json"}
    out = _post(f"{cfg['base_url']}/chat/completions", headers, payload)
    try:
        return out["choices"][0]["message"]["content"].strip()
    except (KeyError, IndexError, TypeError) as e:
        raise VisionError(f"应答无法解析：{json.dumps(out)[:400]}") from e


def _call_anthropic(cfg, model, key, image_path, prompt) -> str:
    mime, b64, _ = _data_uri(image_path)
    payload = {
        "model": model,
        "max_tokens": 2048,
        "messages": [{
            "role": "user",
            "content": [
                {"type": "image", "source": {"type": "base64", "media_type": mime, "data": b64}},
                {"type": "text", "text": prompt},
            ],
        }],
    }
    headers = {
        "x-api-key": key,
        "anthropic-version": "2023-06-01",
        "Content-Type": "application/json",
    }
    out = _post(f"{cfg['base_url']}/messages", headers, payload)
    try:
        parts = [b.get("text", "") for b in out["content"] if b.get("type") == "text"]
        return "".join(parts).strip()
    except (KeyError, TypeError) as e:
        raise VisionError(f"应答无法解析：{json.dumps(out)[:400]}") from e


def transcribe(image_path: str, provider: str = "gemini", model: str | None = None,
               prompt: str | None = None) -> str:
    """一张图 → 手写转写文本。缺 key/网络/应答异常抛 VisionError。"""
    if provider not in PROVIDERS:
        raise VisionError(f"未知后端 {provider}；可选：{', '.join(PROVIDERS)}")
    cfg = PROVIDERS[provider]
    key = _resolve_key(cfg, provider)
    model = model or cfg["default_model"]
    prompt = prompt or DEFAULT_PROMPT
    if cfg["style"] == "anthropic":
        return _call_anthropic(cfg, model, key, image_path, prompt)
    return _call_openai_style(cfg, model, key, image_path, prompt)


def transcribe_card(image_path: str, provider: str = "gemini", model: str | None = None) -> list[dict]:
    """整页卡片 → [{slot, note}]（红手写批注按空间归到对应槽）。解析失败返回 []。"""
    raw = transcribe(image_path, provider, model, prompt=CARD_PROMPT)
    s = raw.strip()
    if s.startswith("```"):  # 容忍模型套了代码围栏
        s = s.strip("`")
        s = s[s.find("\n") + 1:] if "\n" in s else s
        s = s.rstrip("`").strip()
        if s.startswith("json"):
            s = s[4:].strip()
    lb, rb = s.find("["), s.rfind("]")
    if lb < 0 or rb < 0:
        raise VisionError(f"卡片模式应答非 JSON 数组：{raw[:300]}")
    try:
        arr = json.loads(s[lb:rb + 1])
    except json.JSONDecodeError as e:
        raise VisionError(f"卡片模式 JSON 解析失败：{e}；原文 {raw[:300]}") from e
    return [{"slot": str(d.get("slot", "")).strip(), "note": str(d.get("note", "")).strip()}
            for d in arr if isinstance(d, dict) and d.get("note")]


if __name__ == "__main__":  # 单图冒烟：python vision.py <img> [provider] [model]
    img = sys.argv[1]
    prov = sys.argv[2] if len(sys.argv) > 2 else "gemini"
    mdl = sys.argv[3] if len(sys.argv) > 3 else None
    print(transcribe(img, prov, mdl))
