# 测试 llama-server 嵌入向量 API
POST http://127.0.0.1:8080/v1/embeddings
Content-Type: application/json

{"input":"这是一个测试"}
HTTP 200

# 捕获返回的向量（可选）
[Captures]
embedding_data: jsonpath "$.data[0].embedding"

# 核心断言（验证服务完全可用）
[Asserts]
jsonpath "$.object" == "list"               # 返回类型正确
jsonpath "$.data" exists                    # 数据存在
jsonpath "$.data[0].object" == "embedding"  # 嵌入对象正确
jsonpath "$.data[0].embedding" exists       # 向量生成成功
jsonpath "$.model" contains "all-MiniLM"    # 模型正确加载
duration < 2000                             # 响应速度正常（2秒内）