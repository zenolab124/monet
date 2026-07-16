# Attention, One Equation at a Time

> SYNTHETIC SAMPLE — hand-authored for the streaming golden test. Exercises `$$` display math (including aligned and matrix forms) plus inline `$...$` math.

Scaled dot-product attention takes queries $Q$, keys $K$, and values $V$ and produces a weighted average of the values. The canonical definition is:

$$
\mathrm{Attention}(Q, K, V) = \mathrm{softmax}\!\left(\frac{Q K^\top}{\sqrt{d_k}}\right) V
$$

The scaling factor $\frac{1}{\sqrt{d_k}}$ keeps the dot products from growing with dimension $d_k$; without it, the softmax saturates and gradients vanish.

## Softmax, spelled out

For a score vector $s \in \mathbb{R}^n$, the softmax of its $i$-th entry is:

$$
\mathrm{softmax}(s)_i = \frac{e^{s_i}}{\sum_{j=1}^{n} e^{s_j}}
$$

Because the function is shift-invariant, we subtract $\max_j s_j$ before exponentiating for numerical stability. That is, $\mathrm{softmax}(s) = \mathrm{softmax}(s - c\mathbf{1})$ for any scalar $c$.

## Multi-head, as a block

With $h$ heads, each head projects into a $d_k = d_{\text{model}} / h$ subspace:

$$
\begin{aligned}
\mathrm{head}_i &= \mathrm{Attention}(Q W_i^Q,\, K W_i^K,\, V W_i^V) \\
\mathrm{MultiHead}(Q,K,V) &= \mathrm{Concat}(\mathrm{head}_1, \dots, \mathrm{head}_h)\, W^O
\end{aligned}
$$

A tiny numerical check helps intuition. Suppose $d_k = 4$ and a single query attends over two keys with raw scores:

$$
\frac{Q K^\top}{\sqrt{d_k}} = \begin{bmatrix} 2.0 & 0.0 \end{bmatrix}
\quad\Rightarrow\quad
\mathrm{softmax} = \begin{bmatrix} 0.88 & 0.12 \end{bmatrix}
$$

Notes worth remembering:

- The complexity is $O(n^2 d)$ in sequence length $n$ — this is the quadratic wall that motivates sparse and linear attention variants.
- Positional information is injected separately, since the operation above is permutation-equivariant.
- Masking sets disallowed positions to $-\infty$ so their softmax weight is exactly $0$.

```text
score  ->  scale by 1/sqrt(d_k)  ->  (mask)  ->  softmax  ->  weighted sum of V
```

The golden invariant: whether the renderer receives this whole document at once or streamed segment-by-segment, the block containing `\begin{aligned} ... \end{aligned}` must be emitted as one unit — a split inside it would produce mismatched HTML.
