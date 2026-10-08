//! 方法学附录（SPEC §12 第 8 节【硬约束】：每个算法的论文引用、参数、已知局限性）。
//!
//! 三语（英 / 中 / 日）。文献引用本身不翻译——它就是英文原文。
//! `docs/methods.md` 由本模块生成，`cargo test -p scqc-core` 会检查两者同步。

use serde::{Deserialize, Serialize};

use super::i18n::Lang;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MethodEntry {
    pub key: String,
    pub name: String,
    pub citation: String,
    pub license_note: String,
    pub description: String,
    /// (参数名, 本次取值)
    pub params: Vec<(String, String)>,
    pub limitations: Vec<String>,
}

/// 一条三语文本。
type T3 = [&'static str; 3];

fn pick(t: T3, lang: Lang) -> String {
    match lang {
        Lang::En => t[0],
        Lang::Zh => t[1],
        Lang::Ja => t[2],
    }
    .to_string()
}

fn e(
    key: &str,
    name: T3,
    citation: &str,
    license: T3,
    description: T3,
    limitations: &[T3],
    lang: Lang,
) -> MethodEntry {
    MethodEntry {
        key: key.into(),
        name: pick(name, lang),
        citation: citation.into(),
        license_note: pick(license, lang),
        description: pick(description, lang),
        params: Vec::new(),
        limitations: limitations.iter().map(|l| pick(*l, lang)).collect(),
    }
}

const LIC_PAPER_GPL3: T3 = [
    "Implemented from the publication; DropletUtils is GPL-3 and its source was not consulted.",
    "依据论文自行实现（DropletUtils 为 GPL-3，未参考其源码）",
    "論文に基づく独自実装（DropletUtils は GPL-3 のためソースは参照していません）",
];
const LIC_OWN: T3 = ["Own implementation.", "自行实现", "独自実装"];

/// 全部方法条目（参数留空，由引擎填充）。
pub fn entries(lang: Lang) -> Vec<MethodEntry> {
    vec![
        e(
            "knee",
            ["Knee and inflection of the barcode rank curve", "Barcode-rank 曲线的 knee / inflection", "バーコードランク曲線のニーと変曲点"],
            "Lun ATL et al. (2019) EmptyDrops: distinguishing cells from empty droplets in droplet-based single-cell RNA sequencing data. Genome Biology 20:63 (barcodeRanks approach)",
            LIC_PAPER_GPL3,
            [
                "Unique total counts are collapsed and the log–log curve is differentiated by local quadratic regression. The knee is the point of minimum signed curvature; the inflection is the point of minimum first derivative.",
                "对唯一 total 折叠后的 log–log 曲线做局部二次回归求导；knee 取有符号曲率最小处，inflection 取一阶导最小处。",
                "ユニークな総カウントを畳み込み、log–log 曲線を局所二次回帰で微分します。ニーは符号付き曲率が最小の点、変曲点は一次微分が最小の点です。",
            ],
            &[[
                "With an unusual curve shape (for example a double plateau) the knee can land in an unexpected place. The knee is only used as a protective line above which barcodes are called cells directly; it does not by itself determine the cell set.",
                "曲线形态异常（如双平台）时 knee 可能落在非预期位置；knee 只用于\"以上直接判细胞\"的保护线，不单独决定细胞集合。",
                "曲線の形状が非典型的な場合（二段のプラトーなど）、ニーが想定外の位置に来ることがあります。ニーは「これより上は無条件に細胞とみなす」保護線としてのみ使われ、細胞集合を単独で決めるものではありません。",
            ]],
            lang,
        ),
        e(
            "emptydrops",
            ["EmptyDrops cell calling", "EmptyDrops 真细胞鉴定", "EmptyDrops によるセルコーリング"],
            "Lun ATL et al. (2019) Genome Biology 20:63",
            LIC_PAPER_GPL3,
            [
                "Barcodes with total UMI ≤ lower form the ambient profile (Good–Turing smoothed). A Dirichlet-multinomial likelihood is used with α fitted by maximum likelihood; p-values come from a Pólya urn Monte-Carlo simulation, are BH-corrected, and barcodes with FDR ≤ 0.01 are called cells. Barcodes above the knee are retained directly.",
                "以总 UMI ≤ lower 的空液滴构建环境谱（Good–Turing 平滑），Dirichlet-multinomial 似然、α 极大似然；Pólya 罐子蒙特卡洛求 p 值，BH 校正，FDR ≤ 0.01 判为细胞；knee 以上直接保留。",
                "総 UMI が lower 以下のバーコードでアンビエントプロファイルを構成し（Good–Turing 平滑化）、ディリクレ多項尤度と最尤法による α を用います。p 値はポリアの壺モンテカルロで求め、BH 補正後 FDR ≤ 0.01 を細胞と判定します。ニーより上のバーコードはそのまま保持します。",
            ],
            &[
                [
                    "The ambient profile depends on the lower threshold. Differences from Cell Ranger come mainly from low-UMI barcodes recovered here; those may be damaged cells or debris and should be read together with the ddqc result.",
                    "环境谱依赖 lower 阈值；与 Cell Ranger 的差异主要来自低 UMI 的\"抢救\"细胞，这些细胞可能是受损细胞或碎片，需结合 ddqc 结果解读。",
                    "アンビエントプロファイルは lower 閾値に依存します。Cell Ranger との差の多くは低 UMI の回収バーコードに由来し、損傷細胞や残骸の可能性があるため ddqc の結果と併せて解釈してください。",
                ],
                [
                    "Monte-Carlo p-value resolution is limited by niters (default 10000).",
                    "蒙特卡洛 p 值精度受 niters 限制（默认 10000）。",
                    "モンテカルロ p 値の分解能は niters（既定 10000）に制限されます。",
                ],
            ],
            lang,
        ),
        e(
            "embed",
            ["Normalisation / HVG / PCA / kNN / SNN / Leiden clustering", "归一化 / HVG / PCA / kNN / SNN / Leiden 聚类", "正規化 / HVG / PCA / kNN / SNN / Leiden クラスタリング"],
            "Traag VA, Waltman L, van Eck NJ (2019) From Louvain to Leiden. Scientific Reports 9:5233; Halko N, Martinsson PG, Tropp JA (2011) SIAM Review 53(2) (randomised SVD)",
            [
                "Implemented from the publications; leidenalg and igraph are GPL and their sources were not consulted.",
                "依据论文自行实现（leidenalg / igraph 为 GPL，未参考其源码）",
                "論文に基づく独自実装（leidenalg / igraph は GPL のためソースは参照していません）",
            ],
            [
                "log1p(CP10k) → 2000 highly variable genes by the binned-dispersion method → z-score (clipped at ±10) → randomised SVD to 50 principal components → exact kNN (k = 20) → SNN Jaccard graph → Leiden (RB modularity) at resolution 1.3 (fine, for ddqc) and 0.3 (coarse, for ambient RNA).",
                "log1p(CP10k) → 分箱离散度法选 2000 个高变基因 → z-score（裁剪 ±10）→ 随机化 SVD 取 50 个主成分 → 精确 kNN（k=20）→ SNN Jaccard 图 → Leiden（RB 模块度），分辨率 1.3（精细，供 ddqc）与 0.3（粗，供环境 RNA 估计）。",
                "log1p(CP10k) → ビン分割分散法で高変動遺伝子 2000 個を選択 → z-score（±10 でクリップ）→ ランダム化 SVD で主成分 50 個 → 厳密 kNN（k=20）→ SNN Jaccard グラフ → Leiden（RB モジュラリティ）。解像度は 1.3（細かい、ddqc 用）と 0.3（粗い、アンビエント RNA 用）。",
            ],
            &[
                [
                    "The clustering is only a grouping device for later steps; it is not a cell-type annotation.",
                    "聚类只作为后续步骤的分组依据，不代表细胞类型注释。",
                    "このクラスタリングは後続ステップのためのグループ分けであり、細胞種のアノテーションではありません。",
                ],
                [
                    "kNN uses exact search so that results are bit-reproducible; HNSW approximate search is not used because its layer randomisation cannot be seeded.",
                    "kNN 采用精确搜索以保证逐位可复现；HNSW 近似搜索因层数随机化不可设种子而未启用。",
                    "結果をビット単位で再現するため kNN は厳密探索を使用します。HNSW の近似探索は層数の乱択にシードを設定できないため使用していません。",
                ],
            ],
            lang,
        ),
        e(
            "soup",
            ["Ambient RNA estimator A: SoupX-style", "环境 RNA 估计器 A：SoupX 式", "アンビエント RNA 推定器 A：SoupX 法"],
            "Young MD, Behjati S (2020) SoupX removes ambient RNA contamination from droplet-based single-cell RNA sequencing data. GigaScience 9(12)",
            [
                "Implemented from the publication; SoupX is GPL-2 and its source was not consulted.",
                "依据论文自行实现（SoupX 为 GPL-2，未参考其源码）",
                "論文に基づく独自実装（SoupX は GPL-2 のためソースは参照していません）",
            ],
            [
                "Empty-droplet counts are summed and normalised into an ambient profile. Cluster-specific marker genes are found by tf-idf. In clusters that do not express a marker, its observed counts are treated as contamination; a Poisson upper-tail test (BH) removes pairs that look like genuine expression. The contamination fraction ρ is then the mode of the conjugate Gamma posterior, iterated until self-consistent.",
                "空液滴计数和归一化为环境谱；tf-idf 找簇特异标记基因；在不表达该基因的簇中，把观测计数视为污染，Poisson 上尾检验（BH）排除看起来是真实表达的基因–簇对；ρ 取共轭 Gamma 后验的众数，迭代至自洽。",
                "空ドロップレットのカウントを合計・正規化してアンビエントプロファイルを作ります。tf-idf でクラスター特異的マーカー遺伝子を求め、その遺伝子を発現しないクラスターでは観測カウントを混入とみなします。ポアソン上側検定（BH）で真の発現に見える組を除外し、共役ガンマ事後分布の最頻値として ρ を求め、自己無撞着になるまで反復します。",
            ],
            &[
                [
                    "It estimates contamination that matches the empty-droplet profile and depends on marker genes being abundant enough in the ambient pool; with few markers the estimate is unstable.",
                    "估计的是\"来自空液滴环境谱\"的污染，依赖标记基因在环境中足够丰富；标记基因过少时估计不稳。",
                    "空ドロップレットのプロファイルに由来する混入を推定するため、マーカー遺伝子がアンビエント中に十分存在することが前提です。マーカーが少ないと推定が不安定になります。",
                ],
                [
                    "The output is a sample-wide ρ (as in SoupX by default) and does not describe cell-to-cell variation in contamination.",
                    "输出为样本全局 ρ（与 SoupX 默认一致），不反映细胞间的污染差异。",
                    "出力はサンプル全体の ρ（SoupX の既定と同じ）であり、細胞間の混入の違いは表しません。",
                ],
                [
                    "If the mode reaches the edge of the search range the estimate is flagged unreliable and excluded from the consistency verdict.",
                    "众数落在搜索区间端点时，该估计被标记为不可信并排除出一致性判定。",
                    "最頻値が探索範囲の端に達した場合、その推定値は信頼できないものとして一致性判定から除外されます。",
                ],
            ],
            lang,
        ),
        e(
            "decontx",
            ["Ambient RNA estimator B: DecontX (background mode)", "环境 RNA 估计器 B：DecontX（background 模式）", "アンビエント RNA 推定器 B：DecontX（background モード）"],
            "Yang S et al. (2020) Decontamination of ambient RNA in single-cell RNA-seq with DecontX. Genome Biology 21:57",
            [
                "Implemented from the publication; celda is MIT-licensed and no code was copied.",
                "依据论文自行实现（celda 为 MIT，本实现未复制其代码）",
                "論文に基づく独自実装（celda は MIT ライセンスですが、コードは複製していません）",
            ],
            [
                "Each cell's counts are modelled as a mixture of a cluster-native distribution and an ambient distribution; in background mode the ambient distribution is fixed to the empty-droplet profile. Mean-field variational EM estimates the native proportion θ per cell, and ρ = 1 − θ.",
                "每个细胞的计数为\"簇原生分布\"与\"环境分布\"的混合，环境分布固定为空液滴谱（background 模式）；平均场变分 EM 估计每细胞的原生比例 θ，ρ = 1 − θ。",
                "各細胞のカウントを「クラスター固有分布」と「アンビエント分布」の混合としてモデル化し、background モードではアンビエント分布を空ドロップレットプロファイルに固定します。平均場変分 EM で細胞ごとの固有割合 θ を推定し、ρ = 1 − θ とします。",
            ],
            &[
                [
                    "It is conservative for transcriptionally similar populations (shared genes are assigned to the native component); housekeeping genes are mostly called native.",
                    "对表达相似的细胞群倾向保守（把共有基因判为原生）；管家基因多被判为原生。",
                    "発現が似た集団に対しては保守的で（共有遺伝子を固有側に割り当てる）、ハウスキーピング遺伝子は多くが固有と判定されます。",
                ],
                [
                    "Results are sensitive to clustering granularity: clusters that are too fine underestimate contamination, clusters that are too coarse overestimate it.",
                    "结果对聚类粒度敏感：簇过细会低估污染，簇过粗会高估。",
                    "結果はクラスタリングの粒度に敏感です。細かすぎると混入を過小評価し、粗すぎると過大評価します。",
                ],
                [
                    "This implementation correlates with celda at r = 0.99, but its mean is systematically about 4 percentage points lower.",
                    "本实现与 celda 的 ρ 相关 0.99，但均值系统性略低（约 4 个百分点）。",
                    "本実装は celda の ρ と相関 0.99 ですが、平均値は系統的に約 4 ポイント低くなります。",
                ],
            ],
            lang,
        ),
        e(
            "mito",
            ["Ambient RNA estimator C: mitochondrial method", "环境 RNA 估计器 C：线粒体法", "アンビエント RNA 推定器 C：ミトコンドリア法"],
            "Caglayan E et al. (2022) Neuronal ambient RNA contamination causes misinterpreted and masked cell types in brain single-nuclei datasets. Neuron 110(24)",
            LIC_OWN,
            [
                "Nuclei should contain no mitochondrial transcripts, so in single-nucleus data the mitochondrial read fraction divided by the mitochondrial fraction of the ambient profile approximates the contamination fraction.",
                "细胞核内理论上不含线粒体转录本，因此单核数据中的线粒体读数比例 ÷ 环境谱中的线粒体比例 ≈ 污染度。",
                "核内には本来ミトコンドリア転写産物が存在しないため、シングルニュクレアスデータではミトコンドリアリード割合 ÷ アンビエントプロファイル中のミトコンドリア割合が混入率の近似になります。",
            ],
            &[[
                "Meaningful only for single-nucleus data; when the ambient profile carries little mitochondrial signal the estimate is unstable and is marked unavailable.",
                "只对单核数据有意义；环境谱中线粒体比例很低时估计不稳（此时标记为不可用）。",
                "シングルニュクレアスデータにのみ意味があります。アンビエントプロファイルのミトコンドリア割合が低い場合は推定が不安定になり、利用不可として扱います。",
            ]],
            lang,
        ),
        e(
            "consensus",
            ["Ambient RNA consistency verdict and correction recommendation", "环境 RNA 一致性判定与校正建议", "アンビエント RNA の一致性判定と補正の推奨"],
            "Wang Q et al. (2024) A systematic assessment of ambient RNA contamination across 68,000 single-cell and single-nucleus samples (reference ranges: single-cell mean 7.79%, single-nucleus 11.80%)",
            LIC_OWN,
            [
                "A relative difference of 50% or more between the two estimates is treated as disagreement (exempt when both are below 5%, where the relative difference carries no practical meaning). The sample is rated normal / elevated / severe against the assay reference range. Correction is not recommended when the estimators disagree or the rating is severe, and is greyed out below the assay's grey-out threshold.",
                "两估计值相对差 ≥ 50% 判为分歧（两者都 < 5% 时豁免：在低值区相对差没有实际意义）；按平台参考区间给出正常 / 偏高 / 严重评级；分歧或严重时不推荐校正；低于置灰阈值时校正置灰。",
                "2 つの推定値の相対差が 50% 以上であれば不一致とみなします（どちらも 5% 未満の場合は免除。低い水準では相対差に実質的な意味がないためです）。アッセイの参照範囲に照らして正常／やや高い／深刻を判定し、不一致または深刻な場合は補正を推奨せず、無効化閾値を下回る場合は補正を無効にします。",
            ],
            &[
                [
                    "The two estimators model different things: the SoupX-style estimator relies on marker genes appearing where they should not, DecontX on decomposing a mixture of native and ambient distributions. Disagreement is common and may reflect clustering resolution, differences in dissociation, or the presence of a population that resembles the ambient profile.",
                    "两个估计器建模对象不同：SoupX 式依赖簇特异标记基因的\"不该出现\"，DecontX 依赖簇原生分布与环境分布的混合分解；分歧常见，原因可能是聚类分辨率、解离难易度差异或存在与环境谱相似的细胞群。",
                    "2 つの推定器はモデル化の対象が異なります。SoupX 法はマーカー遺伝子が「あるはずのない場所」に現れることに依拠し、DecontX は固有分布とアンビエント分布の混合分解に依拠します。不一致はよくあることで、クラスタリング解像度、解離のしやすさの違い、アンビエントプロファイルに似た集団の存在などが原因となります。",
                ],
                [
                    "No ambient RNA reference range has been published for the Flex assay, so values are reported without a high/low verdict.",
                    "Flex 平台缺乏环境 RNA 参考基准，只报数值不判高低。",
                    "Flex アッセイにはアンビエント RNA の参照範囲が公表されていないため、数値のみを報告し高低の判定は行いません。",
                ],
            ],
            lang,
        ),
        e(
            "correct",
            ["Ambient RNA correction (single method, optional)", "环境 RNA 校正（单一方法，可选）", "アンビエント RNA 補正（単一手法、任意）"],
            "Young & Behjati (2020) (subtraction); Yang et al. (2020) (expected native counts)",
            LIC_OWN,
            [
                "SoupX-style: ρ × total UMI ambient counts are removed per cell and distributed over expressed genes according to the ambient profile (integer output). DecontX-style: the expected native counts from the variational EM are written out (non-integer). The two methods are never chained (constraint C8).",
                "SoupX 式：每细胞扣除 ρ × 总 UMI 个环境 UMI，按环境谱在已表达基因上分配（整数）；DecontX 式：输出变分 EM 的期望原生计数（非整数）。两种方法永不串联（C8）。",
                "SoupX 法：細胞ごとに ρ × 総 UMI 個のアンビエント UMI を、アンビエントプロファイルに従って発現遺伝子に配分して差し引きます（整数出力）。DecontX 法：変分 EM の期待固有カウントを出力します（非整数）。2 つの手法を直列に適用することはありません（制約 C8）。",
            ],
            &[
                [
                    "All ambient correction methods bias expression: highly expressed genes are corrected more strongly, lowly expressed genes barely at all.",
                    "所有环境 RNA 校正方法都存在表达量偏倚，对高表达基因校正更强，对低表达基因校正效果有限。",
                    "いずれのアンビエント補正手法も発現に偏りをもたらします。高発現遺伝子ほど強く補正され、低発現遺伝子はほとんど補正されません。",
                ],
                [
                    "Independent benchmarks show every method over-corrects on clean data, which is why the default is to diagnose without correcting.",
                    "独立基准显示所有方法在干净数据上都会过度校正；因此默认只诊断不校正。",
                    "独立したベンチマークでは、いずれの手法もクリーンなデータで過補正することが示されています。そのため既定では診断のみを行い補正しません。",
                ],
            ],
            lang,
        ),
        e(
            "ddqc",
            ["Low-quality cell filtering: ddqc (data-driven QC)", "低质量细胞过滤：ddqc（数据驱动质控）", "低品質細胞のフィルタリング：ddqc（データ駆動型 QC）"],
            "Subramanian A et al. (2022) Biology-inspired data-driven quality control for scientific discovery in single-cell transcriptomics. Genome Biology 23:267",
            [
                "Implemented from the publication; ddqc_R is BSD-3 and the ribosomal regular expression is aligned with it.",
                "依据论文自行实现（ddqc_R 为 BSD-3；核糖体正则与之对齐）",
                "論文に基づく独自実装（ddqc_R は BSD-3。リボソームの正規表現はこれに合わせています）",
            ],
            [
                "After a coarse pre-filter (fewer than 100 genes or more than 80% mitochondrial), adaptive thresholds of median ± 2 × MAD are computed inside each fine cluster for each criterion (lower bounds on counts and genes, upper bounds on mitochondrial and ribosomal fraction), with guardrails (gene threshold ≤ 200, mitochondrial threshold ≥ the assay guardrail). Classic fixed thresholds (mitochondrial < 10%, genes > 500) are reported alongside, and in strict mode they also act as hard bounds.",
                "粗过滤（基因 < 100 或线粒体 > 80%）后，在精细簇内对各指标做中位数 ± 2×MAD 的自适应阈值（计数 / 基因数下限，线粒体 / 核糖体上限），并加护栏（基因数阈值 ≤ 200、线粒体阈值 ≥ 平台护栏）。同时给出固定阈值（mt < 10%、基因 > 500）的对照结果；严格模式下固定阈值同时作为硬性下限。",
                "粗フィルタ（遺伝子 100 未満またはミトコンドリア 80% 超）の後、細かいクラスターごとに各基準について中央値 ± 2 × MAD の適応閾値を計算します（カウント・遺伝子数は下限、ミトコンドリア・リボソーム割合は上限）。ガードレール（遺伝子閾値 ≤ 200、ミトコンドリア閾値 ≥ アッセイのガードレール）を併用します。従来の固定閾値（ミトコンドリア < 10%、遺伝子 > 500）も併記し、厳格モードではこれを硬い下限としても適用します。",
            ],
            &[
                [
                    "Thresholds depend on clustering quality; MAD is unstable in clusters smaller than about 50 cells.",
                    "阈值依赖聚类质量；簇过小（< 50 细胞）时 MAD 估计不稳。",
                    "閾値はクラスタリングの質に依存します。約 50 細胞未満のクラスターでは MAD が不安定になります。",
                ],
                [
                    "In single-nucleus data the mitochondrial MAD is often zero; a minimum difference of 0.5% avoids removing every cell. The ribosomal criterion is disabled for Flex, whose probe set contains no ribosomal protein genes.",
                    "单核数据线粒体 MAD 常为 0，采用最小差值 0.5% 避免全量剔除；Flex 禁用核糖体维度（探针集不含核糖体蛋白基因）。",
                    "シングルニュクレアスデータではミトコンドリアの MAD が 0 になることが多く、最小差分 0.5% を用いて全細胞の除外を防ぎます。Flex ではプローブセットにリボソームタンパク質遺伝子が含まれないため、リボソーム基準を無効にします。",
                ],
                [
                    "There is no gold standard for QC and every threshold involves a subjective choice. This tool lists which cells were retained that a fixed threshold would have removed, so the choice can be reviewed.",
                    "QC 没有金标准，任何阈值都含主观选择；本工具把\"多保留了什么细胞\"列出供人工判断。",
                    "QC にゴールドスタンダードはなく、いかなる閾値にも主観的な選択が含まれます。本ツールは「固定閾値なら除外されていたどの細胞を保持したか」を明示し、判断できるようにします。",
                ],
            ],
            lang,
        ),
        e(
            "cxds",
            ["Doublet detection A: cxds", "双细胞检测 A：cxds", "ダブレット検出 A：cxds"],
            "Bais AS, Kostka D (2020) scds: computational annotation of doublets in single-cell RNA sequencing data. Bioinformatics 36(4):1150–1158",
            [
                "Implemented from the publication; scds is MIT-licensed.",
                "依据论文自行实现（scds 为 MIT）",
                "論文に基づく独自実装（scds は MIT ライセンス）",
            ],
            [
                "Counts are binarised and the 500 genes with the largest p(1−p) are kept. For each gene pair, the log P of observing an excess of mutually exclusive expression is computed under a binomial independence model. A cell's score is the sum of −2 log P over the mutually exclusive pairs it co-expresses; the highest-scoring cells are called according to the expected doublet rate.",
                "二值化后取 p(1−p) 最大的 500 个基因；对每对基因，在\"独立表达\"的二项模型下计算互斥表达数显著偏多的 log P；细胞得分 = 其共表达的互斥基因对的 −2 log P 之和；按预期双细胞率取得分最高者。",
                "カウントを二値化し、p(1−p) が最大の 500 遺伝子を選びます。各遺伝子対について、独立発現を仮定した二項モデルの下で相互排他的発現が過剰になる log P を計算します。細胞のスコアは、その細胞が共発現している相互排他的な遺伝子対の −2 log P の和で、想定ダブレット率に従って上位を判定します。",
            ],
            &[[
                "Only heterotypic doublets (two different cell types) can be identified; homotypic doublets cannot.",
                "只能识别异型双细胞（两个不同类型细胞融合）；同型双细胞无法识别。",
                "識別できるのは異型ダブレット（異なる細胞種の組み合わせ）のみで、同型ダブレットは検出できません。",
            ]],
            lang,
        ),
        e(
            "doubletfinder",
            ["Doublet detection B: DoubletFinder", "双细胞检测 B：DoubletFinder", "ダブレット検出 B：DoubletFinder"],
            "McGinnis CS, Murrow LM, Gartner ZJ (2019) DoubletFinder: Doublet Detection in Single-Cell RNA Sequencing Data Using Artificial Nearest Neighbors. Cell Systems 8(4):329–337",
            [
                "Implemented from the publication; DoubletFinder is CC0.",
                "依据论文自行实现（DoubletFinder 为 CC0）",
                "論文に基づく独自実装（DoubletFinder は CC0）",
            ],
            [
                "Artificial doublets are generated by randomly pairing real cells (pN = 0.25). After merging, normalisation and PCA, each real cell's pANN is the proportion of artificial doublets among its k nearest neighbours; pK is chosen by a BCmvn parameter sweep, and the highest-pANN cells are called according to the expected doublet rate. Each sample is processed independently.",
                "随机配对真实细胞生成人工双细胞（pN = 0.25），合并后归一化 / PCA，每个真实细胞的 pANN = k 近邻中人工双细胞的比例；pK 由 BCmvn 参数扫描选定；按预期双细胞率取 pANN 最高者。每个样本独立运行。",
                "実細胞をランダムに組み合わせて人工ダブレットを生成し（pN = 0.25）、統合後に正規化と PCA を行います。各実細胞の pANN は k 近傍に含まれる人工ダブレットの割合で、pK は BCmvn パラメータ走査で決定し、想定ダブレット率に従って pANN 上位を判定します。サンプルごとに独立に実行します。",
            ],
            &[
                [
                    "Artificial-doublet pairing is random: with a different seed the original implementation returns doublet sets with a Jaccard index of only 0.4–0.6. This implementation fixes the seed for reproducibility, but per-cell agreement with the original cannot be exact.",
                    "人工双细胞配对是随机的：原版实现换随机种子后，双细胞集合的 Jaccard 仅 0.4–0.6；本实现固定种子保证可复现，但与原版逐细胞结果不可能完全一致。",
                    "人工ダブレットの組み合わせはランダムです。オリジナル実装ではシードを変えるとダブレット集合の Jaccard 係数は 0.4–0.6 にとどまります。本実装は再現性のためシードを固定しますが、オリジナルと細胞単位で完全に一致することはありません。",
                ],
                [
                    "There is no consensus method for estimating the expected doublet rate from the data. This tool uses an empirical formula based on the number of cells loaded (and the multiplexing level); the multiplexed-Flex adjustment is heuristic and user-adjustable.",
                    "预期双细胞率没有从数据直接估计的共识方法；本工具使用基于上样细胞数（及多重化重数）的经验公式，多重化 Flex 的修正为启发式，用户可调整。",
                    "想定ダブレット率をデータから直接推定する合意された手法はありません。本ツールは投入細胞数（および多重化数）に基づく経験式を使用し、多重化 Flex の補正はヒューリスティックでユーザーが調整できます。",
                ],
            ],
            lang,
        ),
    ]
}

/// 平台相关的全局已知局限性（SPEC §12 列出的项）。
pub fn global_limitations(lang: Lang) -> Vec<String> {
    let items: [T3; 5] = [
        [
            "QC has no gold standard and threshold choices are subjective. This report gives the basis for every step and the reason each cell was removed, so the choices can be reviewed.",
            "QC 没有金标准，阈值选择存在主观性；本报告给出每一步的依据与被剔除细胞的原因，供人工复核。",
            "QC にゴールドスタンダードはなく、閾値の選択には主観が伴います。本レポートは各ステップの根拠と除外理由を示し、確認できるようにしています。",
        ],
        [
            "Ambient RNA correction biases expression (highly expressed genes are corrected more strongly). The two estimators model different things, so disagreement has several possible causes.",
            "环境 RNA 校正方法存在表达量偏倚（对高表达基因校正更强）；两个估计器建模对象不同，分歧有多种可能原因。",
            "アンビエント RNA 補正は発現に偏りをもたらします（高発現遺伝子ほど強く補正されます）。2 つの推定器はモデル化の対象が異なるため、不一致には複数の原因がありえます。",
        ],
        [
            "DecontX is conservative for similar populations, calls most housekeeping genes native, and is sensitive to clustering granularity.",
            "DecontX 对相似细胞群倾向保守、管家基因多判为原生、对聚类粒度敏感。",
            "DecontX は類似した集団に対して保守的で、ハウスキーピング遺伝子の多くを固有と判定し、クラスタリングの粒度に敏感です。",
        ],
        [
            "There is no consensus method for estimating the doublet rate; the multiplexed-Flex adjustment is heuristic.",
            "双细胞率缺乏共识估计方法；多重化 Flex 的修正为启发式。",
            "ダブレット率の推定に合意された手法はなく、多重化 Flex の補正はヒューリスティックです。",
        ],
        [
            "The Flex assay has no ambient RNA reference range, and its probe set excludes ribosomal protein, ncRNA, TCR/IG and HLA/KIR genes, so its feature universe differs from whole-transcriptome gene expression.",
            "Flex 平台缺乏环境 RNA 参考基准；Flex 探针集排除核糖体蛋白、ncRNA、TCR/IG、HLA/KIR 基因，基因宇宙与 GEX 不同。",
            "Flex アッセイにはアンビエント RNA の参照範囲がなく、プローブセットはリボソームタンパク質・ncRNA・TCR/IG・HLA/KIR 遺伝子を除外するため、フィーチャーの構成が全トランスクリプトームの遺伝子発現とは異なります。",
        ],
    ];
    items.iter().map(|t| pick(*t, lang)).collect()
}

/// 把方法学附录渲染为 Markdown（`docs/methods.md` 的内容，供人阅读与审阅）。
/// 报告 HTML 与该文档同源于 [`entries`]，不会各自漂移。
pub fn to_markdown(lang: Lang) -> String {
    use std::fmt::Write as _;
    let mut s = String::new();
    s.push_str("# scQC 方法学附录\n\n");
    s.push_str(
        "> 本文件由 `scqc-core` 的 `report::methods` 自动生成（`cargo test -p scqc-core` 会同步），\n\
         > 报告里只放「步骤 + 文献引用」，实现说明、参数与已知局限性在这里。\n\n",
    );
    s.push_str("每个算法均依据原始论文实现，未复制任何 GPL 许可证代码（SPEC §9.2）。\n\n");
    for m in entries(lang) {
        let _ = writeln!(s, "## {}\n", m.name);
        let _ = writeln!(s, "**文献**：{}\n", m.citation);
        let _ = writeln!(s, "**许可证**：{}\n", m.license_note);
        let _ = writeln!(s, "{}\n", m.description);
        let _ = writeln!(s, "**已知局限性**\n");
        for l in &m.limitations {
            let _ = writeln!(s, "- {l}");
        }
        s.push('\n');
    }
    s.push_str("## 总体已知局限性\n\n");
    for l in global_limitations(lang) {
        let _ = writeln!(s, "- {l}");
    }
    s
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    /// `docs/methods.md` 必须与本模块同步；不一致时自动写入并让测试失败，重跑即通过。
    #[test]
    fn methods_doc_is_current() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/methods.md")
            .canonicalize()
            .unwrap_or_else(|_| {
                std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/methods.md")
            });
        let want = to_markdown(Lang::Zh);
        let have = std::fs::read_to_string(&path).unwrap_or_default();
        if have != want {
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            std::fs::write(&path, &want).unwrap();
            panic!("docs/methods.md 已过期，本次已重新生成，请重新运行测试");
        }
    }

    #[test]
    fn every_method_has_citation_and_limitations_in_every_language() {
        for lang in [Lang::En, Lang::Zh, Lang::Ja] {
            for m in entries(lang) {
                assert!(!m.citation.is_empty(), "{} 缺少文献引用", m.name);
                assert!(!m.limitations.is_empty(), "{} 缺少已知局限性", m.name);
                assert!(!m.description.is_empty(), "{} 缺少方法描述", m.name);
            }
        }
    }

    /// 三种语言的条目数与 key 必须完全一致，否则报告会出现"只有一种语言有这条"。
    #[test]
    fn all_languages_have_the_same_entries() {
        let keys: Vec<Vec<String>> = [Lang::En, Lang::Zh, Lang::Ja]
            .iter()
            .map(|l| entries(*l).into_iter().map(|m| m.key).collect())
            .collect();
        assert_eq!(keys[0], keys[1]);
        assert_eq!(keys[1], keys[2]);
        assert_eq!(
            global_limitations(Lang::En).len(),
            global_limitations(Lang::Ja).len()
        );
    }
}
