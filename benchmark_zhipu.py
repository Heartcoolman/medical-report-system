#!/usr/bin/env python3
"""
智谱开放平台模型对比测试：GLM-4.6V-Flash vs GLM-4.6V
对比速度和识别准确率
"""

import base64, json, re, sys, time
import requests
from pathlib import Path

API_KEY = "2f23dbecc49748e4a69c7b40d96b809a.tt92gXHO8PQUzfl1"
API_URL = "https://open.bigmodel.cn/api/paas/v4/chat/completions"

MODELS = [
    {"name": "GLM-4.6V-Flash", "model": "glm-4.6v-flash"},
    {"name": "GLM-4.6V",       "model": "glm-4.6v"},
]

ROUNDS = 2  # 每个模型跑几轮

SYSTEM_PROMPT = """你是一个专业的医疗检验报告识别助手。请从报告中提取以下信息，以严格的 JSON 格式返回，不要包含任何其他文字：
{
  "report_type": "报告类型",
  "hospital": "医院名称",
  "sample_date": "检查/采样日期，格式 YYYY-MM-DD",
  "report_date": "报告出具日期，格式 YYYY-MM-DD",
  "items": [
    {
      "name": "检查项名称（使用报告上的原始名称）",
      "value": "结果值",
      "unit": "单位",
      "reference_range": "参考范围",
      "status": "normal 或 high 或 low"
    }
  ]
}
注意：
- items 中包含所有有结果的检查项
- status 根据参考范围判断，↑/H 为 high，↓/L 为 low
- 只返回 JSON，不要有任何额外说明"""


def call_model(model_id: str, data_url: str) -> dict:
    payload = {
        "model": model_id,
        "messages": [
            {"role": "system", "content": SYSTEM_PROMPT},
            {
                "role": "user",
                "content": [
                    {"type": "image_url", "image_url": {"url": data_url}},
                    {"type": "text", "text": "请识别这份医疗检验报告中的所有信息。"},
                ],
            },
        ],
        "temperature": 0.1,
        "max_tokens": 4096,
    }

    headers = {
        "Authorization": f"Bearer {API_KEY}",
        "Content-Type": "application/json",
    }

    start = time.time()
    try:
        resp = requests.post(API_URL, headers=headers, json=payload, timeout=180)
        elapsed = time.time() - start
        resp.raise_for_status()
        data = resp.json()

        if "error" in data:
            return {"time": elapsed, "error": data["error"].get("message", str(data["error"]))}

        content = data["choices"][0]["message"]["content"]
        # Strip think blocks if any
        content = re.sub(r"<think>.*?</think>", "", content, flags=re.DOTALL)

        usage = data.get("usage", {})

        # Parse JSON from response
        parsed = None
        for pattern in [r"```json\s*(.*?)```", r"```(.*?)```"]:
            m = re.search(pattern, content, re.DOTALL)
            if m:
                try:
                    parsed = json.loads(m.group(1).strip())
                except json.JSONDecodeError:
                    continue
                break
        if not parsed:
            si = content.find("{")
            ei = content.rfind("}")
            if si != -1 and ei != -1:
                try:
                    parsed = json.loads(content[si : ei + 1])
                except json.JSONDecodeError:
                    pass

        if parsed:
            return {
                "time": elapsed,
                "report_type": parsed.get("report_type", ""),
                "hospital": parsed.get("hospital", ""),
                "sample_date": parsed.get("sample_date", ""),
                "report_date": parsed.get("report_date", ""),
                "items_count": len(parsed.get("items", [])),
                "items": parsed.get("items", []),
                "usage": usage,
                "error": None,
            }
        else:
            return {"time": elapsed, "error": f"无法解析JSON: {content[:200]}"}

    except Exception as e:
        elapsed = time.time() - start
        return {"time": elapsed, "error": str(e)}


