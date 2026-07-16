Based on my thorough investigation, I can now provide you with comprehensive findings about the AI variant feature.

## 调研总结：AI 变体功能架构

### 1. **"AI 变体"是什么？用户怎么生成变体？**

**定义**：AI 变体是用户选择一张本体卡牌，输入风格预设（可选）和主题描述（可选），委托 AI 画师重新绘制的衍生作品。

**生成流程**：
- 前端：src/pages-sub/ai-variant/create.vue，Tab 分为「创作」和「历史」
  - 选卡牌（非法术牌）
  - 选 0～N 个风格预设（多选，每选一个多生一张）
  - 可选填主题描述（最多 500 字）
  - 选创作选项：角色优化（skipOriginal）、细节修正（postReviewEdit）
  - 支持单张委托（10 能量）或 5 倍生成（50 能量，M×5 倍）
  
- 云函数：uniCloud-alipay/cloudfunctions/variantGenerator/index.obj.js
  - submit()：单张提交入口
  - batchSubmit()：新增批量提交入口（支持 5 倍生成）
  - 流程：扣能量 → 写任务库（status=pending）→ 调 inkast 异步生图
  - inkastCallback()：图片完成后回调，上传 R2，更新任务状态

**关键参数**：
- source_card_key：本体卡牌
- user_prompt：完整提示词（「预设」+「；」+ user_text 拼接）
- user_text：用户输入的纯净主题（仅显示，预设不含）
- style_preset_id：风格预设稳定 id（筛选用）
- style_preset：风格预设 label 快照（显示兜底）

---

### 2. **"公开变体"动作的具体含义**

**核心模式**：
- 变体生成时默认 **私有**（is_public=false）
- 「公开」是 **独立操作**，点历史页面每张变体右下角的「眼睛」图标（src/pages-sub/ai-variant/create.vue:423-446）
- 公开不需要管理员审核，用户直接 clientDB 改写 is_public 字段

**公开时的冗余操作**：设公开前必须有昵称，否则弹「昵称设置」modal（NicknameSetupModal）
- 设公开时冗余快照：owner_nickname、owner_avatar（仿 forum 设计，方便展示且抗用户改名）
- 取消公开只翻 is_public=false

**方法对应**：
- 前端：togglePublic(task) → applyPublic(task, next)，clientDB 直改 ai-variant-tasks
- 改写字段：
  - is_public（bool）：公开标记
  - owner_nickname（string）：作者昵称快照（公开时写）
  - owner_avatar（string）：作者头像快照（公开时写）

**新变体默认公开偏好**：
- 存储位置：uni-id-users.userSettings.variantDefaultPublic（bool）
- 生成端在 inkastCallback() 阶段读此偏好，自动设 is_public + 冗余快照
- UI 入口："我的-设置"（未在本文件出现，应在 account/settings 页）

---

### 3. **变体数据存储**

**集合**：ai-variant-tasks（uniCloud-alipay/database/ai-variant-tasks.schema.json）

**关键字段**：

| 字段 | 类型 | 说明 |
|------|------|------|
| _id | string | 数据库主键 |
| task_id | string | 任务唯一 uuid，跨系统追踪 |
| owner_uid | string | 提交用户 ID |
| source_card_key | string | 本体卡牌 key |
| user_prompt | string | 完整提示词（仅 owner + admin 可读） |
| user_text | string | 用户主题（仅 owner + admin 可读） |
| style_preset_id | string | 风格预设稳定 id |
| style_preset | string | 风格预设 label 快照 |
| status | string | pending/generating/succeeded/failed/refunded |
| image_url | string | 成功后的 R2 CDN URL |
| cost_energy | int | 消耗能量（失败退款用） |
| is_public | bool | **公开展示标记**（默认 false） |
| owner_nickname | string | **作者昵称快照**（公开时冗余） |
| owner_avatar | string | **作者头像快照**（公开时冗余） |
| is_deleted | bool | 软删除标记（owner 可写） |
| deleted_at | timestamp | 删除时间 |
| created_at | timestamp | 创建时间 |
| post_review_edited | bool | 细节修正是否实际触发 |
| success_round | int | 产图轮次（0=直出/3=anchor） |

**权限规则**（schema permission）：
```
read: doc.owner_uid == auth.uid 
      || 'admin' in auth.role 
      || (doc.is_public == true && doc.status == 'succeeded')
update: doc.owner_uid == auth.uid || 'admin' in auth.role
```
- owner 可写：is_deleted / deleted_at / is_public / owner_nickname / owner_avatar（其余字段 write:false）
- 公开变体的 user_prompt / user_text 被字段级读权限锁定，非 owner 不可见（隐私）

**一个用户能公开多个变体**：✓ 是（per-task 独立标记）

**同一张卡能公开多个变体**：✓ 是（不同用户 / 同用户的不同变体都可独立公开）

---

### 4. **公开变体给谁看、在哪展示、有无审核**

**展示位置**：

