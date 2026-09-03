# 硅基流动 Rerank 重排序 API
POST https://api.siliconflow.cn/v1/rerank
Authorization: Bearer {{SILICONFLOW_API_KEY}}
Content-Type: application/json
{
    "model": "BAAI/bge-reranker-v2-m3",
    "query": "Apple",
    "documents": ["apple", "banana", "fruit", "vegetable"],
    "return_documents": true,
    "top_n": 4
}

# 断言接口正常返回 200
HTTP 200