完美。现在让我创建一份综合总结。我已经有了足够的信息，让我组织一个完整的报告。

## 首页（Home View）数据加载逻辑分析

我已经完整地探索了首页的数据加载逻辑。以下是详细总结：

---

### 1. 首页视图组件位置

**文件路径**: `/Users/dev/monet/src/views/HomeView.vue`

这是核心首页视图组件，使用 Vue 3 `<script setup>` 语法。首页是一个栅格系统（Grid），包含多个预内置卡片和自定义 Widget。

---

### 2. 首页加载的数据类型

首页加载了以下主要数据：

#### A. **使用统计数据 (UsageStats)**
- **来源 Composable**: `useHomeStats.ts`
- **数据结构**:
  ```typescript
  interface UsageStats {
    daily: DailyUsage[]      // 每日统计
    month: {
      total: number
      byModel: ModelUsage[]  // 按模型分组统计
    }
  }
  
  interface DailyUsage {
    date: string    // "YYYY-MM-DD"
    total: number   // Token 总数
  }
  ```
- **使用场景**: 
  - `TodaySummaryCard` - 展示今日 Token 使用量
  - `TokenCard` - Token 消耗卡片
  - `StreakCard` - 连续活跃统计
  - `HeatmapCard` - 16周热力图展示

#### B. **项目及会话数据 (Project[])**
- **来源 Composable**: `useProjects.ts`
- **数据结构**:
  ```typescript
  interface Project {
    id: string
    display_path: string
    sessions: SessionSummary[]
    session_count: number
    last_active: number | null
  }
  
  interface SessionSummary {
    id: string
    title: string | null
    first_user_message: string | null
    model: string | null
    git_branch: string | null
    cwd: string | null
    last_modified: number  // 时间戳
    total_tokens: TokenUsage
    message_count: number
    file_size: number
  }
  ```
- **使用场景**:
  - `TodaySummaryCard` - 今日会话数、使用模型
  - `RecentSessionsCard` - 最近8个会话列表
  - `ModelPreferenceCard` - 模型偏好统计
  - `ProjectActivityCard` - 项目活跃度
  - `BranchActivityCard` - Git 分支活动
  - `WorkRhythmCard` - 工作节奏分析
  - `SessionDepthCard` - 会话深度分布
  - `CostEstimateCard` - 成本估算

#### C. **自定义 Widget 列表**
- **局部状态**: `customWidgets` ref（Map 结构）

#### D. **配置信息**
- `dayStartHour` - 一天的开始时间（用于计算"今日"边界）

---

### 3. 数据获取方式

#### **获取流程**（Tauri invoke 调用）

**第一步：入口 - activeSection 监听**
```typescript
watch(
  activeSection,
  (section) => {
    if (section === 'home') {
      ensureLoaded()      // 触发统计数据加载
      loadProjects()      // 触发项目数据加载
      loadCustomWidgets() // 加载自定义 Widget
    }
  },
  { immediate: true }, // 首次挂载立即执行
)
```

**第二步：并发加载两个 Tauri 命令**
1. `invoke<UsageStats>('get_usage_stats')` - 获取 Token 使用统计
2. `invoke<Project[]>('get_projects')` - 获取所有项目和会话

这两个命令在 `ensureLoaded()` 时并发执行，互不影响：
```typescript
// useHomeStats.ts
function ensureLoaded() {
  if (usageLoading.value || diagLoading.value) return
  loadUsage()     // 并发
  loadDiag()      // 并发
}
```

**第三步：数据加载后的二阶段处理**
```typescript
watch(
  [usage, projects],
  async ([u, p]) => {
    if (!u || !p.length) return
    
    // 1. 获取 dayStartHour 配置
    const cfg = await invoke<{ dayStartHour: number }>('get_widget_config')
    
    // 2. 计算今日的会话数和 Token 数
    let todayTokens = ...
    let sessions = ...
    let models = ...
    
    // 3. 更新 Widget 状态用于快速提示
    invoke('update_widget', { 
      todaySessions: sessions, 
      todayTokens: todayTokens, 
      models 
    })
  },
)
```

**第四步：自定义 Widget 加载**
```typescript
async function loadCustomWidgets() {
  const list = await invoke<DashboardWidget[]>('list_dashboard_widgets')
  const map = new Map<string, DashboardWidget>()
  for (const w of list) map.set(w.id, w)
  customWidgets.value = map
}
```

