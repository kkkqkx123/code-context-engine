import json
import os
from collections import defaultdict, OrderedDict

# Resolve paths relative to this script so it can run from anywhere.
SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
REPO_ROOT = os.path.abspath(os.path.join(SCRIPT_DIR, "..", "..", ".."))
OPENAPI_BASE = os.path.join(REPO_ROOT, "tools", "openapi-codegen", "openapi.json")
OPENAPI_ADMISSION = os.path.join(REPO_ROOT, "tools", "openapi-codegen", "openapi-admission.json")
OUT = SCRIPT_DIR

base_doc = json.load(open(OPENAPI_BASE, encoding="utf-8"))

admission_doc = None
if os.path.exists(OPENAPI_ADMISSION):
    admission_doc = json.load(open(OPENAPI_ADMISSION, encoding="utf-8"))


def schema_name(obj):
    if not isinstance(obj, dict):
        return ""
    if "$ref" in obj:
        return obj["$ref"].split("/")[-1]
    if "items" in obj:
        inner = schema_name(obj["items"])
        return ("array[" + inner + "]") if inner else "array"
    if "type" in obj:
        return obj["type"]
    return ""


def params_summary(op):
    parts = []
    for param in op.get("parameters", []) or []:
        loc = {"query": "q", "path": "p", "header": "h"}.get(param.get("in", ""), "")
        name = param.get("name", "")
        parts.append(f"{name} ({loc})" if loc else name)

    if "requestBody" in op:
        content = (op.get("requestBody", {}).get("content", {}) or {})
        schema = {}
        if "application/json" in content:
            schema = content["application/json"].get("schema", {}) or {}
        elif content:
            schema = next(iter(content.values())).get("schema", {}) or {}
        model = schema_name(schema)
        body = "body" + (f" ({model})" if model else "")
        parts.append(body)

    return ", ".join(parts)


def esc(value):
    return str(value).replace("|", "\\|").replace("\n", " ")


# Collect operations from the base snapshot.
base_keys = set()
ops = []
for path, methods in base_doc.get("paths", {}).items():
    for method, op in methods.items():
        if method.lower() not in ("get", "post", "put", "patch", "delete"):
            continue
        key = (method.upper(), path)
        base_keys.add(key)
        ops.append(
            {
                "method": method.upper(),
                "path": path,
                "tag": (op.get("tags") or ["Untagged"])[0],
                "op": op,
                "admission": False,
            }
        )

# Merge operations that only exist in the admission snapshot.
if admission_doc is not None:
    for path, methods in admission_doc.get("paths", {}).items():
        for method, op in methods.items():
            if method.lower() not in ("get", "post", "put", "patch", "delete"):
                continue
            key = (method.upper(), path)
            if key in base_keys:
                continue
            ops.append(
                {
                    "method": method.upper(),
                    "path": path,
                    "tag": (op.get("tags") or ["Untagged"])[0],
                    "op": op,
                    "admission": True,
                }
            )

# Group by OpenAPI tag.
groups = defaultdict(list)
for item in ops:
    groups[item["tag"]].append(item)

# Logical tag order and file names.
tag_order = [
    "Index",
    "Summary",
    "Storage",
    "Project",
    "Entity",
    "Graph",
    "Metrics",
    "Watch",
    "Config",
    "Qdrant",
    "Search",
    "Tools",
    "Health",
    "Ingest",
    "Admission",
]
for tag in groups:
    if tag not in tag_order:
        tag_order.append(tag)

tag_files = {
    "Index": "02-index.md",
    "Summary": "03-summary.md",
    "Storage": "04-storage.md",
    "Project": "05-project.md",
    "Entity": "06-entity.md",
    "Graph": "07-graph.md",
    "Metrics": "08-metrics.md",
    "Watch": "09-watch.md",
    "Config": "10-config.md",
    "Qdrant": "11-qdrant.md",
    "Search": "12-search.md",
    "Tools": "13-tools.md",
    "Health": "14-health.md",
    "Ingest": "15-ingest.md",
    "Admission": "16-admission.md",
}

# Assign file names to any tag not in the explicit mapping.
next_number = 17
for tag in tag_order:
    if tag not in tag_files:
        tag_files[tag] = f"{next_number:02d}-{tag.lower()}.md"
        next_number += 1

