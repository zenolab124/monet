# Gradient Descent, Derived Carefully

> SYNTHETIC SAMPLE — hand-authored for the streaming golden test. Exercises `$$` display math and inline `$...$` math because genuine math blocks were absent from the real session corpus.

Gradient descent minimizes a differentiable loss $L(\theta)$ by repeatedly stepping against the gradient. The core update, with learning rate $\eta > 0$, is:

$$
\theta_{t+1} = \theta_t - \eta \, \nabla_\theta L(\theta_t)
$$

Here $\nabla_\theta L$ is the vector of partial derivatives $\left[\frac{\partial L}{\partial \theta_1}, \dots, \frac{\partial L}{\partial \theta_n}\right]^\top$. For a single training example $(x_i, y_i)$ and the squared-error loss, we have $L_i = \tfrac{1}{2}(f_\theta(x_i) - y_i)^2$, so the per-example gradient is simply $(f_\theta(x_i) - y_i)\,\nabla_\theta f_\theta(x_i)$.

## Why the step size matters

If $\eta$ is too large the iterate overshoots; if too small, convergence crawls. For a quadratic bowl $L(\theta) = \tfrac{1}{2}\theta^\top A \theta$ with symmetric positive-definite $A$, convergence requires:

$$
0 < \eta < \frac{2}{\lambda_{\max}(A)}
$$

where $\lambda_{\max}(A)$ is the largest eigenvalue. The asymptotic rate is governed by the condition number $\kappa = \lambda_{\max} / \lambda_{\min}$.

## The full-batch objective

Averaging over $N$ examples gives the empirical risk:

$$
L(\theta) = \frac{1}{N} \sum_{i=1}^{N} \ell\big(f_\theta(x_i),\, y_i\big) + \frac{\lambda}{2}\lVert \theta \rVert_2^2
$$

The second term is $L_2$ regularization with strength $\lambda$; its gradient contributes $\lambda\theta$ to every step, pulling weights toward the origin.

Key practical notes:

1. **Normalize inputs** so that features share a scale — otherwise $A$ is ill-conditioned and $\kappa \gg 1$.
2. **Warm up** the learning rate over the first few hundred steps.
3. Prefer momentum: $v_{t+1} = \mu v_t - \eta \nabla L$, then $\theta_{t+1} = \theta_t + v_{t+1}$, with $\mu \approx 0.9$.

A minimal reference implementation:

```python
def sgd_step(theta, grad, lr=1e-2, weight_decay=0.0):
    # theta and grad are numpy arrays of identical shape
    return theta - lr * (grad + weight_decay * theta)
```

Inline math and display math should both survive segmentation: a split that lands between the `$$` fences must never leave one fence orphaned, or the rendered HTML for the segment will diverge from the whole-text render. That invariant is exactly what this fixture guards.
