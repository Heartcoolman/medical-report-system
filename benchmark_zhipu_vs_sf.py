#!/usr/bin/env python3
"""
对比测试：智谱原生 GLM-4.6V vs 硅基流动 GLM-4.6V
相同模型，不同平台，对比速度和准确率
"""

import base64, json, re, sys, time
import requests
from pathlib import Path

MODELS = [
    {
        "name": "智谱原生 GLM-4.6V",
        "model": "glm-4.6v",
        "api_url": "https://open.bigmodel.cn/api/paas/v4/chat/completions",
        "api_key": "2f23dbecc49748e4a69c7b40d96b809a.tt92gXHO8PQUzfl1",
    },
    {
        "name": "硅基流动 GLM-4.6V",
        "model": "zai-org/GLM-4.6V",
        "api_url": "https://api.siliconflow.cn/v1/chat/completions",
        "api_key": "sk-tlqmgztjissfptfvvxphznsvjclqgnnaupkniheofhvagtkz",
    },
]

ROUNDS = 2

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
- 如果不是检验报告而是费用清单等，report_type 填写实际类型，items 留空数组
- 只返回 JSON，不要有任何额外说明"""


def call_model(cfg: dict, data_url: str) -> dict:
    payload = {
        "model": cfg["model"],
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
        "Authorization": f"Bearer {cfg['api_key']}",
        "Content-Type": "application/json",
    }

    start = time.time()
    try:
        resp = requests.post(cfg["api_url"], headers=headers, json=payload, timeout=300)
        elapsed = time.time() - start
        resp.raise_for_status()
        data = resp.json()

        if "error" in data:
            return {"time": elapsed, "error": data["error"].get("message", str(data["error"]))}

        content = data["choices"][0]["message"]["content"]
        content = re.sub(r"<think>.*?</think>", "", content, flags=re.DOTALL)
        usage = data.get("usage", {})

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
                "raw_content": content,
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
        print("未找到测试图片")
        sys.exit(1)

    file_data = []
    for fp in test_files:
        with open(fp, "rb") as f:
            b64 = base64.standard_b64encode(f.read()).decode()
        mime = "image/png" if str(fp).lower().endswith(".png") else "image/jpeg"
        file_data.append((fp.name, f"data:{mime};base64,{b64}"))

    print("=" * 70)
    print("  平台对比：智谱原生 vs 硅基流动（同模型 GLM-4.6V）")
    print("=" * 70)
    print(f"测试文件数: {len(file_data)}, 每模型轮次: {ROUNDS}")
    print()

    all_results = {m["name"]: [] for m in MODELS}

    for file_name, data_url in file_data:
        short_name = file_name.split("_", 1)[-1][:50]
        print(f"--- 文件: {short_name} ---")

        for model_cfg in MODELS:
            for rd in range(1, ROUNDS + 1):
                tag = f"  [{model_cfg['name']}] 第{rd}轮"
                print(f"{tag} ...", end=" ", flush=True)

                r = call_model(model_cfg, data_url)
                all_results[model_cfg["name"]].append(r)

                if r.get("error"):
                    print(f"❌ {r['error'][:80]} ({r['time']:.1f}s)")
                else:
                    tok = r.get("usage", {})
                    tok_str = f" | tokens: {tok.get('total_tokens', '?')}" if tok else ""
                    print(
                        f"✅ {r['time']:.1f}s | "
                        f"类型: {r['report_type']} | "
                        f"项数: {r['items_count']}"
                        f"{tok_str}"
                    )
                # 间隔避免限流
                if rd < ROUNDS or model_cfg != MODELS[-1]:
                    time.sleep(3)
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

        print(f"\n【{name}】")
        print(f"  模型ID: {model_cfg['model']}")
        print(f"  API: {model_cfg['api_url']}")
        print(f"  成功率: {len(ok)}/{len(rs)}")
        if times:
            print(f"  平均耗时: {sum(times)/len(times):.2f}s")
            print(f"  最快/最慢: {min(times):.2f}s / {max(times):.2f}s")
        if items:
            print(f"  平均识别项数: {sum(items)/len(items):.1f}")
            total_toks = [r.get("usage", {}).get("total_tokens", 0) for r in ok]
            if any(total_toks):
                print(f"  平均 tokens: {sum(total_toks)/len(total_toks):.0f}")

    # 速度差异
    print(f"\n{'=' * 70}")
    ok_a = [r for r in all_results[MODELS[0]["name"]] if not r.get("error")]
    ok_b = [r for r in all_results[MODELS[1]["name"]] if not r.get("error")]
    if ok_a and ok_b:
        avg_a = sum(r["time"] for r in ok_a) / len(ok_a)
        avg_b = sum(r["time"] for r in ok_b) / len(ok_b)
        if avg_a < avg_b:
            print(f"  ⚡ {MODELS[0]['name']} 快 {avg_b/avg_a:.1f}x ({avg_a:.1f}s vs {avg_b:.1f}s)")
        else:
            print(f"  ⚡ {MODELS[1]['name']} 快 {avg_a/avg_b:.1f}x ({avg_b:.1f}s vs {avg_a:.1f}s)")

    # === 详细对比 ===
    print(f"\n{'=' * 70}")
    print("  识别结果详细对比（取最后一轮）")
    print("=" * 70)

    for model_cfg in MODELS:
        name = model_cfg["name"]
        rs = all_results[name]
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
        if last_ok["items_count"] > 0:
            print(f"  {'检查项':<20} {'结果':<12} {'单位':<10} {'参考范围':<18} {'状态'}")
            print(f"  {'-'*20} {'-'*12} {'-'*10} {'-'*18} {'-'*6}")
            for item in last_ok.get("items", []):
                n = item.get("name", "")[:20]
                v = str(item.get("value", ""))[:12]
                u = item.get("unit", "")[:10]
                ref = item.get("reference_range", "")[:18]
                s = item.get("status", "")
                flag = " ↑" if s == "high" else (" ↓" if s == "low" else "")
                print(f"  {n:<20} {v:<12} {u:<10} {ref:<18} {s}{flag}")

    print()


if __name__ == "__main__":
    main()
