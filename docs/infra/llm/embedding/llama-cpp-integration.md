# 集成 llama.cpp Embedding 支持指南

## 概述

本文档说明如何通过 HTTP 方式使用 llama.cpp server 提供 embedding 服务。

**重要说明**：项目统一使用 HTTP 方式访问 llama.cpp，不使用本地库绑定。这样可以：
- 避免复杂的 C++ 编译环境配置
- 消除跨平台兼容性问题
- 保持架构简洁统一
- 支持独立部署和扩展

---

## 一、llama.cpp Server 概述

### 1.1 为什么使用 HTTP 方式

| 对比项 | HTTP 方式 | 本地库方式 |
|--------|----------|-----------|
| 编译复杂度 | 低（无需 C++ 编译） | 高（需要 CMake、Vulkan SDK 等） |
| 跨平台支持 | 优秀 | 困难 |
| 部署灵活性 | 可独立部署、扩展 | 进程内运行 |
| 维护成本 | 低 | 高 |
| 性能 | 足够（网络延迟非瓶颈） | 略优（无网络开销） |

### 1.2 llama.cpp Server 特性

llama.cpp 提供了高性能的 HTTP Server（`llama-server`），支持：

- **OpenAI 兼容接口**：Chat Completions、Embeddings、Responses
- **Anthropic Messages API** 兼容
- **多用户并发**：支持并行请求处理
- **GPU 加速**：CUDA、Metal、Vulkan、OpenVINO 等多后端支持
- **连续批处理**（Continuous Batching）：提升吞吐量
- **投机解码**（Speculative Decoding）：加速推理

---

## 二、启动 llama.cpp Server

### 2.1 基础启动命令

```bash
# 最简启动（仅聊天，无 embedding）
./llama-server -m model.gguf --port 8080

# 启用 Embedding 端点（关键参数）
./llama-server -m model.gguf --embedding --pooling cls --port 8080

# 完整配置示例（GPU 加速 + 多用户）
./llama-server \
    --model /path/to/model.gguf \
    --embedding \
    --pooling cls \
    --port 8080 \
    --host 0.0.0.0 \
    --ctx-size 8192 \
    --batch-size 512 \
    --parallel 4 \
    --cont-batching \
    --gpu-layers 99 \
    --metrics
```

### 2.2 关键启动参数说明

| 参数 | 简写 | 说明 | Embedding 必需性 |
|------|------|------|-----------------|
| `--model` | `-m` | 模型文件路径（GGUF 格式） | **必需** |
| `--embedding` | | 启用 embedding 端点 | **必需** |
| `--pooling` | | 池化策略：`none`, `cls`, `mean`, `last` | **必需**（非 `none`） |
| `--port` | | 监听端口（默认 8080） | 可选 |
| `--host` | | 监听地址（默认 127.0.0.1） | 可选 |
| `--ctx-size` | `-c` | 上下文窗口大小（token 数） | 推荐设置 |
| `--batch-size` | | 最大批处理大小 | 推荐设置 |
| `--parallel` | `-np` | 并行请求数（多用户） | 可选 |
| `--cont-batching` | | 启用连续批处理 | 推荐启用 |
| `--gpu-layers` | `-ngl` | 卸载到 GPU 的层数（99=全部） | GPU 可用时推荐 |
| `--metrics` | | 启用性能指标端点 | 可选（调试用） |
| `--flash-attn` | `-fa` | 启用 Flash Attention | 可选（提升性能） |

### 2.3 Pooling 策略说明

| Pooling 类型 | 说明 | 适用场景 |
|-------------|------|---------|
| `cls` | 使用 [CLS] token 的隐藏状态 | BERT 类模型 |
| `mean` | 对所有 token 输出取平均 | 通用 embedding |
| `last` | 使用最后一个 token 的输出 | 特定模型 |
| `none` | 返回所有 token 的未池化向量 | 需要 token 级嵌入时 |

**注意**：使用 `/v1/embeddings`（OpenAI 兼容端点）时，pooling **不能设置为 `none`**，否则该端点不可用。

---

## 三、Docker 部署

### 3.1 使用官方镜像

```bash
# 官方 Docker 镜像
docker run -d \
    --name llama-embedding \
    -p 8080:8080 \
    -v /path/to/models:/models \
    ghcr.io/ggml-org/llama.cpp:server \
    -m /models/nomic-embed-text-v1.5.Q8_0.gguf \
    --embedding \
    --pooling cls \
    --ctx-size 8192 \
    --batch-size 512 \
    --gpu-layers 0 \
    --host 0.0.0.0

# 查看日志
docker logs -f llama-embedding

# 健康检查
curl http://localhost:8080/health
```

### 3.2 GPU 支持（Docker）

```bash
# 使用 CUDA（需要 nvidia-docker）
docker run -d \
    --gpus all \
    --name llama-embedding-gpu \
    -p 8080:8080 \
    -v /path/to/models:/models \
    ghcr.io/ggml-org/llama.cpp:server-cuda \
    -m /models/nomic-embed-text-v1.5.Q8_0.gguf \
    --embedding \
    --pooling cls \
    --gpu-layers 99 \
    --host 0.0.0.0
```

---

## 四、项目配置

### 4.1 配置文件示例

