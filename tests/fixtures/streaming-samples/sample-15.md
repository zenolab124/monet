## Tokio vs Goroutine 并发模型对比

<div style="display:flex;gap:12px;flex-wrap:wrap">
<div style="flex:1;min-width:280px;padding:16px;border:1px solid #e0e0e0;border-radius:8px">

### Tokio (Rust)

- **调度方式**：协作式（cooperative）——任务必须主动 `.await` 让出控制权，不 yield 就独占线程
- **内存开销**：每个 task 约 **几百字节**（无栈协程，状态机编译生成）
- **错误处理**：`Result<T, E>` 强制在编译期处理，panic 默认不跨 task 传播，需 `JoinHandle` 显式捕获
- **生命周期**：借用检查器约束跨 await 引用，`'static + Send` 是 spawn 的硬门槛

</div>
<div style="flex:1;min-width:280px;padding:16px;border:1px solid #e0e0e0;border-radius:8px">

### Goroutine (Go)

- **调度方式**：抢占式（自 Go 1.14 起基于信号的异步抢占）——运行时可在任意安全点切走 goroutine
- **内存开销**：初始栈 **2KB**，按需动态增长（典型运行态几 KB～数十 KB）
- **错误处理**：无强制机制，靠 `error` 返回值 + `recover` 捕获 panic，遗漏是常见 bug 源
- **生命周期**：GC 管理，无所有权约束，goroutine 泄漏是隐性风险

</div>
</div>

---

### 代码示例

<details>
<summary>Tokio — 并发请求</summary>

```rust
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let urls = vec!["https://a.com", "https://b.com"];
    let mut handles = Vec::new();

    for url in urls {
        let url = url.to_string();
        handles.push(tokio::spawn(async move {
            reqwest::get(&url).await?.text().await
        }));
    }

    for h in handles {
        match h.await? {
            Ok(body) => println!("{} bytes", body.len()),
            Err(e) => eprintln!("request failed: {e}"),
        }
    }
    Ok(())
}
```

注意 `url.to_string()` —— 闭包必须拥有数据所有权（`'static + Send`），不能借用栈上引用。

</details>

<details>
<summary>Go — 并发请求</summary>

```go
func main() {
    urls := []string{"https://a.com", "https://b.com"}
    ch := make(chan string, len(urls))

    for _, url := range urls {
        go func(u string) {
            resp, err := http.Get(u)
            if err != nil {
                ch <- fmt.Sprintf("error: %v", err)
                return
            }
            defer resp.Body.Close()
            body, _ := io.ReadAll(resp.Body)
            ch <- fmt.Sprintf("%d bytes", len(body))
        }(url)
    }

    for range urls {
        fmt.Println(<-ch)
    }
}
```

简洁直接，但 `body, _ := ...` 这种忽略 error 的写法编译器不会拦你。

</details>

---

### 关键结论

<mark>Tokio 的核心优势是零成本抽象——编译期状态机 + 借用检查把并发 bug 挡在运行前，代价是学习曲线陡峭。</mark>

<mark>Goroutine 的核心优势是心智负担低——写并发像写顺序代码，代价是 goroutine 泄漏和未处理 error 只能靠纪律和工具兜底。</mark>

---

### 适用场景推荐

| 场景 | 推荐 | 理由 |
|---|---|---|
| 高并发网络代理 / 网关 | **Tokio** | 每连接开销极低，百万连接级别内存可控 |
| 微服务 CRUD / API 后端 | **Go** | 开发速度快，goroutine 开销对中等并发绰绰有余 |
| 嵌入式 / 资源受限环境 | **Tokio** | 无 GC、无运行时栈增长，内存占用可预测 |
| 快速原型 / DevOps 工具 | **Go** | 编译快、部署单二进制、上手门槛低 |
| 延迟敏感的实时系统 | **Tokio** | 无 GC 停顿，尾延迟可控 |
| 团队 Rust 经验不足 | **Go** | 并发模型学习成本低一个数量级，招人也更容易 |

一句话：<mark>性能天花板和正确性保证选 Tokio，开发效率和团队可达性选 Go。</mark>

完毕。