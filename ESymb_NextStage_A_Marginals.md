# ESymb 下一阶段（A）：前缀/后缀边缘化（marginals）
**非重复跨圈 observable 的低秩扫描（rank ≤ 2 可证）**

> **定位**：A 线解决“不要只盯重复 word family”，改为构造 **非重复（non-repeating）** 的跨圈可观测量，并复用既有的 *normalize-aware* rank screening + exact solve 工作流，去抓 **rank ≤ 2** 的可证结构。

---

## 1. 背景与理论动机

### 1.1 为什么需要 A（在 loops=1..6 的信息瓶颈下）
- 跨圈序列只有 6 个点：Hankel 方阵最多到 `3×3`，因此仅能对 **常系数递推阶 ≤ 2** 提供可信的“plateau 证书”。
- 这导致：许多真实复杂的对象会落在 `INCONCLUSIVE`（并不代表“无结构”，而是“点太少无法证实”）。
- A 的策略是：选取**更“平均化/投影化”**的 observable，让真实结构更可能在短序列上表现为 rank 1 或 2。

### 1.2 核心直觉：边缘化会把“字级细节”压缩成低维动力学
把每圈的系数函数视为加权语言：
- 字母表 Σ（例如 9 个字母）
- 第 L 圈（权重 2L）给出 `f_L(w)`：每个长度 `2L` 的 word 赋一个有理数系数

边缘化（对满足某个前/后缀条件的 word 求和）本质上是对 `f_L` 施加线性泛函。  
在很多“近似线性系统/加权自动机”模型里，这会将复杂结构投影到由少数谱模态控制的量上，因此更容易出现：
- 归一化后为几何级数（rank 1）
- 或满足二阶常系数递推（rank 2）

> 这也是为什么 A 必须和 normalize candidates 配套：很多“看似变系数”的增长（阶乘/双阶乘/二项式）剥掉之后，剩余部分更接近常系数递推。

---

## 2. Observable 定义（非重复）

设第 L 圈（权重=2L）的系数函数为 `f_L(w)`，w 为长度 `2L` 的 word。

### A1) 固定后缀 s（长度 k）
选择 `|s|=k`（建议 k=1 或 2）：
\[
c_L^{(s)} := \sum_{w\in\Sigma^{2L},\; w\text{ ends with } s} f_L(w).
\]

### A2) 固定前缀 p（长度 r）
选择 `|p|=r`（建议 r=1 或 2）：
\[
c_L^{(p)} := \sum_{w\in\Sigma^{2L},\; w\text{ starts with } p} f_L(w).
\]

### A3) 固定前缀 u 与后缀 v（长度 r,k）的“二维边缘化矩阵”
对每个 L 定义小矩阵：
\[
M_L(u,v) := \sum_{m\in\Sigma^{2L-r-k}} f_L(u\,m\,v), \qquad |u|=r,\ |v|=k.
\]
- 固定 (u,v)，得到跨圈标量序列 `c_L^{(u,v)} := M_L(u,v)`。
- 对固定 L，也可以把所有 (u,v) 组成矩阵 `M_L`，其矩阵秩 `rank(M_L)` 可作为“结构复杂度”的直观指标。

---

## 3. 算法（不写代码的“实现级”步骤）

### 3.1 数据流（streaming）构桶：单圈一次扫完即可
对每个 `Esymb_L{L}.jsonl`（L=1..6）流式读取 term：
- term 给出 `word`（长度 2L 的字母序列）与 `coeff`（有理数）
- 提取 key：
  - prefix key = `word[0..r)`
  - suffix key = `word[2L-k..2L)`
- 进行累加：
  - `bucket_suffix[s] += coeff`
  - `bucket_prefix[p] += coeff`
  - `bucket_uv[(u,v)] += coeff`

**规模可控**：
- k=1：≤|Σ| 个桶
- k=2：≤|Σ|² 个桶
- (r,k)=(2,2)：≤|Σ|⁴ 个桶（9 字母时 6561），每桶只存 6 个点（L=1..6）

### 3.2 归一化与 rank screening（SWSB：screen-with-solve-budget）
对每个桶得到的跨圈序列 `c_L`，在候选归一化集合 `G` 上尝试：
- `none`
- `odd double factorial`：\(g(L)=(2L-3)!!\)
- `even double factorial`：\(g(L)=(2L-2)!!\)
- （可选）含二项式/阶乘的模板（例如来自中心二项式系数的组合增长）