def main():
    uploads = Path(__file__).parent / "backend" / "uploads"
    test_files = sorted(uploads.glob("*.jpg")) + sorted(uploads.glob("*.jpeg")) + sorted(uploads.glob("*.png"))
    if not test_files:
        print("未找到测试图片文件")
        sys.exit(1)

    # Prepare base64 for each file
    file_data = []
    for fp in test_files:
        with open(fp, "rb") as f:
            b64 = base64.standard_b64encode(f.read()).decode()
        lower = str(fp).lower()
        if lower.endswith(".png"):
            mime = "image/png"
        else:
            mime = "image/jpeg"
        data_url = f"data:{mime};base64,{b64}"
        file_data.append((fp.name, data_url))

    print("=" * 70)
    print("  智谱开放平台模型对比测试：GLM-4.6V-Flash vs GLM-4.6V")
    print("=" * 70)
    print(f"测试文件数: {len(file_data)}")
    print(f"每模型轮次: {ROUNDS}")
    print()

    all_results = {m["name"]: [] for m in MODELS}

    for file_name, data_url in file_data:
        short_name = file_name.split("_", 1)[-1][:40]
        print(f"--- 文件: {short_name} ---")

        for model_cfg in MODELS:
            for rd in range(1, ROUNDS + 1):
                tag = f"  [{model_cfg['name']}] 第{rd}轮"
                print(f"{tag} ...", end=" ", flush=True)
                r = call_model(model_cfg["model"], data_url)
                all_results[model_cfg["name"]].append(r)

                if r.get("error"):
                    print(f"❌ {r['error'][:80]} ({r['time']:.1f}s)")
                else:
                    tokens = r.get("usage", {})
                    tok_info = ""
                    if tokens:
                        tok_info = f" | tokens: {tokens.get('total_tokens', '?')}"
                    print(
                        f"✅ {r['time']:.1f}s | "
                        f"类型: {r['report_type']} | "
                        f"检查项: {r['items_count']}个"
                        f"{tok_info}"
                    )
        print()

    # === 汇总 ===
    print("=" * 70)
    print("  汇 总 对 比")
    print("=" * 70)

    for model_cfg in MODELS:
        name = model_cfg["name"]
        rs = all_results[name]
        ok = [r for r in rs if not r.get("error")]
        times = [r["time"] for r in ok]
        items = [r["items_count"] for r in ok]

        print(f"\n【{name}】 (model: {model_cfg['model']})")
        print(f"  成功率: {len(ok)}/{len(rs)}")
        if times:
            print(f"  平均耗时: {sum(times)/len(times):.2f}s")
            print(f"  最快/最慢: {min(times):.2f}s / {max(times):.2f}s")
        if items:
            print(f"  平均识别项数: {sum(items)/len(items):.1f}")
            # Tokens
            total_toks = [r.get("usage", {}).get("total_tokens", 0) for r in ok]
            if any(total_toks):
                print(f"  平均 tokens: {sum(total_toks)/len(total_toks):.0f}")

    # === 逐轮详细对比 ===
    print(f"\n{'=' * 70}")
    print("  识别结果详细对比（取最后一轮）")
    print("=" * 70)

    for model_cfg in MODELS:
        name = model_cfg["name"]
        rs = all_results[name]
        # Take last successful result
        last_ok = None
        for r in reversed(rs):
            if not r.get("error"):
                last_ok = r
                break
        if not last_ok:
            print(f"\n【{name}】: 全部失败")
            continue

        print(f"\n【{name}】 — {last_ok['items_count']} 项, {last_ok['time']:.2f}s")
        print(f"  报告类型: {last_ok['report_type']}")
        print(f"  医院: {last_ok['hospital']}")
        print(f"  采样日期: {last_ok.get('sample_date', '')}")
        print(f"  报告日期: {last_ok.get('report_date', '')}")
        print(f"  {'检查项':<20} {'结果':<12} {'单位':<10} {'参考范围':<18} {'状态'}")
        print(f"  {'-'*20} {'-'*12} {'-'*10} {'-'*18} {'-'*6}")
        for item in last_ok.get("items", []):
            n = item.get("name", "")[:20]
            v = str(item.get("value", ""))[:12]
            u = item.get("unit", "")[:10]
            ref = item.get("reference_range", "")[:18]
            s = item.get("status", "")
            flag = ""
            if s == "high":
                flag = " ↑"
            elif s == "low":
                flag = " ↓"
            print(f"  {n:<20} {v:<12} {u:<10} {ref:<18} {s}{flag}")

    print()


if __name__ == "__main__":
    main()
