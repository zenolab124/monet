Node 事件循环底层是 libuv 提供的,核心是**六个阶段循环 + 阶段间清空微任务队列**。我按"骨架 → 细节 → 易错点 → 浏览器对比"四层讲。

---

**一、六个阶段(按顺序循环)**

```
┌─────────────────────────────┐
│ ① timers (setTimeout/Interval 到期回调)
│ ② pending callbacks (上轮某些系统操作回调,如 TCP errno)
│ ③ idle, prepare (内部使用,不接触)
│ ④ poll (拉 I/O 事件 + 执行 I/O 回调)        ← 主战场
│ ⑤ check (setImmediate 回调)
│ ⑥ close callbacks (socket.on('close') 等)
└─────────────────────────────┘
        循环回到 ①
```

**重点是 poll 阶段**:
- 计算 poll 超时时间
- 处理 I/O 事件队列里的回调
- 队列空时:
  - 如果有 `setImmediate` 注册 → 进入 check 阶段
  - 否则**阻塞等待**新 I/O 事件,直到最近的 timer 到期再走下一轮

---

**二、微任务队列在哪里清**

Node 有**两条微任务队列**:

1. **`process.nextTick` 队列**(Node 独有,优先级最高)
2. **Promise.then / queueMicrotask 队列**

**清空时机**:
- Node 11 之前:**每个阶段全部宏任务执行完后**才清一次微任务
- **Node 11+(对齐浏览器)**:**每执行完一个宏任务就清一次微任务**

每次清的顺序:**先把 `nextTick` 队列清空 → 再清 Promise 微任务队列**。如果在清微任务的过程中又往 `nextTick` 塞回调,继续清,直到都空。

> 这就是为什么 `process.nextTick` 滥用会"饿死"事件循环 —— 后续阶段一直进不去。

---

**三、几个高频考点**

**`setTimeout(fn, 0)` vs `setImmediate(fn)`**:

- 在**主模块顶层**:结果**不确定**。谁先取决于进入事件循环时 timer 计时是否已到 1ms(Node 最小延迟,0 被取整为 1)。
- 在 **I/O 回调内部**:`setImmediate` **一定先于** `setTimeout(fn, 0)`。因为 I/O 回调跑在 poll 阶段,下一个就是 check 阶段(setImmediate),要绕一整圈才回到 timers。

这是面试常被钓的题,答"不一定 + 看在哪儿调"才是稳妥答案。

**`process.nextTick` vs `Promise.then`**:

```js
setTimeout(() => console.log('timer'), 0)
Promise.resolve().then(() => console.log('promise'))
process.nextTick(() => console.log('nextTick'))
console.log('sync')

// sync → nextTick → promise → timer
```

同步代码 → 微任务(nextTick 优先于 Promise)→ 宏任务。

**I/O 回调里的顺序**:

```js
fs.readFile('a.txt', () => {
  setTimeout(() => console.log('timer'), 0)
  setImmediate(() => console.log('immediate'))
})
// 稳定输出:immediate → timer
```

---

**四、和浏览器事件循环的区别**

| 维度 | 浏览器 | Node |
|---|---|---|
| 宏任务分类 | 一条宏任务队列(粗略) | **六个阶段队列**,各管各的 |
| 微任务清空 | 每个宏任务后清 | Node 11+ 同浏览器;之前是阶段末清 |
| 特殊 API | `requestAnimationFrame` | `setImmediate` / `process.nextTick` |
| 渲染 | 微任务后可能触发渲染 | 无渲染概念 |

---

**五、易错点 / 我踩过的坑**

1. **`process.nextTick` 不是"立刻执行"**,是"当前操作完成后、下一个阶段之前"。但因为它优先级太高,**滥用会阻塞 I/O**,我见过有人用它写递归"分片",结果接口全卡死。
2. **大量同步 CPU 任务**(JSON.parse 大对象、加解密、压缩)会**阻塞整个事件循环**,Node 单线程一旦卡住所有请求一起卡。重活塞 `worker_threads` 或拆任务。
3. **微任务里疯狂塞微任务**会形成微任务循环,事件循环阶段一直推进不下去。
4. **timer 不准**:`setTimeout(fn, 100)` 不保证 100ms 后执行,只保证**不早于 100ms**;主线程忙,timer 就推迟。
5. **Promise 链里的同步代码**也在微任务阶段执行,如果链很长又全是同步,照样阻塞。

---

**六、一句话心法**

Node 事件循环的本质是:**libuv 的六阶段循环 + 阶段间清两条微任务队列 + 单线程跑 JS,I/O 走 libuv 线程池**。理解它的关键不是背阶段名,而是想清楚:

> 一个回调被注册时,它会进入**哪个队列**?事件循环跑到**哪个阶段**才会捞它?在它前面还有**哪些微任务**会插队?

把这三个问题答得出来,事件循环就算掌握了。

完毕。