# API 文档

本文档描述了 Code Context Engine 提供的所有 HTTP API 接口。

## 注意

## 所有索引 API **必须**提供 `project_id` 参数。

## 概述

Code Context Engine 提供了一组 RESTful API，用于代码索引、搜索、查询和管理。API 服务器基于 Axum 框架实现，所有业务逻辑位于 orchestrator 层。

## 基础信息

- **协议**: HTTP/1.1
- **数据格式**: JSON
- **字符编码**: UTF-8
- **基础路径**: `/api`

## API 分类

### 1. 索引操作 (Index Operations)

- [POST /api/index](./index.md#post-apiindex) - 执行完整索引
- [POST /api/index/incremental](./index.md#post-apiindexincremental) - 增量索引
- [POST /api/parse](./index.md#post-apiparse) - 解析单个文件

### 2. 搜索查询 (Search)

- [POST /api/search](./search.md#post-apisearch) - 执行搜索查询
- [POST /api/search/aggregated](./aggregated-search.md#post-apisearchaggregated) - 多查询并行检索

### 3. 实体查询 (Entity Queries)

- [GET /api/project/{project_id}/function/{id}](./entity.md#get-apiprojectproject_idfunctionid) - 获取函数详情
- [GET /api/project/{project_id}/function/{id}/calls](./entity.md#get-apiprojectproject_idfunctionidcalls) - 获取函数调用关系
- [GET /api/project/{project_id}/function/{id}/callers](./entity.md#get-apiprojectproject_idfunctionidcallers) - 获取函数被调用关系
- [GET /api/project/{project_id}/call-chain/{id}](./entity.md#get-apiprojectproject_idcall-chainid) - 获取调用链
- [GET /api/project/{project_id}/call-path](./entity.md#get-apiprojectproject_idcall-path) - 查询调用路径
- [GET /api/project/{project_id}/class/{id}/inheritance](./entity.md#get-apiprojectproject_idclassidinheritance) - 获取类继承关系
- [GET /api/project/{project_id}/class/{id}/implementations](./entity.md#get-apiprojectproject_idclassidimplementations) - 获取类实现关系
- [POST /api/entities/search](./entity-search.md#post-apientitiessearch) - 实体全文搜索（FTS5）

### 4. 存储管理 (Storage Management)

- [DELETE /api/index](./storage.md#delete-apiindex) - 清空索引
- [DELETE /api/index/file/:file_path](./storage.md#delete-apiindexfilefile_path) - 删除文件索引
- [DELETE /api/index/entity/:id](./storage.md#delete-apiindexentityid) - 删除实体
- [DELETE /api/index/batch](./storage.md#delete-apiindexbatch) - 批量删除
- [GET /api/index/stats](./storage.md#get-apiindexstats) - 获取索引统计
- [GET /api/storage/status](./storage.md#get-apistoragestatus) - 获取存储状态

### 5. 项目管理 (Project Management)

- [POST /api/project](./project.md#post-apiproject) - 创建项目
- [GET /api/project](./project.md#get-apiproject) - 列出所有项目
- [GET /api/project/:id](./project.md#get-apiprojectid) - 获取项目详情
- [PUT /api/project/:id](./project.md#put-apiprojectid) - 更新项目
- [DELETE /api/project/:id](./project.md#delete-apiprojectid) - 删除项目
- [POST /api/project/:id/index](./project.md#post-apiprojectidindex) - 索引项目
- [POST /api/project/:id/reload](./project-config.md#post-apiprojectidreload) - 重新加载配置
- [PUT /api/project/:id/config](./project-config.md#put-apiprojectidconfig) - 更新项目配置

### 6. 文件摘要 (File Summary)

- [POST /api/summary](./summary.md#post-apisummary) - 生成文件摘要

### 7. 热重载 (Hot Reload)

- [POST /api/project/{project_id}/watch/start](./watch.md#post-apiprojectproject_idwatchstart) - 启动文件监视
- [POST /api/project/{project_id}/watch/stop](./watch.md#post-apiprojectproject_idwatchstop) - 停止文件监视
- [GET /api/project/{project_id}/watch/status](./watch.md#get-apiprojectproject_idwatchstatus) - 获取监视状态

### 8. 配置管理 (Configuration Management)

- [POST /api/config/reload](./config-reload.md#post-apiconfigreload) - 全局配置重载

### 9. 工具 API (Tools)

- [POST /api/tools/compress](./tools.md#post-apitoolscompress) - 压缩代码
- [POST /api/tools/compress/batch](./tools.md#post-apitoolscompressbatch) - 批量压缩
- [POST /api/tools/diagnose](./tools.md#post-apitoolsdiagnose) - 诊断代码
- [POST /api/tools/keyword-search](./tools.md#post-apitoolskeyword-search) - 关键词搜索
- [POST /api/tools/symbols](./tools.md#post-apitoolssymbols) - 获取符号信息
- [POST /api/tools/references](./tools.md#post-apitoolsreferences) - 查找引用
- [POST /api/tools/definition](./tools.md#post-apitoolsdefinition) - 跳转到定义

### 10. 指标导出 (Metrics)

- [GET /api/metrics](./metrics.md#get-apimetrics) - Prometheus 格式指标
- [GET /api/metrics/json](./metrics.md#get-apimetricsjson) - JSON 格式指标
- [GET /api/metrics/history](./metrics-history.md#get-apimetricshistory) - 查询历史指标
- [DELETE /api/metrics/cleanup](./metrics-history.md#delete-apimetricscleanup) - 清理历史指标

### 11. 健康监控 (Health)

- [GET /api/health](./health.md#get-apihealth) - 统一健康检查
- [GET /api/health/qdrant](./health.md#get-apihealthqdrant) - Qdrant 详细诊断
- [GET /api/health/embedding](./health.md#get-apiheathembedding) - Embedding 服务健康
- [GET /api/health/bm25](./health.md#get-apihealthbm25) - BM25 索引健康

### 12. 重试队列管理 (Retry Queue)

- [GET /api/retry-queue](./health.md#get-apiretry-queue) - 重试队列状态
- [POST /api/retry-queue/process](./health.md#post-apiretry-queueprocess) - 手动触发重试处理
- [DELETE /api/retry-queue](./health.md#delete-apiretry-queue) - 清空重试队列

## 通用响应格式

### 成功响应

```json
{
  "success": true,
  "data": { ... }
}
```

### 错误响应

```json
{
  "success": false,
  "error": {
    "code": "ERROR_CODE",
    "message": "Error description",
    "details": "Additional details (optional)"
  }
}
```

## 错误代码

| 错误代码                | 描述           |
| ----------------------- | -------------- |
| `INVALID_REQUEST`       | 无效的请求参数 |
| `INVALID_INPUT`         | 无效的输入数据 |
| `ENTITY_NOT_FOUND`      | 实体不存在     |
| `INDEX_NOT_INITIALIZED` | 索引未初始化   |
| `PARSE_ERROR`           | 解析错误       |
| `STORAGE_ERROR`         | 存储错误       |
| `QUERY_ERROR`           | 查询错误       |
| `INTERNAL_ERROR`        | 内部错误       |

## 性能考虑

- 搜索和查询操作通常立即返回
- 建议使用分页限制结果数量
- 热重载功能会持续消耗系统资源