对每个归一化候选：
1) 形成 `d_L = c_L / g(L)`  
2) 在多个素数模 p 下计算 Hankel rank curve `r(N), N=0..2`
3) 聚合策略（保守）：对每个 N 取各 prime 的 **最大** rank（避免“模 p 偶然降秩”）
4) 判定：
   - `TRIVIAL`：全 0
   - `PASS`：出现 plateau 且 rank ≤ 2（在 N≤2 内可证）
   - `FAIL/INCONCLUSIVE`：否则只记录

`PASS` 的桶进入 exact solve（有理数域）阶段：恢复最小阶 ≤2 的常系数递推，并可映射回原未归一化序列。

---

## 4. 输出与解释（你能从 A 得到什么）

### 4.1 典型产出类型
- **He/Cai 型**：归一化后 rank 1（几何级数），映射回原序列是形如
  \[
  c_L = \lambda(L)\,c_{L-1}
  \]
  其中 \(\lambda(L)\) 来自你用的模板（如 \((2L-3)\) 或双阶乘比值）。
- **二阶常系数类**：归一化后 rank 2，得到
  \[
  d_{L+2} = a\,d_{L+1} + b\,d_L.
  \]
- **桶间对称性线索**（但依赖关系主要在 B）：一些桶的序列完全相同/成比例，提示字母重标记或投影对称。

### 4.2 “二维矩阵”视角额外有价值
对每个 L 的 `M_L`：
- `rank(M_L)` 低 ⇒ prefix 与 suffix 的耦合通过少数潜在状态传递（很像有限维线性系统的输入/输出维数）。
- `rank(M_L)` 随 L 稳定或缓慢增长 ⇒ 强结构信号。

---

## 5. 测试计划（A 线必须做的正确性与稳健性检查）

### 5.1 代数恒等式（强一致性检查）
对任意固定 L：
- **后缀分解守恒**：
  \[
  \sum_{s\in\Sigma^k} c_L^{(s)} = \sum_{w\in\Sigma^{2L}} f_L(w) \;(:= \text{Total}_L).
  \]
- **前缀分解守恒**：
  \[
  \sum_{p\in\Sigma^r} c_L^{(p)} = \text{Total}_L.
  \]
- **二维边缘化与一维边缘化一致**：
  \[
  \sum_v M_L(u,v) = c_L^{(u)} ,\quad \sum_u M_L(u,v) = c_L^{(v)}.
  \]
这些都是“桶划分”的定义后果，失败基本意味着 key 切片/长度/字母编码有 bug。

### 5.2 数值/模 p 稳健性
- 多 prime 下 rank curve 应基本一致；若某 prime 特别小导致异常降秩，保守聚合（max）应避免误判 PASS。
- `PASS` 桶的 exact solve 结果应能在有理数域完全重现 6 点（并可 predict 第 7 点作为 sanity check）。

### 5.3 归一化映射检查
- 对 `PASS` 的归一化序列递推，映射回原序列后应仍对 6 点精确成立（允许系数变成 L-依赖的显式函数，因为这是由 g(L) 的比值导致的）。

### 5.4 决定性（determinism）
- 同一输入文件、同一参数、多次运行输出完全一致（桶顺序、CSV 排序、摘要统计稳定）。

---

## 6. 第一轮实验建议（参数与验收）

### 6.1 建议优先跑的切片
- `suffix k=1`、`suffix k=2`
- `prefix r=1`、`prefix r=2`
- `prefix-suffix (r,k)=(1,1)`，再到 `(2,2)`

### 6.2 验收标准（第一轮）
- **正确性**：所有守恒恒等式通过（见 5.1）
- **结构性**：
  - 至少出现一批桶在某个归一化候选下 rank 明显降低（例如从 INCONCLUSIVE 变 PASS 或 rank=1）
  - 对 `prefix-suffix (2,2)`，至少在某个 L（建议 L=6）观察到 `rank(M_L)` 显著小于 81（或小于出现的行/列数），作为进入 C 线的强信号
- **可复现**：输出字节级稳定

---

## 7. 与现有工具链的衔接（不涉及实现细节）
你已有 `mpl-experiments esymb-rank-scan` 的 normalize-aware screen + exact solve 工作流，可复用于 A 中每个桶的跨圈序列扫描与递推恢复（尤其是 rank≤2 的可证书类）。项目 README 也明确该子命令会输出 `rank_scan.csv` 与 `summary.md`。fileciteturn0file0L141-L146

