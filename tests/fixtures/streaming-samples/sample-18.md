完整 CLI 能力盘清了。

## 一张图看全能力

| 类别 | 命令 | 做什么 | 对项目价值 |
|---|---|---|---|
| **uniCloud 部署** | `cloud functions --upload cloudfunction/common/db/vf/action/all --name X [--force]` | 上传云函数 / 公共模块 / schema / 校验函数 / action | ⭐⭐⭐⭐⭐ |
| **uniCloud 反向同步** | `cloud functions --download ...` | 从云端拉回本地 | ⭐⭐ |
| **uniCloud 初始化** | `cloud functions --initdatabase` | db-init 跑 db_init.json | ⭐⭐⭐ |
| **uniCloud 切空间** | `cloud functions --assignspace N` | 切换已绑定的云空间 | ⭐⭐（你只有一个空间）|
| **uniCloud 列资源** | `cloud functions --list cloudfunction/db/space [--cloud]` | 看本地/云端有哪些云函数、schema | ⭐⭐⭐⭐（AI 排查时） |
| **小程序日志** | `logcat mp-weixin --project snap-ub --mode full` | 微信小程序运行时 console + 报错 | ⭐⭐⭐⭐⭐ |
| **支付宝小程序日志** | `logcat mp-alipay --project snap-ub --mode full` | 你也发布支付宝小程序，能用 | ⭐⭐⭐⭐ |
| **H5 日志** | `logcat web --browser Chrome --mode full` | H5 浏览器日志 | ⭐⭐⭐ |
| **uniCloud 日志** | `logcat unicloud --project snap-ub` | **云函数运行时报错和 console** | ⭐⭐⭐⭐⭐ |
| **App 打包日志** | `logcat pack` | 云打包过程日志 | ⭐（你不打 App）|

## "AI 读报错"这件事 —— 能做到，但有 3 个 unknown

文档明确支持，但有几个细节没说清，会决定 AI 怎么用：

**已知**

- `--mode` 三档：`prevBuild`（默认，上次构建日志）/ `lastBuild`（最新构建）/ **`full`（完整日志，AI 调试要用这个）**
- 输出是 HBuilderX 控制台日志的镜像，AI 通过 Bash 就能读到
- HBuilderX 必须在跑（CLI 连进程）
- 平台必须有"运行"动作正在进行（微信开发者工具开着、Chrome 开着 H5、云函数有调用）

**Unknown（决定 AI 用法的关键）**

1. **logcat 是阻塞流式（持续输出新日志，要 Ctrl+C 停止）还是一次性 dump（执行完返回）？**
   - 流式 → AI 必须后台跑（`run_in_background`）+ Monitor 监听新行
   - 一次性 → AI 直接 Bash 跑能拿全部输出
2. **报错内容粒度** —— 只有 console.log，还是包含 JS error stack trace、warning、网络请求失败？
3. **uniCloud logcat 是看云端真实运行日志，还是只是本地调试 console？** —— 如果是云端真实日志，价值极高（生产报错 AI 也能看）

**两件事都能 5 分钟实测验证**（无副作用）：跑两条命令看输出形态就知道了。

## "AI 读报错"的实际改进图景

**改造前**（现状）
```
你: 这个云函数 variantGenerator 报错了
AI: 让我看下代码... [猜测]
你: 报错是 xxx [手动复制]
AI: 哦那应该是...
```

**改造后**
```
你: variantGenerator 又抛错了
AI: [自己跑 cli logcat unicloud --project snap-ub]
    [读到完整 stack trace]
    在 _timing.js:42 抛 TypeError... 已修复
```

这是从"AI 蒙着调试"到"AI 自主诊断"的根本跃迁。**这一项的价值超过 uniCloud 部署 CLI 本身。**

## 下一步建议

要不要我现在用安全模式实测下面两条，把 3 个 unknown 一次解决？（都是只读，无副作用）

1. `cli logcat unicloud --project snap-ub`（带 5 秒 timeout，看是流式还是一次性）
2. `cli logcat mp-weixin --project snap-ub --mode full`（同上）

实测完你就知道这个能力的真实形态，再决定要不要工程化。

完毕。