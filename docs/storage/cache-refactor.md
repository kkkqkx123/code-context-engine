# 缓存架构重构方案

## 1. 现状分析

### 1.1 当前架构

项目现有的热更新系统包含以下核心组件：

#### 内存层：FileCache (`src/scanner/cache.rs`)

**职责**：周期性全量扫描时的变化检测
- 存储：纯内存 `HashMap<PathBuf, CachedFileMeta>`
- 元信息：`path`, `content_hash`, `size`, `modified_time`
- 生命周期：重启丢失，每次初始化需重建
- 使用场景：
  - `ChangeDetector::check_changes()` - 快速检测是否有变化
  - `ChangeDetector::scan_and_detect()` - 详细扫描并返回变更列表

**关键方法**：
```rust
pub fn check_changed(&mut self, path: &Path, size: u64, modified: DateTime<Utc>) -> ChangeStatus
```

#### 持久化层：CacheRepository (`src/storage/sqlite/repo/cache_repo.rs`)

**职责**：存储完整的 ParsedFile（序列化后）
- 表结构：`cache (file_hash PK, file_path, language, cached_data BLOB, ...)`
- 粒度：**文件级别**（整个解析结果）
- 问题：
  - ❌ 存储整个 ParsedFile，无法细粒度更新实体
  - ❌ 与 FileRepository 职责重叠（都存文件信息）
  - ❌ 未被 ChangeDetector 使用（仅内存 FileCache）

#### 基础元信息：FileRepository (`src/storage/sqlite/repo/file_repo.rs`)

**职责**：存储文件基础信息
- 表结构：`files (id, path, language, last_modified, created_at, project_id)`
- 用途：RelationIndex 的持久化支持
- 局限：缺少 `content_hash`、`file_size` 等字段

### 1.2 热更新流程

```rust
// src/orchestrator/hot_update/coordinator.rs

pub async fn update(&mut self) -> Result<BatchChangeResult> {
    // Step 1: 扫描并检测变化（使用内存 FileCache）
    let cache_result = self.scan_and_detect_changes().await?;
    
    if !cache_result.has_changes() {
        return Ok(result);  // 无变化直接返回
    }
    
    // Step 2: 处理删除的文件
    for path in &cache_result.removed { ... }
    
    // Step 3: 解析变化的文件（added + modified）
    for path in changed_paths {
        let parse_result = self.process_file_change(&path, &cache_result).await?;
        result.add_parse_result(parse_result);
    }
    
    // Step 4: 更新 FileCache
    self.update_cache(&cache_result).await;
    
    Ok(result)
}
```

**防抖机制**（`GlobalDebounce`）：
- `pending_interval`: 30秒（默认）- 有变化时的短间隔
- `max_wait_time`: 5分钟（默认）- 高频修改的安全上限
- **重要**：高频修改通过限流和缓存合并解决，不应排除

**模式切换**（`ModeStateMachine`）：
- File Watch 模式：实时监听文件系统事件
- Periodic Scan 模式：降级为周期性扫描
- Storm 检测：事件频率超过阈值时自动降级

### 1.3 核心问题

| 问题 | 影响 |
|------|------|
| **三层缓存冗余** | FileCache（内存）、CacheRepository（SQLite）、FileRepository（SQLite）职责不清 |
| **CacheRepository 未被使用** | ChangeDetector 只用内存 FileCache，CacheRepository 孤立存在 |
| **无法细粒度更新** | CacheRepository 存储整个 ParsedFile，实体变化需重新解析整个文件 |
| **缺乏灵活过滤** | 只能通过 scanner.exclude_patterns 过滤，不支持文件大小、语言等维度 |
| **重启性能差** | FileCache 纯内存，每次启动需全量扫描重建 |

---

## 2. 重构目标

### 2.1 核心洞察

经过仔细分析现有代码，发现：

1. **files 表已存在**：已有 `path`, `language`, `last_modified`, `created_at`, `project_id`
2. **只需添加 content_hash**：扩展 files 表即可支持基于 hash 的变化检测
3. **file_size 无需持久化**：扫描时从文件系统读取，仅用于临时比较
4. **CacheRepository 已实现文件级缓存**：存储完整 ParsedFile，但未被 ChangeDetector 使用

### 2.2 简化后的方案

**不再创建 FileMetadataStore**，而是：

1. **扩展 files 表**：添加 `content_hash TEXT` 字段
2. **复用 CacheRepository**：改造为细粒度的实体缓存
3. **修改 ChangeDetector**：直接使用 SQLite 查询 + 内存索引

### 2.3 目标架构

