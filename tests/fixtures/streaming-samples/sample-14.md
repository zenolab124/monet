## Tokio vs Goroutine 并发模型对比

<div style="display:flex;gap:12px;flex-wrap:wrap">
<div style="flex:1;min-width:280px;padding:16px;border:1px solid #e0e0e0;border-radius:8px;background:#faf5ef">

### Rust · Tokio

- **调度方式**：编译期生成状态机，work-stealing 线程池调度 Future
- **内存开销**：每个 task 初始几百字节，按需增长（零成本抽象）
- **错误处理**：`Result<T, E>` 强制处理，编译器不放过任何未处理的错误
- **并发原语**：`async/await` + `select!` + `mpsc`/`oneshot` channel
- **取消机制**：drop Future 即取消，但需手动保证清理逻辑（`CancellationToken`）

</div>
<div style="flex:1;min-width:280px;padding:16px;border:1px solid #e0e0e0;border-radius:8px;background:#eff4fa">

### Go · Goroutine

- **调度方式**：运行时 GMP 调度器，M:N 线程模型，抢占式调度（Go 1.14+）
- **内存开销**：每个 goroutine 初始 2-8 KB 栈，动态扩缩
- **错误处理**：返回 `error` 接口，靠约定而非编译器强制，容易被忽略
- **并发原语**：`go` 关键字 + `select` + channel（CSP 模型）
- **取消机制**：`context.Context` 传播取消信号，生态一致性好

</div>
</div>

---

### 核心差异深挖

<div style="display:flex;gap:12px;flex-wrap:wrap;margin-top:8px">
<div style="flex:1;min-width:280px;padding:12px;border:1px solid #d4c5a9;border-radius:6px;background:#fdf8f0">

**Tokio 调度细节**

Future 是惰性的——不 `.await` 就不执行。调度器通过 `Waker` 机制精确唤醒就绪的 task，<mark>没有运行时栈切换开销</mark>。但 `async fn` 跨 `.await` 持有的所有变量都会被捕获进状态机，大结构体会膨胀 Future 体积。

</div>
<div style="flex:1;min-width:280px;padding:12px;border:1px solid #a9c5d4;border-radius:6px;background:#f0f6fd">

**Goroutine 调度细节**

Goroutine 是立即执行的——`go f()` 即入队。调度器在函数调用点、channel 操作、系统调用处插入调度检查点，Go 1.14 起支持基于信号的异步抢占。<mark>栈是真实的、可增长的</mark>，所以阻塞调用（含 CGo）天然兼容。

</div>
</div>

---

### 代码示例

<details>
<summary>Rust Tokio — 并发请求示例</summary>

```rust
use tokio::task::JoinSet;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let urls = vec!["https://a.com", "https://b.com", "https://c.com"];
    let mut set = JoinSet::new();

    for url in urls {
        let url = url.to_string();
        set.spawn(async move {
            let resp = reqwest::get(&url).await?;
            Ok::<_, anyhow::Error>(resp.status())
        });
    }

    while let Some(result) = set.join_next().await {
        let status = result??; // 两层：JoinError + 业务 Error
        println!("status: {status}");
    }
    Ok(())
}
```

注意 `result??` 的双层解包——Tokio 把 task panic/取消错误和业务错误分开，编译器强制你两层都处理。

</details>

<details>
<summary>Go — 并发请求示例</summary>

```go
func main() {
    urls := []string{"https://a.com", "https://b.com", "https://c.com"}
    g, _ := errgroup.WithContext(context.Background())

    for _, url := range urls {
        g.Go(func() error {
            resp, err := http.Get(url)
            if err != nil {
                return err
            }
            defer resp.Body.Close()
            fmt.Println("status:", resp.StatusCode)
            return nil
        })
    }

    if err := g.Wait(); err != nil {
        log.Fatal(err)
    }
}
```

`errgroup` 是 Go 社区弥补原生 goroutine 缺乏结构化并发的标准方案。

</details>

---

### 关键结论

<mark>Tokio 在极致吞吐和内存效率上胜出，Go 在开发速度和心智负担上胜出。</mark>

两者不是"谁更好"，而是在不同约束下的最优解：Tokio 的编译期保证换来了陡峭学习曲线（生命周期、Pin、Send 约束）；Go 的简单性换来了运行时开销和错误处理的纪律依赖。

---

### 适用场景推荐

| 场景 | 推荐 | 理由 |
|------|------|------|
| 高并发网络代理/网关 | **Tokio** | 百万级连接下内存优势明显，零拷贝 + 状态机调度 |
| 微服务 / CRUD API | **Go** | 开发快、部署简单、团队上手成本低 |
| 嵌入式/资源受限环境 | **Tokio** | 无 GC、内存可控、可 `no_std` |
| CLI 工具 / DevOps 脚本 | **Go** | 单二进制交叉编译、标准库齐全 |
| 实时音视频/游戏服务器 | **Tokio** | 延迟可预测，无 GC 暂停 |
| 数据管道 / ETL | **Go** | channel + goroutine 天然管道模型，原型快 |
| 已有 C/C++ 互操作需求 | **Tokio** | FFI 零开销，Go 的 CGo 有调度和性能惩罚 |

完毕。