---

### 4. 缓存机制详解

#### **A. 内存缓存（Session 生命周期）**
所有数据都在内存 ref 中缓存，只在以下情况重新加载：

1. **首次进入首页** → 显示 loading，加载数据
2. **重新进入首页** → 静默后台刷新（不显示 loading）

```typescript
// useHomeStats.ts 中的关键逻辑
async function loadUsage() {
  const hasCached = usage.value !== null  // ← 检查内存缓存
  if (!hasCached) usageLoading.value = true
  usageError.value = null
  try {
    usage.value = await invoke<UsageStats>('get_usage_stats')
  } finally {
    usageLoading.value = false
  }
}
```

#### **B. 项目数据文件变更监听**
```typescript
// useProjects.ts
listen('projects-changed', () => {
  reloadProjectsSilently()  // 后端文件变更 → 静默重新加载
})
```

#### **C. Vue Computed 缓存**
所有卡片中的衍生数据都通过 Vue 的 `computed` 自动缓存，只在依赖数据变化时重新计算：

- `TodaySummaryCard.vue`: 
  - `todayTokens` - 从 `usage.daily` 中筛选今日
  - `todaySessions` - 从 `projects.sessions` 中筛选今日
  - `todayModels` - 从今日会话提取模型列表

- `RecentSessionsCard.vue`:
  - `recent` - 展平所有项目会话 → 排序 → 取前8条

- `HeatmapCard.vue`:
  - `rows` - 构建 16周x7天的热力格子，含分位计算

#### **D. localStorage 持久化（UI 状态）**
```typescript
// useUiState.ts
const STORAGE_KEY = 'monet-ui'
watch([sidebarsCollapsed, activeSection, ...], ([collapsed, section, ...]) => {
  localStorage.setItem(STORAGE_KEY, JSON.stringify({...}))
})
```
持久化的是 **UI 布局状态**（侧栏宽度、当前区域等），**不缓存业务数据**。

#### **E. 一阶段 vs 二阶段加载**

| 阶段 | 操作 | 何时执行 |
|------|------|---------|
| 一阶段 | 并发: `get_usage_stats` + `get_projects` | 进入首页时 |
| 二阶段 | 顺序: `get_widget_config` 然后 `update_widget` | 两个主数据都到达后 |

---

### 5. 文件位置速查表

| 功能 | 文件路径 |
|------|---------|
| **首页视图** | `/src/views/HomeView.vue` |
| **统计数据管理** | `/src/composables/useHomeStats.ts` |
| **项目数据管理** | `/src/composables/useProjects.ts` |
| **UI 状态管理** | `/src/composables/useUiState.ts` |
| **会话过滤排序** | `/src/composables/useSessions.ts` |
| **数据类型定义** | `/src/types/index.ts` |
| **卡片组件** | `/src/components/home/*.vue` |
|   - 今日摘要 | `TodaySummaryCard.vue` |
|   - 最近会话 | `RecentSessionsCard.vue` |
|   - 热力图 | `HeatmapCard.vue` |
|   - 模型偏好 | `ModelPreferenceCard.vue` |
|   - 项目活跃 | `ProjectActivityCard.vue` |
|   - 分支活动 | `BranchActivityCard.vue` |
|   - 工作节奏 | `WorkRhythmCard.vue` |
|   - 会话深度 | `SessionDepthCard.vue` |
|   - Token/成本 | `TokenCard.vue`, `CostEstimateCard.vue` |
|   - 连续统计 | `StreakCard.vue` |
|   - 栅格布局 | `HomeGrid.vue` |

---

### 关键设计特点

1. **双 Composable 模式**: 统计数据和项目数据分离管理，各自独立缓存
2. **惰性加载**: 首页首次进入才触发数据加载（`{ immediate: true }` watch）
3. **静默刷新**: 再次进入首页不显示 loading，在后台更新数据
4. **文件监听**: 后端文件变更事件驱动项目数据同步（无长轮询）
5. **二阶段处理**: 等待两个主数据都到达后，进行配置查询和 Widget 状态更新
6. **零业务数据持久化**: localStorage 仅用于 UI 布局状态，业务数据完全内存缓存