1. **卡牌详情页「大家画的」模块**（src/components/PublicVariantsRow/index.vue）
   - 查询：is_public=true + status=succeeded + is_deleted!=true（按 created_at desc，limit 12）
   - 显示：展开态 3 列网格，收起态匀速跑马灯横滑
   - 作者头像昵称暂时注释隐去（预留恢复接口）
   - 点击进入 preview-detail，mode=public（公开浏览模式）

2. **首页轮播卡「创作变体」（src/components/HomeCards/VariantShowcaseCard.vue）
   - 查询：is_public=true + status=succeeded + is_deleted!=true（limit 30，shuffle 后去重取 10 张）
   - 显示：多本体优先（按 source_card_key 去重），相同本体的变体作补足
   - 作者头像昵称暂时隐去（与详情页一致）

3. **变体详情页（src/pages-sub/ai-variant/preview-detail/index.vue）**
   - 支持「自己」、「分享」、「preset 浏览」三种模式
   - **公开浏览模式**（mode='public'）：只展示风格标签，隐藏约稿提示词（非 owner 看不到）

**审核要求**：❌ 无
- 用户直接公开，不需管理员审核
- 能量（10/张）是唯一的滥用防控

---

### 5. **当前 git 工作区改动分析**

#### **src/pages-sub/ai-variant/create.vue**（未提交）

**关键改动**：

1. **底部提交按钮UI 重构**（line 467-504）
   - 旧：单个圆形按钮（消耗 N 能量委托 M 张）
   - 新：双按钮并排
     - 主按钮：×1 倍（10 能量 / 张）
     - 副按钮：×5 倍（紫色，50 能量 / 张）

2. **onSubmit 函数签名变更**（line 1164）
   ```javascript
   async function onSubmit(multiplier = 1)  // 新增参数
   ```
   - multiplier=1：普通生成
   - multiplier=5：5 倍生成

3. **新增 onSubmitBatch 函数**（line 1131）
   - ×5 生成的二次确认弹层
   - 显示总张数、总能量、当前余额
   - 确认后调 onSubmit(BATCH_MULTIPLIER)

4. **提交逻辑简化**（line 1164-1250）
   - 旧：逐张调 submit()，失败单张保留
   - 新：一次 batchSubmit()，云端并发处理 + 精确退款
   - 清除冗余 UI 文案（"再点一次重试"去掉）

#### **uniCloud-alipay/cloudfunctions/variantGenerator/index.obj.js**（未提交）

**关键改动**：

1. **新增配置常量**（line 22-23）
   ```javascript
   MAX_BATCH_SIZE: 50,        // 单次最多 50 张
   BATCH_CONCURRENCY: 20,     // 并发调 inkast 上限
   ```

2. **新增工具函数**
   - cleanPipelinePolicy()（line 161-176）：pipeline_policy 防御清洗（submit 与 batchSubmit 共用）
   - runWithConcurrency()（line 178-194）：有限并发执行器（结果不中断、按顺序返回）

3. **新增 batchSubmit() 方法**（line 569-679）
   - 入参：items[]（每项含 source_card_key / user_prompt / pipeline_policy 等）
   - 流程：逐项校验清洗 → 一次扣总能量 → 有限并发跑「写表+调 inkast」→ 精确退款
   - 返回：success_count / failed_count / new_balance

4. **submit() 优化**（line 442）
   - 调用 cleanPipelinePolicy() 函数（原逻辑内联，现提取复用）

**与「公开」「能量」的关联**：
- ❌ 改动中 **无关**「公开」逻辑（is_public 不动）
- ✓ 与能量相关：
  - 5 倍生成一次扣 N×5 倍能量
  - 失败精确退款（原来逐张退，现在按失败数累计退）
  - 保持 POST_REVIEW_EDIT_COST=5 追扣机制

---

### 关键代码位置总结

| 功能 | 文件 | 行号 |
|------|------|------|
| 变体生成 UI | src/pages-sub/ai-variant/create.vue | 44-226 |
| 公开/取消公开按钮 | src/pages-sub/ai-variant/create.vue | 438-447 |
| 公开逻辑实现 | src/pages-sub/ai-variant/create.vue | 1614-1652 |
| 单张提交 | uniCloud-alipay/cloudfunctions/variantGenerator/index.obj.js | 417-562 |
| 批量提交（新） | uniCloud-alipay/cloudfunctions/variantGenerator/index.obj.js | 569-679 |
| 回调处理 | uniCloud-alipay/cloudfunctions/variantGenerator/index.obj.js | 696-911 |
| Schema 定义 | uniCloud-alipay/database/ai-variant-tasks.schema.json | - |
| 卡牌详情公开池 | src/components/PublicVariantsRow/index.vue | 29-56 |
| 首页轮播展示 | src/components/HomeCards/VariantShowcaseCard.vue | 91-129 |
| 详情页多模式 | src/pages-sub/ai-variant/preview-detail/index.vue | 1-200+ |