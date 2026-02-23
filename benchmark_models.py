#!/usr/bin/env python3
"""
性能对比测试：Qwen/Qwen3-VL-32B-Instruct vs PaddlePaddle/PaddleOCR-VL-1.5
PaddleOCR 不支持 PDF，自动转 PNG 后测试。为公平起见，Qwen 也同时测试 PNG 输入。
"""

import base64, json, os, re, sys, time
import requests
from pathlib import Path

# 模型配置: 显示名, 模型ID, API地址, 环境变量密钥名, 是否支持PDF
MODEL_CONFIGS = [
    {
        "name": "Qwen3-VL-32B (硅基流动)",
        "model": "Qwen/Qwen3-VL-32B-Instruct",
        "api_url": "https://api.siliconflow.cn/v1/chat/completions",
        "key_env": "SILICONFLOW_API_KEY",
        "supports_pdf": True,
    },
    {
        "name": "Qwen3-VL-235B (硅基流动)",
        "model": "Qwen/Qwen3-VL-235B-A22B-Instruct",
        "api_url": "https://api.siliconflow.cn/v1/chat/completions",
        "key_env": "SILICONFLOW_API_KEY",
        "supports_pdf": True,
    },
    {
        "name": "Kimi-K2.5 (硅基流动/关思考)",
        "model": "Pro/moonshotai/Kimi-K2.5",
        "api_url": "https://api.siliconflow.cn/v1/chat/completions",
        "key_env": "SILICONFLOW_API_KEY",
        "supports_pdf": True,
        "extra_params": {"thinking": {"type": "disabled"}},
    },
]

SYSTEM_PROMPT = """你是一个专业的医疗检验报告识别助手。请从报告中提取以下信息，以严格的 JSON 格式返回，不要包含任何其他文字：
{
  "report_type": "报告类型",
  "hospital": "医院名称",
  "sample_date": "检查/采样日期，格式 YYYY-MM-DD",
  "report_date": "报告出具日期，格式 YYYY-MM-DD",
  "items": [
    {
      "name": "检查项名称",
      "value": "结果值",
      "unit": "单位",
      "reference_range": "参考范围",
      "status": "normal 或 high 或 low"
    }
  ]
}
注意：
- 只返回 JSON，不要有任何额外说明"""


def load_env():
    env_path = Path(__file__).parent / ".env"
    if env_path.exists():
        for line in env_path.read_text().splitlines():
            line = line.strip()
            if line and not line.startswith("#") and "=" in line:
                k, v = line.split("=", 1)
                os.environ.setdefault(k.strip(), v.strip())


def get_api_key(env_name: str) -> str:
    load_env()
    key = os.environ.get(env_name)
    if not key:
        print(f"错误: 未设置 {env_name} 环境变量")
        sys.exit(1)
    return key


def pdf_to_png_bytes(pdf_path: str) -> bytes:
    """将 PDF 第一页转为 PNG bytes"""
    import fitz
    doc = fitz.open(pdf_path)
    page = doc[0]
    pix = page.get_pixmap(matrix=fitz.Matrix(2, 2))
    return pix.tobytes("png")


def call_model(config: dict, file_path: str) -> dict:
    """调用模型，直接发送 PDF"""
    api_key = get_api_key(config["key_env"])
    api_url = config["api_url"]
    model = config["model"]

    with open(file_path, "rb") as f:
        b64 = base64.standard_b64encode(f.read()).decode()
    lower = file_path.lower()
    if lower.endswith(".png"):
        mime = "image/png"
    elif lower.endswith((".jpg", ".jpeg")):
        mime = "image/jpeg"
    else:
        mime = "application/pdf"
    data_url = f"data:{mime};base64,{b64}"

    payload = {
        "model": model,
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
    }
    # 额外参数（如关闭思考）
    if "extra_params" in config:
        payload.update(config["extra_params"])

    headers = {
        "Authorization": f"Bearer {api_key}",
        "Content-Type": "application/json",
    }

    start = time.time()
    try:
        resp = requests.post(api_url, headers=headers, json=payload, timeout=180)
        elapsed = time.time() - start
        resp.raise_for_status()
        data = resp.json()

        if "error" in data:
            return {"time": elapsed, "error": data["error"]["message"]}

        content = data["choices"][0]["message"]["content"]
        content = re.sub(r"<think>.*?</think>", "", content, flags=re.DOTALL)

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
                "items_count": len(parsed.get("items", [])),
                "items": parsed.get("items", []),
                "error": None,
            }
        else:
            return {"time": elapsed, "error": f"无法解析JSON: {content[:150]}"}

    except Exception as e:
        elapsed = time.time() - start
        return {"time": elapsed, "error": str(e)}