```
┌─────────────────────────────────────┐
│  HotUpdateCoordinator               │
│  - GlobalDebounce（已有）            │
│  - ModeStateMachine（已有）          │
│  - ExcludeRules（新增）              │ ← orchestrator/hot_update/
└──────────┬──────────────────────────┘
           │
           ├──────────────────────────────────────┐
           │                                      │
┌──────────▼──────────┐              ┌───────────▼────────────┐
│ ChangeDetector      │              │ EntityCacheRepository  │
│ (修改)              │              │ (新建，替换CacheRepo)  │
│ - 内存索引:         │              │ - SQLite:              │
│   HashMap<Path,Hash>│              │   entity_cache         │
│ - SQLite查询:       │              │ - BLOB: rkyv + zstd    │
│   SELECT hash FROM  │              │ - 覆盖式更新            │
│   files WHERE path=?│              └────────────────────────┘
└──────────┬──────────┘                       ↑
           │                                  │
           └────────── 查询 ──────────────────┘
                      ↓
         ┌──────────────────────┐
         │ files 表（扩展）      │
         │ + content_hash 字段  │
         └──────────────────────┘
```

**优势**：
- ✅ 不创建新表，仅扩展 files 表
- ✅ 细粒度实体更新（无需重新解析整个文件）
- ✅ 灵活的排除规则（路径、大小、语言等）
- ✅ 重启可用（SQLite 持久化）

---

## 3. 模块设计

### 3.1 模块位置

```
src/
├── utils/
│   └── glob.rs                        # ✅ Glob 匹配工具（已有，复用）
│
├── storage/                           # ✅ 仅数据存储实现
│   └── sqlite/
│       ├── repo/
│       │   ├── file_repo.rs           # 🔧 修改：添加 content_hash 字段
│       │   ├── entity_cache_repo.rs   # 🆕 新建：实体缓存 CRUD
│       │   └── mod.rs
│       └── mod.rs
│
└── orchestrator/                      # ✅ 业务流程编排
    └── hot_update/
        ├── exclude_rules.rs           # 🆕 新建：排除规则匹配
        ├── change_detector.rs         # 🔧 修改：集成 ExcludeRules + SQLite查询
        ├── coordinator.rs             # ⚙️ 保持不变
        ├── debounce.rs                # ⚙️ 保持不变
        ├── mode_switch.rs             # ⚙️ 保持不变
        └── mod.rs
```

### 3.2 Storage 目录职责边界

**应该包含**：
- ✅ 数据库连接管理
- ✅ Repository 模式实现（CRUD）
- ✅ 序列化/反序列化
- ✅ 事务管理

**不应该包含**：
- ❌ 业务逻辑（排除规则匹配）
- ❌ 编排逻辑（变化检测流程）
- ❌ 工具函数（glob 匹配）

### 3.3 files 表扩展

#### 当前结构

```sql
CREATE TABLE files (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    path TEXT NOT NULL,
    language TEXT NOT NULL,
    last_modified INTEGER NOT NULL,  -- Unix timestamp
    created_at INTEGER NOT NULL,
    project_id INTEGER
);
```

#### 扩展后结构

```sql
ALTER TABLE files ADD COLUMN content_hash TEXT;

CREATE INDEX idx_files_path ON files(path);
CREATE INDEX idx_files_hash ON files(content_hash);
```

**为什么不需要 file_size？**
- `file_size` 仅用于临时变化检测（对比文件系统）
- 扫描时从 `FileEntry.size` 直接读取，无需持久化
- 持久化会增加更新开销（每次文件修改都要更新 size）

#### ChangeDetector 集成

**当前实现**：
```rust
// ChangeDetector::scan_and_detect()
let entries = scanner.scan(&self.scan_options)?;
for entry in &entries {
    self.file_cache.check_changed(&entry.path, entry.size, entry.modified)
}
```

**重构后实现**：
```rust
// ChangeDetector::scan_and_detect()
let entries = scanner.scan(&self.scan_options)?;

for entry in &entries {
    // 1. 检查排除规则
    if self.exclude_rules.should_exclude(entry) {
        continue;
    }
    
    // 2. 查询 SQLite 获取缓存的 hash
    let cached_hash = self.db.get_file_hash(&entry.path).await?;
    
    // 3. 对比 hash（如果不同则文件已修改）
    if cached_hash != entry.content_hash {
        changed_files.push(entry.clone());
        // 更新 SQLite 中的 hash
        self.db.update_file_hash(&entry.path, &entry.content_hash).await?;
    }
}
```

**内存索引优化**：
```rust
pub struct ChangeDetector {
    db: Arc<SqliteDatabase>,
    hash_cache: Arc<RwLock<HashMap<PathBuf, String>>>, // 简单 HashMap
}

impl ChangeDetector {
    async fn get_cached_hash(&self, path: &Path) -> Option<String> {
        // 先查内存
        if let Some(hash) = self.hash_cache.read().await.get(path) {
            return Some(hash.clone());
        }
        
        // 再查 SQLite
        if let Some(hash) = self.db.get_file_hash(path).await? {
            self.hash_cache.write().await.insert(path.to_path_buf(), hash.clone());
            return Some(hash);
        }
        
        None
    }
}
```

