#!/usr/bin/env python3
"""快速检查：两个平台的 GLM-4.6V 是否开了思考模式"""

import base64, json, re, time
import requests
from pathlib import Path

MODELS = [
    {
        "name": "智谱原生",
        "model": "glm-4.6v",
        "api_url": "https://open.bigmodel.cn/api/paas/v4/chat/completions",
        "api_key": "2f23dbecc49748e4a69c7b40d96b809a.tt92gXHO8PQUzfl1",
    },
    {
        "name": "硅基流动",
        "model": "zai-org/GLM-4.6V",
        "api_url": "https://api.siliconflow.cn/v1/chat/completions",
        "api_key": "sk-tlqmgztjissfptfvvxphznsvjclqgnnaupkniheofhvagtkz",
    },
]

uploads = Path(__file__).parent / "backend" / "uploads"
fp = next(uploads.glob("*.jpg"))
with open(fp, "rb") as f:
    b64 = base64.standard_b64encode(f.read()).decode()
data_url = f"data:image/jpeg;base64,{b64}"

for cfg in MODELS:
    print(f"\n{'='*50}")
    print(f"【{cfg['name']}】 model={cfg['model']}")
    print(f"{'='*50}")

    payload = {
        "model": cfg["model"],
        "messages": [
            {"role": "system", "content": "用JSON返回报告类型。"},
            {"role": "user", "content": [
                {"type": "image_url", "image_url": {"url": data_url}},
                {"type": "text", "text": "这是什么类型的报告？"},
            ]},
        ],
        "temperature": 0.1,
        "max_tokens": 512,
    }

    headers = {"Authorization": f"Bearer {cfg['api_key']}", "Content-Type": "application/json"}

    start = time.time()
    resp = requests.post(cfg["api_url"], headers=headers, json=payload, timeout=120)
    elapsed = time.time() - start
    data = resp.json()

    if "error" in data:
        print(f"  错误: {data['error']}")
        continue

    content = data["choices"][0]["message"]["content"]
    usage = data.get("usage", {})

    print(f"  耗时: {elapsed:.1f}s")
    print(f"  usage: {json.dumps(usage, ensure_ascii=False)}")

    has_think = "<think>" in content
    print(f"  包含 <think> 标签: {'是 ✅' if has_think else '否 ❌'}")

    # 显示原始内容前300字符
    print(f"  原始响应 (前300字):")
    print(f"  {content[:300]}")
    print()
