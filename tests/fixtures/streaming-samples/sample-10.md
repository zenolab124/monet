完美。现在让我总结所有的关键发现：

## 最终总结

我已系统性、彻底地搜索和分析了 SNAP-UB 项目的能量虚拟货币系统。以下是完整的事实总结：

---

### **1. 能量（energy）数据存储**

**用户余额字段和集合：**
- **余额字段**: `balance` (int)
- **集合**: `user_energy` 
- **文件位置**: `/Users/zz/workspace/unibest/snap-ub/uniCloud-alipay/database/user_energy.schema.json:20`
- **相关字段**:
  - `balance`: 当前余额（权限：仅 admin 可写）
  - `total_earned`: 累计获得（权限：仅 admin 可写）
  - `total_spent`: 累计消耗（权限：仅 admin 可写）
  - `last_seen_grant_at`: 用户上次查看 admin_grant 奖励弹窗时间（作为水位线，用户可写）

---

### **2. 现有能量获取渠道及奖励额度**

默认任务配置在：`/Users/zz/workspace/unibest/snap-ub/uniCloud-alipay/cloudfunctions/taskCenter/index.obj.js:68-129`

| 任务 key | 任务名称 | 奖励(基础) | 限制类型 | 实现位置 |
|---------|--------|---------|--------|--------|
| `daily_login` | 每日签到 | 1～10(递进式) | daily, 1次 | taskCenter:501-514 (递进规则) |
| `watch_ad_daily` | 看广告得能量 | 66 | unlimited | taskCenter:82-92 |
| `rating_daily` | 每日评分 | 1 | daily, 20次 | taskCenter:94-104 |
| `comment_daily` | 每日评论 | 5 | daily, 10次 | taskCenter:106-116 |
| `painter_commission_bonus` | 画师作画礼 | 200 | once | taskCenter:118-128 |
| `ai_variant_generate` | AI生图消费 | -10(扣费) | 无限 | variantGenerator:20, 463-471 |

**所有任务通过一个云对象实现：**
- `uniCloud-alipay/cloudfunctions/taskCenter/index.obj.js` — `completeTask(taskKey, params)` 方法（第 380 行）

---

### **3. 能量发放机制（获取 vs 消费）**

**获取（earn）:**
- **云函数发放** (`adminGrantEnergy`): `/Users/zz/workspace/unibest/snap-ub/uniCloud-alipay/cloudfunctions/adminGrantEnergy/index.js`
  - 三种模式: `list`(前端调度≤500人/批) / `all`(云端分页) / 遗留 `grants` 数组
  - 写入: `energy_records`(type='earn', source='admin_grant', batch_id) + `user_energy.balance++`
  - 幂等键: `(user_id, batch_id)`

- **任务完成发放** (taskCenter.completeTask):
  - 写入三张表（无事务）:
    1. `user_energy.update({balance, total_earned})`
    2. `energy_records.add({type:'earn', source:'task_xxx'})`
    3. `task_completions.add({task_key, reward_received})`

- **消费（spend）:**
  - taskCenter.spendEnergy() 方法（第 781 行）— 通用消费接口
  - variantGenerator.spendEnergyLocal() (第 230 行) — 本地扣费（生图时）
  - variantGenerator.chargeEnergyForce() (第 322 行) — 强制扣费（post_review_edit 补扣 5）
  - 所有消费写入 `energy_records(type='spend')`

**统一发放工具：**
- 无统一单函数，但有通用模式: 写 `energy_records` → 更新 `user_energy` → 更新关联业务表（如 task_completions）
- 文件: `/Users/zz/workspace/unibest/snap-ub/uniCloud-alipay/cloudfunctions/variantGenerator/index.obj.js:230-282` (spendEnergyLocal)

---

### **4. energy_records 集合的结构和用途**

**文件**: `/Users/zz/workspace/unibest/snap-ub/uniCloud-alipay/database/energy_records.schema.json`

**核心字段**:
- `user_id`: 用户ID
- `amount`: 能量数量（正数=获得，负数=消耗）
- `type`: enum['earn', 'spend'] — 类型
- `source`: 来源标识 (task_comment, task_post, task_daily_login, admin_grant, ai_variant_refund 等) — 第 30 行注释
- `task_id`: 关联任务ID（如果有）
- `balance_after`: 变动后余额
- `batch_id`: 批量发放时的批次号，用作幂等键，(user_id, batch_id) 唯一 — 第 46 行
- `created_at`: 变动时间

