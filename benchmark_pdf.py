#!/usr/bin/env python3
"""
用实际检验报告 PDF 对比：智谱原生 vs 硅基流动 GLM-4.6V
分别测试 开启思考 / 关闭思考
"""

import base64, json, re, time
import requests

API_URL_ZHIPU = "https://open.bigmodel.cn/api/paas/v4/chat/completions"
API_KEY_ZHIPU = "2f23dbecc49748e4a69c7b40d96b809a.tt92gXHO8PQUzfl1"
API_URL_SF    = "https://api.siliconflow.cn/v1/chat/completions"
API_KEY_SF    = "sk-tlqmgztjissfptfvvxphznsvjclqgnnaupkniheofhvagtkz"

PDF_PATH = "/Users/liji/Downloads/store_a5aa9b5811176d6925e37a746df6745c36806dd9c4e0de90.pdf"

CONFIGS = [
    {"name": "智谱原生 (开思考)", "model": "glm-4.6v", "api_url": API_URL_ZHIPU, "api_key": API_KEY_ZHIPU, "thinking": True},
    {"name": "智谱原生 (关思考)", "model": "glm-4.6v", "api_url": API_URL_ZHIPU, "api_key": API_KEY_ZHIPU, "thinking": False},
    {"name": "硅基流动 (开思考)", "model": "zai-org/GLM-4.6V", "api_url": API_URL_SF, "api_key": API_KEY_SF, "thinking": True},
    {"name": "硅基流动 (关思考)", "model": "zai-org/GLM-4.6V", "api_url": API_URL_SF, "api_key": API_KEY_SF, "thinking": False},
]

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
- items 中包含所有有结果的检查项，包括定量和定性结果
- status 根据参考范围判断，↑/H 为 high，↓/L 为 low
- 严格逐行提取，不要跨行混淆
- 只返回 JSON，不要有任何额外说明"""


def call_model(cfg: dict, data_url: str) -> dict:
    payload = {
        "model": cfg["model"],
        "messages": [
            {"role": "system", "content": SYSTEM_PROMPT},
            {"role": "user", "content": [
                {"type": "image_url", "image_url": {"url": data_url}},
                {"type": "text", "text": "请识别这份医疗检验报告中的所有信息。"},
            ]},
        ],
        "temperature": 0.1,
        "max_tokens": 4096,
    }

    # 关闭思考
    if not cfg.get("thinking", True):
        # 智谱格式
        if "bigmodel.cn" in cfg["api_url"]:
            payload["extra"] = {"enable_thinking": False}
        # 硅基流动格式
        else:
            payload["thinking"] = {"type": "disabled"}

    headers = {"Authorization": f"Bearer {cfg['api_key']}", "Content-Type": "application/json"}

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
            # Try removing special tokens
            clean = re.sub(r"<\|[^|]*\|>", "", content).strip()
            si = clean.find("{")
            ei = clean.rfind("}")
            if si != -1 and ei != -1:
                try:
                    parsed = json.loads(clean[si : ei + 1])
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
    with open(PDF_PATH, "rb") as f:
        b64 = base64.standard_b64encode(f.read()).decode()
    data_url = f"data:application/pdf;base64,{b64}"
    print(f"PDF 大小: {len(b64) * 3 // 4 // 1024} KB")

    print("=" * 70)
    print("  检验报告 PDF 对比：智谱原生 vs 硅基流动 (GLM-4.6V)")
    print("  分别测试开启/关闭思考模式")
    print("=" * 70)

    results = {}

    for cfg in CONFIGS:
        name = cfg["name"]
        print(f"\n  [{name}] 调用中 ...", end=" ", flush=True)

        r = call_model(cfg, data_url)
        results[name] = r

        if r.get("error"):
            print(f"❌ {r['error'][:100]} ({r['time']:.1f}s)")
        else:
            usage = r.get("usage", {})
            reasoning = usage.get("completion_tokens_details", {}).get("reasoning_tokens", 0)
            comp = usage.get("completion_tokens", 0)
            print(
                f"✅ {r['time']:.1f}s | "
                f"项数: {r['items_count']} | "
                f"tokens: {usage.get('total_tokens', '?')} "
                f"(推理: {reasoning}, 输出: {comp - reasoning})"
            )

        time.sleep(3)

    # === 汇总表 ===
    print(f"\n{'=' * 70}")
    print("  汇 总 对 比")
    print("=" * 70)
    print(f"\n  {'配置':<22} {'耗时':>6} {'项数':>4} {'推理tokens':>10} {'输出tokens':>10} {'总tokens':>8}")
    print(f"  {'-'*22} {'-'*6} {'-'*4} {'-'*10} {'-'*10} {'-'*8}")

    for cfg in CONFIGS:
        name = cfg["name"]
        r = results[name]
        if r.get("error"):
            print(f"  {name:<22} {'失败':>6}")
            continue
        usage = r.get("usage", {})
        reasoning = usage.get("completion_tokens_details", {}).get("reasoning_tokens", 0)
        comp = usage.get("completion_tokens", 0)
        total = usage.get("total_tokens", 0)
        print(f"  {name:<22} {r['time']:>5.1f}s {r['items_count']:>4} {reasoning:>10} {comp - reasoning:>10} {total:>8}")

    # === 详细识别结果 ===
    print(f"\n{'=' * 70}")
    print("  识别结果详细对比")
    print("=" * 70)

    for cfg in CONFIGS:
        name = cfg["name"]
        r = results[name]
        if r.get("error"):
            print(f"\n【{name}】: 失败 - {r['error'][:80]}")
            continue

        print(f"\n【{name}】 — {r['items_count']} 项, {r['time']:.2f}s")
        print(f"  报告类型: {r['report_type']}")
        print(f"  医院: {r['hospital']}")
        print(f"  采样日期: {r.get('sample_date', '')} / 报告日期: {r.get('report_date', '')}")
        if r["items_count"] > 0:
            print(f"  {'检查项':<24} {'结果':<12} {'单位':<8} {'参考范围':<18} {'状态'}")
            print(f"  {'-'*24} {'-'*12} {'-'*8} {'-'*18} {'-'*8}")
            for item in r.get("items", []):
                n = item.get("name", "")[:24]
                v = str(item.get("value", ""))[:12]
                u = item.get("unit", "")[:8]
                ref = item.get("reference_range", "")[:18]
                s = item.get("status", "")
                flag = " ↑" if s == "high" else (" ↓" if s == "low" else "")
                print(f"  {n:<24} {v:<12} {u:<8} {ref:<18} {s}{flag}")

    print()


if __name__ == "__main__":
    main()