**性能分析**：
- 10,000 文件的 hash 缓存 ≈ 400KB 内存（每个 hash 40字节）
- SQLite 查询 P95 < 1ms（有索引）
- 内存命中 P95 < 1μs

### 3.4 EntityCacheRepository 设计

#### 表结构

```sql
CREATE TABLE entity_cache (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    entity_id INTEGER NOT NULL,
    file_id INTEGER NOT NULL REFERENCES file_metadata(id),
    entity_kind TEXT NOT NULL,      -- Function, Class, Method, etc.
    entity_name TEXT NOT NULL,
    signature TEXT,
    
    serialized_data BLOB NOT NULL,  -- rkyv + zstd
    original_size INTEGER,
    compressed_size INTEGER,
    
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    
    UNIQUE(entity_id, file_id)
);

CREATE INDEX idx_entity_cache_by_file ON entity_cache(file_id);
CREATE INDEX idx_entity_cache_by_kind ON entity_cache(entity_kind);
```

#### 覆盖式更新

```rust
impl EntityCacheRepository {
    /// 替换文件的所有实体（先删后增，事务保证原子性）
    pub async fn replace_file_entities(
        &self,
        file_id: i64,
        entities: &[Entity],
    ) -> Result<()> {
        let tx = self.db.begin()?;
        
        // 1. 删除旧数据
        execute_update(
            &tx,
            "DELETE FROM entity_cache WHERE file_id = ?1",
            params![file_id],
        )?;
        
        // 2. 插入新数据
        for entity in entities {
            self.upsert_entity(&tx, entity).await?;
        }
        
        tx.commit()?;
        Ok(())
    }
    
    /// 清理孤立实体（file_id 不存在于 file_metadata 中）
    pub async fn cleanup_orphaned_entities(&self) -> Result<usize> {
        let tx = self.db.begin()?;
        let count = execute_update(
            &tx,
            "DELETE FROM entity_cache WHERE file_id NOT IN (SELECT id FROM file_metadata)",
            params![],
        )?;
        tx.commit()?;
        Ok(count)
    }
}
```

**为什么不需要复杂的清理？**
- 覆盖式更新（DELETE + INSERT）保证每个文件只有最新版本的实体
- `cleanup_orphaned_entities()` 仅在删除文件后调用，清理孤立数据
- 不做版本管理，不保留历史版本

### 3.5 ExcludeRules 设计

#### 配置示例

```toml
[hot_update.exclude]
enabled = true

rules = [
    # 路径模式过滤
    { type = "path_pattern", pattern = "**/*.pb.rs" },
    { type = "path_pattern", pattern = "**/target/**" },
    
    # 文件大小过滤
    { type = "file_size", max_bytes = 1048576 },  # 1MB
    
    # 语言类型过滤
    { type = "language", languages = ["proto", "graphql"] },
    
    # 组合条件（AND 逻辑）
    { 
        type = "composite",
        conditions = [
            { type = "path_pattern", pattern = "**/bindings/*.rs" },
            { type = "file_size", max_bytes = 524288 }  # 512KB
        ]
    },
]
```

#### 实现

```rust
// src/orchestrator/hot_update/exclude_rules.rs

use crate::utils::glob::Glob;  // 复用现有的 Glob 实现

pub struct ExcludeRules {
    config: HotUpdateExcludeConfig,
    compiled_globs: Vec<Glob>,  // 预编译的 glob 模式
}

impl ExcludeRules {
    /// 判断文件是否应该被排除
    pub fn should_exclude(&self, file_entry: &FileEntry) -> bool {
        if !self.config.enabled {
            return false;
        }
        
        for rule in &self.config.rules {
            if self.matches_rule(rule, file_entry) {
                return true;
            }
        }
        
        false
    }
    
    fn matches_rule(&self, rule: &ExcludeRule, entry: &FileEntry) -> bool {
        match rule.rule_type {
            RuleType::PathPattern => {
                // 使用 Glob 进行路径匹配
                self.compiled_globs.iter().any(|g| g.is_match(&entry.path))
            }
            RuleType::FileSize => {
                entry.size > rule.max_bytes.unwrap_or(u64::MAX)
            }
            RuleType::Language => {
                entry.language_info.as_ref()
                    .map(|lang| rule.languages.contains(&lang.name))
                    .unwrap_or(false)
            }
            RuleType::Composite => {
                // AND 逻辑：所有子条件都满足
                rule.conditions.iter()
                    .all(|cond| self.matches_rule(cond, entry))
            }
        }
    }
}
```