```toml
# config.toml
[embedder]
# llama.cpp server 不需要 API Key，但需要占位
api_keys = ["no-key"]

# 指向 llama-server 的地址
base_url = "http://localhost:8080/v1"

# 模型名称（任意，llama.cpp 不验证）
model = "nomic-embed-text"

# 批处理配置
max_batch_tokens = 8192
max_item_tokens = 8192

# 超时配置
timeout_secs = 30
max_retries = 3
retry_delay_ms = 1000

# 编码格式
use_base64 = false  # llama.cpp 支持 float 和 base64

[embedder.preprocessor]
type = "none"
```

### 4.2 验证配置

启动项目后，可以通过以下方式验证：

```bash
# 测试 embedding 端点
curl http://localhost:9000/v1/embeddings \
    -H "Content-Type: application/json" \
    -d '{
        "input": "Hello world",
        "model": "nomic-embed-text"
    }'
```

---

## 五、Embedding API 说明

### 5.1 OpenAI 兼容端点

**端点**：`POST /v1/embeddings`

**请求格式**：

```bash
curl http://localhost:8080/v1/embeddings \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer your-api-key" \
    -d '{
        "input": "Hello world",
        "model": "text-embedding",
        "encoding_format": "float"
    }'
```

**请求参数**：

| 参数 | 类型 | 必需 | 说明 |
|------|------|------|------|
| `input` | string 或 array | **是** | 单个文本或文本数组 |
| `model` | string | 否 | 模型标识（可选，server 自动识别） |
| `encoding_format` | string | 否 | `"float"`（默认）或 `"base64"` |
| `dimensions` | integer | 否 | 截断嵌入维度（可选） |

**批量请求示例**：

```bash
curl http://localhost:8080/v1/embeddings \
    -H "Content-Type: application/json" \
    -d '{
        "input": ["First text", "Second text", "Third text"],
        "model": "text-embedding",
        "encoding_format": "float"
    }'
```

**响应格式**（OpenAI 兼容）：

```json
{
  "object": "list",
  "data": [
    {
      "object": "embedding",
      "index": 0,
      "embedding": [0.123, -0.456, 0.789, ...],
      "model": "text-embedding"
    },
    {
      "object": "embedding",
      "index": 1,
      "embedding": [0.234, -0.567, 0.890, ...],
      "model": "text-embedding"
    }
  ],
  "model": "text-encoding",
  "usage": {
    "prompt_tokens": 15,
    "total_tokens": 15
  }
}
```

**特点**：
- 返回 **欧几里得归一化**（L2 normalized）的嵌入向量
- 与 OpenAI API 完全兼容
- 支持 `base64` 编码（减少传输体积）

---

## 六、性能优化建议

### 6.1 Server 端优化

1. **启用 GPU 加速**：
   ```bash
   --gpu-layers 99  # 卸载所有层到 GPU
   ```

2. **启用连续批处理**：
   ```bash
   --cont-batching  # 提升多用户吞吐量
   ```

3. **调整批处理大小**：
   ```bash
   --batch-size 512  # 根据内存调整
   ```

4. **启用 Flash Attention**（如果支持）：
   ```bash
   --flash-attn
   ```

### 6.2 客户端优化

1. **调整批处理大小**：
   ```toml
   max_batch_tokens = 8192  # 根据模型调整
   ```

2. **启用 base64 编码**（减少传输体积）：
   ```toml
   use_base64 = true
   ```

3. **合理设置超时**：
   ```toml
   timeout_secs = 30  # 根据批大小调整
   ```

---

## 七、常见问题

### 7.1 如何选择模型？

推荐模型：
- **nomic-embed-text-v1.5**：通用 embedding，性能优秀
- **bge-m3**：多语言支持，适合中文
- **all-MiniLM-L6-v2**：轻量级，速度快

### 7.2 如何选择 pooling 策略？

- **cls**：BERT 类模型推荐
- **mean**：通用选择，适合大多数场景
- **last**：特定模型需要

### 7.3 性能不够怎么办？

1. 启用 GPU 加速（`--gpu-layers 99`）
2. 启用连续批处理（`--cont-batching`）
3. 增加并行数（`--parallel 4`）
4. 调整批处理大小（`--batch-size`）

### 7.4 内存不足怎么办？

1. 减小上下文窗口（`--ctx-size`）
2. 减小批处理大小（`--batch-size`）
3. 使用量化模型（Q4、Q8）
4. 减少 GPU 层数（`--gpu-layers`）

---

## 八、参考资源

- llama.cpp 官方仓库：https://github.com/ggml-org/llama.cpp
- llama.cpp Docker 镜像：https://github.com/ggml-org/llama.cpp/pkgs/container/llama.cpp
- GGUF 模型下载：https://huggingface.co/models?search=gguf
- OpenAI Embedding API 文档：https://platform.openai.com/docs/api-reference/embeddings

---

## 九、总结

通过 HTTP 方式使用 llama.cpp server 是推荐的方式，具有以下优势：

✅ **部署简单**：无需复杂的编译环境
✅ **跨平台兼容**：支持所有平台
✅ **架构统一**：与其他 provider 保持一致
✅ **易于扩展**：可独立部署、水平扩展
✅ **维护成本低**：无需处理 FFI 绑定问题

性能方面，对于 embedding 任务，网络延迟通常不是瓶颈，HTTP 方式的性能完全满足需求。