def main():
    load_env()

    # 选取 3 个不重名的测试文件
    uploads = Path("/home/liji/yiliao/backend/uploads")
    seen = set()
    test_files = []
    for f in sorted(uploads.glob("*.pdf")):
        suffix = f.name.split("_", 1)[-1]
        if suffix not in seen:
            seen.add(suffix)
            test_files.append(str(f))
        if len(test_files) >= 5:
            break

    if not test_files:
        print("未找到测试 PDF 文件")
        sys.exit(1)

    print("=== 模型性能对比测试 ===")
    print(f"测试文件数: {len(test_files)}")
    print(f"输入格式: 直接 PDF")
    for i, cfg in enumerate(MODEL_CONFIGS):
        print(f"模型 {chr(65+i)}: {cfg['name']} ({cfg['model']})")
    print("=" * 60)

    results = {cfg["name"]: [] for cfg in MODEL_CONFIGS}

    for i, fpath in enumerate(test_files, 1):
        fname = Path(fpath).name.split("_", 1)[-1]
        print(f"\n--- 文件 {i}: {fname} ---")

        for cfg in MODEL_CONFIGS:
            is_pdf = fpath.lower().endswith(".pdf")
            if is_pdf and not cfg.get("supports_pdf", True):
                print(f"  跳过 {cfg['name']} (不支持 PDF)")
                results[cfg["name"]].append({"time": 0, "error": "不支持 PDF", "skipped": True})
                continue

            print(f"  调用 {cfg['name']} ...", end=" ", flush=True)
            r = call_model(cfg, fpath)
            results[cfg["name"]].append(r)

            if r.get("error"):
                err_msg = r["error"][:80]
                print(f"❌ {err_msg} ({r['time']:.1f}s)")
            else:
                print(
                    f"✅ {r['time']:.1f}s | "
                    f"类型: {r['report_type']} | "
                    f"医院: {r['hospital']} | "
                    f"检查项: {r['items_count']}个"
                )

    # 汇总
    print("\n" + "=" * 60)
    print("=== 汇总对比 ===\n")

    for cfg in MODEL_CONFIGS:
        name = cfg["name"]
        rs = results[name]
        ok = [r for r in rs if not r.get("error")]
        times = [r["time"] for r in ok]
        items = [r["items_count"] for r in ok]

        print(f"【{name}】")
        print(f"  成功率: {len(ok)}/{len(rs)}")
        if times:
            print(f"  平均耗时: {sum(times)/len(times):.1f}s")
            print(f"  最快/最慢: {min(times):.1f}s / {max(times):.1f}s")
        if items:
            print(f"  平均识别项数: {sum(items)/len(items):.1f}")
        print()

    # 逐文件详细对比
    print("=== 逐文件详细对比 ===\n")
    for i, fpath in enumerate(test_files):
        fname = Path(fpath).name.split("_", 1)[-1]
        print(f"文件 {i+1}: {fname}")
        for cfg in MODEL_CONFIGS:
            name = cfg["name"]
            r = results[name][i]
            if r.get("error"):
                print(f"  {name}: ❌ 错误")
            else:
                print(f"  {name}: {r['time']:.1f}s | {r['items_count']}项 | {r['report_type']}")
                names = [item.get("name", "?") for item in r.get("items", [])]
                if names:
                    print(f"    项目: {', '.join(names)}")
        print()


if __name__ == "__main__":
    main()