**用途**: 完整的能量流水账本，支持：
- 用户能量记录查询（`fetchEnergyRecords`）
- 管理员赠送未读检测：`source='admin_grant' AND created_at > user_energy.last_seen_grant_at` — taskCenter.ts:92-114
- 审计 & 数据恢复

---

### **5. 每日限额 / 重置 / 防重复领取的现有实现**

**时间范围计算（采用本地时区）：**
```javascript
const now = new Date()
const todayStart = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime()
// — taskCenter:33, 118, 278-279, 418
```
- 跨度：从当日 00:00:00（本地时区）到次日 00:00:00

**每日重置存储：**
- `task_completions.completion_date` >= `todayStart` 视为"今天"
- 检查同 (user_id, task_key, completion_date >= todayStart) 的完成记录次数

**防重复核心逻辑（daily_login 示例）：**

```javascript
// taskCenter:302-312 — daily 任务重复检查
const completions = await db
  .collection('task_completions')
  .where({
    user_id: uid,
    task_key: taskKey,
    completion_date: dbCmd.gte(todayStart),  // 只查今日
  })
  .get()
currentCount = completions.reduce((sum, item) => sum + item.completion_count, 0)
canComplete = currentCount < taskConfig.limit_count  // limit_count=1 时只能领1次
```

**签到防重复的连续天数计算**（递进奖励）:
- calculateConsecutiveDays(userId) — taskCenter:14-62
- 倒序查 task_completions，检查连续性（每天向前退 24h 查一次）
- 规则: 连续签到 1-10 天返回 `[1,1,3,3,5,5,7,7,9,9]` 对应元素；第 11 天后固定 10 — taskCenter:506-511

**时区处理：**
- ⚠️ 目前**全部采用本地/服务器时区**，无 UTC 转换
- completion_date 存的是 `new Date(...).getTime()`（本地时间戳）
- 如果云函数部署在非北京时区，会出现"用户在北京时间 23:59 秒杀，第二天却能重复领取"的抖动

---

### **6. task_completions 集合的用途**

**文件**: `/Users/zz/workspace/unibest/snap-ub/uniCloud-alipay/database/task_completions.schema.json`

**核心字段**:
- `user_id`, `task_key`: 复合唯一性（无强制 unique 约束，但业务层保证）
- `completion_date`: 完成日期（用于 daily/weekly/monthly 统计）— 第 24-26 行
- `completion_count`: 完成次数（通常为 1）
- `reward_received`: 本次获得的奖励能量

**用途**:
- 统一的**通用任务完成记录表**，适用于所有新任务类型
- 支持 daily / weekly / monthly / once 等多种周期判定
- 完成记录永久保留，不删除
- **完全可复用**：新增任务 key 只需在 `task_configs` 添加配置，云函数代码 0 改动

---

### **关键代码位置总览**

| 需求 | 文件位置 |
|------|---------|
| 能量数据模型 | `/uniCloud-alipay/database/user_energy.schema.json:10-64` |
| 能量流水记录 | `/uniCloud-alipay/database/energy_records.schema.json` |
| 完成记录表 | `/uniCloud-alipay/database/task_completions.schema.json` |
| 任务配置表 | `/uniCloud-alipay/database/task_configs.schema.json` |
| 云对象实现 | `/uniCloud-alipay/cloudfunctions/taskCenter/index.obj.js:220-993` |
| 管理员发放 | `/uniCloud-alipay/cloudfunctions/adminGrantEnergy/index.js:43-146`(list 模式) |
| 前端任务中心 | `/src/pages-sub/task-center/task-center.vue` |
| 前端 Store | `/src/store/taskCenter.ts` |
| 前端 Hook | `/src/hooks/useEnergy.ts` |
| AI生图扣费 | `/uniCloud-alipay/cloudfunctions/variantGenerator/index.obj.js:230-282` |
| 文档(完整设计) | `/docs/knowledge/domains/task-energy.md` |
| 文档(流程) | `/docs/knowledge/workflows/task-daily-checkin.md` |
| 文档(admin操作) | `/docs/knowledge/workflows/admin-grant-energy.md` |

---

### **为"用户公开AI变体后给能量奖励"的设计建议**

基于现有架构，推荐：

1. **新增 task_key**: 如 `ai_variant_public_bonus`（一次性或有周期限制）
2. **数据触发点**: variantGenerator 中 `is_public=true` 转移时，调 taskCenter.completeTask()
3. **记录关联**: energy_records.task_id 指向 ai-variant-tasks._id，完整追踪
4. **幂等保护**: 由 (user_id, ai_variant_task_id) 确保不重复发放（或在 energy_records 检查）
5. **不用新集合**: task_completions 已足够通用