**集成到 ChangeDetector**：
```rust
// ChangeDetector::scan_and_detect()
let entries = scanner.scan(&self.scan_options)?;

for entry in &entries {
    // 1. 检查排除规则
    if self.exclude_rules.should_exclude(entry) {
        tracing::debug!(path = %entry.path.display(), "Excluded by rules");
        continue;
    }
    
    // 2. 检查文件变化
    if self.metadata_store.check_changed(entry).is_changed() {
        changed_files.push(entry.clone());
    }
}
```

---

## 4. 数据库设计

### 4.1 files 表扩展

**当前结构**：
```sql
CREATE TABLE files (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    path TEXT NOT NULL,
    language TEXT NOT NULL,
    last_modified INTEGER NOT NULL,  -- Unix timestamp
    created_at INTEGER NOT NULL,
    project_id INTEGER
);
```

**扩展后**：
```sql
ALTER TABLE files ADD COLUMN content_hash TEXT;

CREATE INDEX idx_files_path ON files(path);
CREATE INDEX idx_files_hash ON files(content_hash);
```

**迁移脚本**：
```sql
-- 添加字段
ALTER TABLE files ADD COLUMN content_hash TEXT;

-- 为已有记录设置默认值（可选）
UPDATE files SET content_hash = '' WHERE content_hash IS NULL;

-- 创建索引
CREATE INDEX IF NOT EXISTS idx_files_path ON files(path);
CREATE INDEX IF NOT EXISTS idx_files_hash ON files(content_hash);
```

### 4.2 entity_cache 表

```sql
CREATE TABLE entity_cache (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    entity_id INTEGER NOT NULL,
    file_id INTEGER NOT NULL REFERENCES file_metadata(id),
    entity_kind TEXT NOT NULL,
    entity_name TEXT NOT NULL,
    signature TEXT,
    
    serialized_data BLOB NOT NULL,  -- rkyv + zstd
    original_size INTEGER,
    compressed_size INTEGER,
    
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    
    UNIQUE(entity_id, file_id)
);

CREATE INDEX idx_entity_cache_by_file ON entity_cache(file_id);
CREATE INDEX idx_entity_cache_by_kind ON entity_cache(entity_kind);
```

---

## 5. 实施计划

### 5.1 时间线（总计 6-9 周）

```
Week 1-3:  阶段 1 - files 表扩展 + ChangeDetector 修改
Week 4-7:  阶段 2 - EntityCacheRepository
Week 8-9:  阶段 3 - ExcludeRules + 优化
```

### 5.2 各阶段交付物

#### 阶段 1: files 表扩展（3 周）

**任务**：
1. 修改 `FileRecord` 结构，添加 `content_hash` 字段
2. 修改 `FileRepository`，支持 hash 的 CRUD
3. 数据库迁移脚本（ALTER TABLE）
4. 修改 `ChangeDetector` 集成 SQLite 查询
5. 删除 `FileCache` 依赖

**验收标准**：
- 查询延迟 P95 < 1ms
- 单元测试覆盖率 > 80%
- 无功能退化

#### 阶段 2: EntityCacheRepository（4 周）

**任务**：
1. 创建 `EntityCacheRepository` 模块 (`src/storage/sqlite/repo/entity_cache_repo.rs`)
2. 实现覆盖式更新逻辑
3. 添加 `cleanup_orphaned_entities()` 方法
4. 数据迁移工具（CacheRepository → EntityCache）
5. 删除 `CacheRepository`

**验收标准**：
- CacheRepository 完全移除
- 实体查询性能不低于原有水平
- 数据完整性验证通过

#### 阶段 3: ExcludeRules + 优化（2 周）

**任务**：
1. 创建 `ExcludeRules` 模块 (`src/orchestrator/hot_update/exclude_rules.rs`)
2. 复用 `src/utils/glob.rs` 的 Glob 实现
3. 集成到 `ChangeDetector`
4. 增加监控指标
5. 完善文档

**验收标准**：
- 排除规则配置正常工作
- 监控指标准确可靠
- 文档完整更新

---

## 6. 风险评估

| 风险项 | 概率 | 影响 | 缓解措施 |
|--------|------|------|----------|
| 性能退化 | 中 | 高 | 保留内存索引，充分基准测试 |
| 数据丢失 | 低 | 极高 | 迁移前备份 + 双写验证 + 回滚脚本 |
| 兼容性问题 | 中 | 中 | 保留旧 API 过渡期（2 周） |
| 开发延期 | 中 | 中 | 分阶段交付，每阶段可独立上线 |

---

## 7. 命名规范遵循

严格遵守 Module Design Guidelines：

✅ **正确命名**（描述职责）：
- `FileMetadataStore`
- `EntityCacheRepository`
- `ExcludeRules`

❌ **禁止命名**（暗示比较）：
- ~~`OptimizedFileRepository`~~
- ~~`UnifiedEntityStorage`~~
- ~~`SmartExcludeMatcher`~~
- ~~`CleanCacheRepository`~~
