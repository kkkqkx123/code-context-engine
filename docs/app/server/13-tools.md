# Tools 域 HTTP 端点
> 操作数：9
| Method | Path | Handler (operationId) | Parameters / Body |
|--------|------|------------------------|-------------------|
| POST | `/api/tools/compress` | `handle_compress` | body (CompressRequest) |
| POST | `/api/tools/compress/batch` | `handle_compress_batch` | body (BatchCompressRequest) |
| POST | `/api/tools/definition` | `handle_goto_definition` | body (GotoDefinitionRequest) |
| POST | `/api/tools/diagnose` | `handle_diagnose` | body (DiagnoseRequest) |
| POST | `/api/tools/fold` | `handle_fold` | body (FoldRequest) |
| POST | `/api/tools/fold/batch` | `handle_fold_batch` | body (BatchFoldRequest) |
| POST | `/api/tools/keyword-search` | `handle_keyword_search` | body (KeywordSearchRequest) |
| POST | `/api/tools/references` | `handle_find_references` | body (FindReferencesRequest) |
| POST | `/api/tools/symbols` | `handle_get_symbols` | body (GetSymbolsRequest) |