os.makedirs(OUT, exist_ok=True)

# Generate one markdown file per tag.
for tag in tag_order:
    items = sorted(groups[tag], key=lambda x: (x["path"], x["method"]))
    lines = []
    lines.append(f"# {tag} 域 HTTP 端点\n")
    lines.append(f"> 操作数：{len(items)}\n")
    lines.append("| Method | Path | Handler (operationId) | Parameters / Body |\n")
    lines.append("|--------|------|------------------------|-------------------|\n")
    for item in items:
        op = item["op"]
        operation_id = op.get("operationId", "")
        params = params_summary(op)
        suffix = "  (admission)" if item["admission"] else ""
        lines.append(
            f"| {item['method']} | `{item['path']}`{suffix} | `{operation_id}` | {esc(params)} |\n"
        )
    with open(os.path.join(OUT, tag_files[tag]), "w", encoding="utf-8") as fh:
        fh.write("".join(lines))

# Base counts for the README.
base_path_count = len(base_doc.get("paths", {}))
base_op_count = sum(len(v) for v in base_doc.get("paths", {}).values())
admission_extra_paths = 0
admission_extra_ops = 0
if admission_doc is not None:
    base_key_set = set()
    for path, methods in base_doc.get("paths", {}).items():
        for method in methods:
            base_key_set.add((method.upper(), path))
    for path, methods in admission_doc.get("paths", {}).items():
        for method in methods:
            if (method.upper(), path) not in base_key_set:
                admission_extra_ops += 1
                admission_extra_paths += 1 if path not in [p for (m, p) in base_key_set] else 0
    # path count: unique paths in admission not in base
    admission_extra_paths = len(
        set(admission_doc.get("paths", {})) - set(base_doc.get("paths", {}))
    )

readme = []
readme.append("# Server HTTP API 清单\n")
readme.append("")
readme.append("> 范围：`crates/app/cce-server` HTTP 传输层暴露的全部端点。")
readme.append(
    f"> 来源：`tools/openapi-codegen/openapi.json`（OpenAPI 3.0 快照，{base_op_count} 个操作 / {base_path_count} 个路径）。"
)
if admission_doc is not None:
    readme.append(
        f"> admission 构建（`--features admission`）额外 {admission_extra_ops} 个操作 / {admission_extra_paths} 个路径。"
    )
readme.append("> 基础前缀：`/api`（控制台日志与 WebSocket 单独列出）。")
readme.append("")
readme.append("## 通用约定\n")
readme.append("- JSON 响应统一为 `{ \"success\": bool, \"data\": ..., \"error\": { \"code\", \"message\", \"details?\" } }`。")
readme.append("- 项目作用域端点使用路径参数 `{project_id}`（整数项目 ID）。")
readme.append("- admission 构建下，远程部署需要配置 `CCE_ADMISSION_TOKENS`；未配置时回退为本地路由。")
readme.append("- 控制台专用端点（日志、WebSocket）见 `console-api.md`，不在 OpenAPI 契约中。")
readme.append("- MCP 工具见 `mcp-api.md`。")
readme.append("")
readme.append("## 文档索引\n")
readme.append("| 文件 | 域 | 操作数 |")
readme.append("|------|----|--------|")
for tag in tag_order:
    readme.append(f"| [{tag_files[tag]}]({tag_files[tag]}) | {tag} | {len(groups[tag])} |")
readme.append("")
readme.append("## 补充文档\n")
readme.append("| 文件 | 说明 |")
readme.append("|------|------|")
readme.append("| [console-api.md](console-api.md) | 控制台专用日志与 WebSocket 端点 |")
readme.append("| [mcp-api.md](mcp-api.md) | MCP 工具表面 |")
readme.append("")
readme.append("## 生成说明\n")
readme.append("")
readme.append("本目录由 `docs/app/server/generate-api-list.py` 从 `tools/openapi-codegen/openapi*.json` 自动生成；端点变更后请重新运行该脚本。")
readme.append("")

with open(os.path.join(OUT, "README.md"), "w", encoding="utf-8") as fh:
    fh.write("\n".join(readme))

print(f"generated {len(ops)} operations across {len(tag_order)} tags -> {OUT}")
