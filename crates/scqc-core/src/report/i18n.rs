//! 报告的三语文案（SPEC §12 · v2.3）。
//!
//! 报告是**存档物**：它会被附在稿件里、发给合作者、在几年后被人翻出来核对数字。
//! 因此它按"英语 + 用户选择的语言"输出两份完整正文，而不是把两种语言逐句穿插——
//! 后者读起来累，也没法单独把英文那一半发给国外合作者。
//!
//! 界面那套词典（`src/i18n/*.ts`）与这里是两套东西，不共享：界面文案短、口语化，
//! 报告文案长、正式，而且报告由引擎独立生成（命令行不经过界面也要能出报告）。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    En,
    Zh,
    Ja,
}

impl Lang {
    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::Zh => "zh",
            Lang::Ja => "ja",
        }
    }
    /// 语言切换按钮上的名字（用该语言自称）
    pub fn endonym(self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::Zh => "简体中文",
            Lang::Ja => "日本語",
        }
    }
    pub fn html_lang(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::Zh => "zh-CN",
            Lang::Ja => "ja",
        }
    }
    pub fn parse(s: &str) -> Option<Lang> {
        match s.trim().to_ascii_lowercase().as_str() {
            "en" | "english" => Some(Lang::En),
            "zh" | "zh-cn" | "chinese" => Some(Lang::Zh),
            "ja" | "jp" | "japanese" => Some(Lang::Ja),
            _ => None,
        }
    }
}

/// `{0}` / `{1}` 占位符替换。比 `format!` 灵活：模板在运行时才选定。
pub fn f(tpl: &str, args: &[&str]) -> String {
    let mut out = String::with_capacity(tpl.len() + 16);
    let bytes: Vec<char> = tpl.chars().collect();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == '{' {
            let mut j = i + 1;
            let mut n = String::new();
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                n.push(bytes[j]);
                j += 1;
            }
            if !n.is_empty() && j < bytes.len() && bytes[j] == '}' {
                if let Ok(k) = n.parse::<usize>() {
                    out.push_str(args.get(k).copied().unwrap_or(""));
                    i = j + 1;
                    continue;
                }
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    out
}

macro_rules! msgs {
    ($($id:ident : $en:expr, $zh:expr, $ja:expr ;)*) => {
        /// 报告文案 id。新增一条就在这里加一行三语，漏一种语言编译不过。
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum M { $($id),* }

        impl M {
            /// 所有文案 id。给"英文文案里不许出现中日文字符"那条测试用。
            pub const ALL: &'static [M] = &[$(M::$id),*];

            pub fn t(self, lang: Lang) -> &'static str {
                match self {
                    $(M::$id => match lang {
                        Lang::En => $en,
                        Lang::Zh => $zh,
                        Lang::Ja => $ja,
                    }),*
                }
            }
            /// 带 `{0}` 占位符的版本
            pub fn f(self, lang: Lang, args: &[&str]) -> String {
                f(self.t(lang), args)
            }
        }
    };
}

msgs! {
    // ---- 通用 ----
    ReportTitle: "scQC quality control report", "scQC 质检报告", "scQC 品質管理レポート";
    NotAvailable: "n/a", "不可用", "該当なし";
    NoneValue: "None", "无", "なし";
    Assay: "Assay", "平台类型", "アッセイ";
    Species: "Species", "物种", "生物種";
    GeneratedUtc: "Generated (UTC)", "生成时间（UTC）", "生成時刻（UTC）";
    LanguageSwitch: "Language", "语言", "言語";
    ShowBoth: "Both", "两种语言", "両方";

    // ---- 目录 ----
    Toc1: "1 Result", "1 结果", "1 結果";
    Toc2: "2 Cells", "2 细胞判定", "2 セル判定";
    Toc3: "3 Quality metrics", "3 质量指标", "3 品質指標";
    Toc4: "4 Filtering", "4 过滤", "4 フィルタリング";
    Toc5: "5 Ambient RNA", "5 环境 RNA", "5 アンビエント RNA";
    Toc6: "6 Sample", "6 样本与输入", "6 サンプルと入力";
    Toc7: "7 Methods", "7 方法与文献", "7 方法と文献";
    Toc8: "8 Parameters", "8 参数", "8 パラメータ";
    Toc9: "", "", "";

    // ---- 章节标题 ----
    S1Title: "1. Result", "1. 结果", "1. 結果";
    S2Title: "2. Cells", "2. 细胞判定", "2. セル判定";
    S3Title: "3. Quality metrics", "3. 质量指标", "3. 品質指標";
    S4Title: "4. Filtering", "4. 过滤", "4. フィルタリング";
    S5Title: "5. Ambient RNA", "5. 环境 RNA", "5. アンビエント RNA";
    S6Title: "6. Sample and inputs", "6. 样本与输入", "6. サンプルと入力";
    S7Title: "7. Methods", "7. 方法与文献", "7. 方法と文献";
    S8Title: "8. Parameters", "8. 参数", "8. パラメータ";
    SubLowQuality: "Low-quality cells", "低质量细胞", "低品質細胞";
    SubDoublets: "Doublets", "双细胞", "ダブレット";
    MethodsStep: "Step", "步骤", "ステップ";
    MethodsCitation: "Method and citation", "方法与文献", "手法と文献";
    S9Title: "", "", "";

    // ---- 警示 ----
    WarningsTitle: "Warnings raised during the run ({0})", "运行中的警示（{0} 条）", "実行中の警告（{0} 件）";

    // ---- 第 1 节 ----
    PlatformType: "Assay", "平台类型", "アッセイ";
    UserConfirmed: "user confirmed/changed", "用户确认/修改", "ユーザーによる確認／変更";
    OvPlatformConfirmed: "assay confirmed by the user", "平台由用户确认", "アッセイはユーザーが確認";
    OvPlatformDiffers: "assay set to {0} by the user (automatic detection suggested {1})", "平台由用户改为 {0}（自动识别为 {1}）", "アッセイをユーザーが {0} に変更（自動判定は {1}）";
    OvSpecies: "species set by the user", "物种由用户指定", "生物種はユーザー指定";
    OvMultiplexN: "multiplexing level set by the user", "多重化重数由用户指定", "多重化数はユーザー指定";
    OvMitoGuardrail: "mitochondrial guardrail set by the user", "线粒体护栏由用户指定", "ミトコンドリアのガードレールはユーザー指定";
    DetectionEvidence: "Detection evidence", "平台识别依据", "判定根拠";

    // ---- 平台识别依据（与界面同一套 code，见 SPEC §10.5）----
    EvMetricsField: "metrics_summary.csv reports {0} as \"{1}\"", "metrics_summary.csv 的 {0} 字段为「{1}」", "metrics_summary.csv の {0} 欄が「{1}」";
    EvMetricsNoChem: "metrics_summary.csv has no chemistry/assay field", "metrics_summary.csv 中无 chemistry/assay 字段", "metrics_summary.csv に chemistry/assay 欄がありません";
    EvMetricsAbsent: "metrics_summary.csv not provided", "未提供 metrics_summary.csv", "metrics_summary.csv が指定されていません";
    EvFeaturesProbeCol: "features.tsv has probe-related columns", "features.tsv 中存在 probe 相关列", "features.tsv に probe 関連の列があります";
    EvFeaturesNoRibo: "no RPS/RPL ribosomal protein genes among {0} features — characteristic of the Flex probe set", "{0} 个特征中没有 RPS/RPL 核糖体蛋白基因——Flex 探针集特征", "{0} 個のフィーチャー中に RPS/RPL リボソームタンパク質遺伝子がありません — Flex プローブセットの特徴";
    EvFeaturesRibo: "{0} RPS/RPL ribosomal protein genes present — characteristic of a whole-transcriptome reference", "存在 {0} 个 RPS/RPL 核糖体蛋白基因——全转录组参考特征", "RPS/RPL リボソームタンパク質遺伝子が {0} 個存在します — 全トランスクリプトームリファレンスの特徴";
    EvBarcodeMultiplex: "barcodes are {0} nt ({1} nt GEM + {2} nt Probe Barcode) across {3} Probe Barcodes", "barcode 为 {0} nt（GEM {1} nt + Probe Barcode {2} nt），共 {3} 种 Probe Barcode", "バーコードは {0} nt（GEM {1} nt + Probe Barcode {2} nt）、Probe Barcode は {3} 種類";
    EvBarcode16nt: "barcodes are 16 nt with no Probe Barcode segment", "barcode 为 16 nt，无 Probe Barcode 结构", "バーコードは 16 nt で Probe Barcode 部分がありません";
    EvBarcodeAtypical: "atypical barcode length distribution: {0}", "barcode 长度分布不典型：{0}", "バーコード長の分布が非典型的：{0}";
    EvNGeneExpression: "{0} Gene Expression features (Flex ≈ 18k, whole transcriptome ≈ 36k)", "{0} 个 Gene Expression 特征（Flex ≈ 18k，全转录组 ≈ 36k）", "Gene Expression フィーチャー {0} 個（Flex ≈ 18k、全トランスクリプトーム ≈ 36k）";
    EvSpeciesCounts: "{0} MT- and {1} mt- prefixed genes; {2} RPS/RPL, {3} Rps/Rpl", "MT- 前缀 {0} 个、mt- 前缀 {1} 个；RPS/RPL {2} 个、Rps/Rpl {3} 个", "MT- 接頭辞 {0} 個、mt- 接頭辞 {1} 個、RPS/RPL {2} 個、Rps/Rpl {3} 個";
    EvSpeciesMixed: "both human and mouse mitochondrial genes present — species undetermined", "同时存在人与鼠的线粒体基因——无法判定物种", "ヒトとマウスのミトコンドリア遺伝子が両方存在 — 生物種を判定できません";
    EvSpeciesUnknown: "no MT-/mt- prefixed genes found — species undetermined", "未找到 MT-/mt- 前缀基因——无法判定物种", "MT-/mt- 接頭辞の遺伝子が見つかりません — 生物種を判定できません";
    MultiplexN: "Multiplexing level", "多重化重数", "多重化数";
    NotMultiplexed: "Not multiplexed", "无（非多重化）", "なし（非多重化）";
    RawBarcodes: "Barcodes in raw matrix", "raw barcode 数", "raw マトリックスのバーコード数";
    CellRangerCells: "Cells called by Cell Ranger", "Cell Ranger 判定细胞数", "Cell Ranger が判定した細胞数";
    NGenes: "Features used for QC", "质控使用的特征数", "QC に使用したフィーチャー数";
    RawNotProvided: "raw matrix not provided", "未提供 raw", "raw マトリックスなし";
    RunAndVersion: "Run time / version", "运行时间 / 软件版本", "実行時刻 / ソフトウェアバージョン";
    RandomSeed: "random seed", "随机种子", "乱数シード";
    TotalElapsed: "total elapsed", "总耗时", "総所要時間";
    PlatformCaveats: "First-release limitations for this assay", "本平台第一版限制说明", "このアッセイにおける初版の制限事項";
    InputFile: "Input file ({0})", "输入文件（{0}）", "入力ファイル（{0}）";
    FileLine: "{0} · {1} bytes · SHA-256 {2}…", "{0} · {1} 字节 · SHA-256 {2}…", "{0} · {1} バイト · SHA-256 {2}…";
    FeatureAlignment: "Feature list alignment", "基因列表对齐", "フィーチャー一覧の整合";
    NonGexFeatures: "Non–Gene Expression features", "非基因表达特征", "Gene Expression 以外のフィーチャー";
    AlignNote: "raw {0}, filtered {1}; QC used the {2} in common, by feature ID. {4} of raw UMI fall outside and were not used.",
        "raw {0} 个、filtered {1} 个，质控用按特征 ID 取交集的 {2} 个；raw 中落在交集外的 UMI 占 {4}，未参与计算。",
        "raw {0} 個、filtered {1} 個。QC はフィーチャー ID で共通する {2} 個を使用。raw の UMI のうち {4} は共通部分外で未使用。";
    NonGexNote: "{0} non-Gene-Expression features ({1}), {2} of cell UMI. QC used Gene Expression only; the full feature matrix for the same cells is in filtered_all_features/.",
        "{0} 个非 Gene Expression 特征（{1}），占细胞总 UMI 的 {2}。质控只用 Gene Expression；同一批细胞的全特征矩阵在 filtered_all_features/。",
        "Gene Expression 以外のフィーチャー {0} 個（{1}）、細胞 UMI の {2}。QC は Gene Expression のみを使用し、同じ細胞の全フィーチャー行列は filtered_all_features/ にあります。";

    // ---- 第 2 节 ----
    KneeSectionTitle: "Barcode rank plot", "Barcode-rank 曲线（knee plot）", "バーコードランクプロット";
    FinalMetricsTitle: "Metrics of the retained cells", "最终保留细胞的指标分布", "保持した細胞の指標";
    StatFinalCells: "Cells retained", "最终保留细胞数", "保持した細胞数";
    StatMedianUmi: "Median UMI counts", "总 UMI 中位数", "UMI 数の中央値";
    StatMedianGenes: "Median genes", "基因数中位数", "遺伝子数の中央値";
    StatMedianMito: "Median mitochondrial fraction", "线粒体比例中位数", "ミトコンドリア割合の中央値";
    StatMedianRibo: "Median ribosomal fraction", "核糖体比例中位数", "リボソーム割合の中央値";
    RiboNotShown: "not shown (see note)", "不显示（见说明）", "非表示（注記参照）";
    RiboHiddenTitle: "Ribosomal fraction is not shown", "本报告不显示核糖体比例", "本レポートではリボソーム割合を表示しません";
    MitoHeading: "Mitochondrial read fraction — {0}", "线粒体读数比例 —— {0}", "ミトコンドリアリード割合 — {0}";
    MitoViability: "cell viability", "细胞活性", "細胞の生存性";
    MitoViabilityNote: "An elevated mitochondrial read fraction usually reflects a damaged membrane and leaking cytoplasmic mRNA, and is a signal of declining viability.",
        "线粒体读数比例升高通常反映细胞膜受损、胞质 mRNA 泄漏，是细胞活性下降的信号。",
        "ミトコンドリアリード割合の上昇は通常、細胞膜の損傷と細胞質 mRNA の漏出を反映し、生存性の低下を示します。";
    MitoStripping: "nuclear isolation quality", "核剥离质量", "核単離の質";
    MitoStrippingNote: "Nuclei should contain no mitochondrial transcripts. In single-nucleus data the mitochondrial signal comes from incompletely stripped cytoplasm or from ambient RNA, so a higher fraction means poorer isolation. The semantics are the reverse of single-cell data: it is not a viability indicator.",
        "细胞核内理论上不含线粒体转录本；单核数据中的线粒体读数来自未剥离干净的胞质或环境 RNA，比例越高说明核剥离质量越差（语义与单细胞相反：它不是活性指标）。",
        "核内には本来ミトコンドリア転写産物は存在しません。シングルニュクレアスデータのミトコンドリアリードは、除去しきれなかった細胞質かアンビエント RNA に由来し、割合が高いほど核単離の質が低いことを意味します（シングルセルとは意味が逆で、生存性の指標ではありません）。";
    MitoDampened: "probe-set under-coverage", "探针降采样后的线粒体信号", "プローブによる過小カバレッジ";
    MitoDampenedNote: "The Flex probe set under-covers mitochondrial genes, so the mitochondrial fraction reads low and cannot be compared with thresholds or published values from whole-transcriptome gene expression data.",
        "Flex 探针集对线粒体基因的覆盖被压低，线粒体比例的数值偏低，不能与 GEX 数据的阈值或经验值直接比较。",
        "Flex のプローブセットはミトコンドリア遺伝子を十分にカバーしないため、ミトコンドリア割合は低めに出ます。全トランスクリプトームの遺伝子発現データの閾値や経験値と直接比較することはできません。";

    // ---- 第 3 节 ----
    CcMode: "Mode", "模式", "モード";
    CcEmptyDrops: "EmptyDrops (raw matrix, re-evaluated here)", "EmptyDrops（raw 矩阵，本工具判定）", "EmptyDrops（raw マトリックスを用いて本ツールが判定）";
    CcTrustUpstream: "Cell Ranger cell list used as-is (this assay does not run EmptyDrops)", "采用 Cell Ranger 细胞集合（本平台不做 EmptyDrops）", "Cell Ranger の細胞リストをそのまま使用（このアッセイでは EmptyDrops を実行しません）";
    CcRawMissing: "Cell Ranger cell list used as-is (no raw matrix provided)", "未提供 raw 矩阵，采用 Cell Ranger 细胞集合", "raw マトリックスがないため Cell Ranger の細胞リストをそのまま使用";
    CcKnee: "Knee (UMI counts)", "knee（总 UMI）", "ニー（UMI 数）";
    CcInflection: "Inflection (UMI counts)", "inflection（总 UMI）", "変曲点（UMI 数）";
    CcLower: "Ambient pool upper bound (lower)", "环境池上界 lower", "アンビエントプール上限（lower）";
    CcAlpha: "Dirichlet concentration α", "Dirichlet 浓度 α", "ディリクレ集中度 α";
    CcTested: "Candidate barcodes tested", "被检验的候选 barcode 数", "検定した候補バーコード数";
    CcCalled: "Cells called by scQC", "本工具判定细胞数", "本ツールが判定した細胞数";
    CcRescued: "Called empty by Cell Ranger, cell by scQC", "Cell Ranger 判空、本工具判细胞", "Cell Ranger は空、本ツールは細胞と判定";
    CcLost: "Called cell by Cell Ranger, empty by scQC", "Cell Ranger 判细胞、本工具判空", "Cell Ranger は細胞、本ツールは空と判定";
    CcRescuedTitle: "Called empty by Cell Ranger, cell by scQC: {0}", "Cell Ranger 判为空、本工具判为细胞：{0} 个", "Cell Ranger は空、本ツールは細胞と判定：{0} 件";
    CcRescuedBody: "Median gene count {0}; most expressed: {1}. If the list is dominated by MALAT1, mitochondrial or ribosomal genes, these are more likely damaged cells or debris than a new cell type — read them together with section 5. Full list in cells_rescued.csv.",
        "这些 barcode 的基因数中位数为 {0}；高表达基因：{1}。若高表达基因以 MALAT1、线粒体基因、核糖体基因为主，更可能是受损细胞或碎片而非新细胞类型，请结合第 5 节的过滤结果解读。完整列表见 cells_rescued.csv。",
        "これらのバーコードの検出遺伝子数の中央値は {0}、発現上位は {1} です。MALAT1・ミトコンドリア遺伝子・リボソーム遺伝子が上位を占める場合、新規細胞種ではなく損傷細胞や残骸である可能性が高く、第 5 節の結果と併せて解釈してください。全リストは cells_rescued.csv にあります。";
    CcLostBody: "Called cell by Cell Ranger, empty droplet by scQC: {0} (see cells_lost.csv and cells_removed.csv).",
        "Cell Ranger 判为细胞、本工具判为空液滴：{0} 个（见 cells_lost.csv 与 cells_removed.csv）。",
        "Cell Ranger は細胞、本ツールは空ドロップレットと判定：{0} 件（cells_lost.csv と cells_removed.csv を参照）。";
    CcNoteEmptyDrops: "Barcodes with total UMI at or below lower define the ambient profile; every candidate above it is tested against that profile.",
        "总 UMI 不高于 lower 的 barcode 构成环境谱，其上的候选逐个与该谱做检验。",
        "総 UMI が lower 以下のバーコードでアンビエントプロファイルを構成し、それを超える候補を 1 件ずつ検定します。";
    CcNoteTrust: "Cell calling was not re-evaluated for this assay; the upstream cell list is used unchanged.",
        "本平台不重新鉴定真细胞，直接采用上游的细胞集合。",
        "このアッセイではセルコーリングを再判定せず、上流の細胞リストをそのまま使用します。";

    // ---- 第 4 节 ----
    AmbEstimator: "Estimator", "估计器", "推定器";
    AmbRho: "Contamination ρ", "污染度 ρ", "混入率 ρ";
    AmbNote: "Note", "说明", "説明";
    AmbSoupName: "A: SoupX-style (empty-droplet profile + cluster markers)", "A：SoupX 式（空液滴谱 + 簇标记基因）", "A：SoupX 法（空ドロップレットプロファイル + クラスターマーカー）";
    AmbSoupUnreliable: "A: SoupX-style (⚠ estimate unreliable, excluded from the verdict)", "A：SoupX 式（⚠️ 估计不可信，未参与判定）", "A：SoupX 法（⚠ 推定値が信頼できないため判定から除外）";
    AmbSoupDetail: "interval {0}-{1}", "区间 {0}-{1}", "区間 {0}-{1}";
    AmbDecontxName: "B: DecontX (background mode, mean over cells)", "B：DecontX（background 模式，逐细胞均值）", "B：DecontX（background モード、細胞平均）";
    AmbDecontxDetail: "median {0}, range {1}-{2}", "中位数 {0}，范围 {1}-{2}", "中央値 {0}、範囲 {1}-{2}";
    AmbConverged: "converged", "已收敛", "収束";
    AmbNotConverged: "iteration limit reached", "未收敛", "反復上限に到達";
    AmbMitoName: "C: mitochondrial method", "C：线粒体法", "C：ミトコンドリア法";
    AmbMitoDetail: "mitochondrial fraction in the ambient profile {0}", "环境谱中的线粒体比例 {0}", "アンビエントプロファイル中のミトコンドリア割合 {0}";
    AmbMitoUnusable: "the ambient profile contains too little mitochondrial signal for this estimator to be stable",
        "环境谱中的线粒体信号过低，该估计不稳定",
        "アンビエントプロファイルのミトコンドリアシグナルが低すぎ、この推定は安定しません";
    AmbVerdictLine: "Consistency: {0}", "一致性判定：{0}", "一致性判定：{0}";
    AmbRelativeDiff: "relative difference {0}, interval {1}", "相对差 {0}，区间 {1}", "相対差 {0}、区間 {1}";
    AmbRatingLine: "Rating: {0}", "评级：{0}", "評価：{0}";
    AmbCorrectionLine: "Correction: {0}", "校正建议：{0}", "補正の推奨：{0}";
    AmbDisagreeReasons: "Possible reasons for the disagreement:", "分歧的可能原因：", "不一致の考えられる原因：";
    AmbSevereTitle: "Severe contamination", "严重污染", "深刻な混入";
    AmbReferenceTitle: "Reference range", "参考区间", "参照範囲";
    AmbRefMean: "Mean contamination for this assay (literature)", "本平台平均污染度（文献）", "このアッセイの平均混入率（文献値）";
    AmbRefGrey: "Correction greyed out below", "低于此值校正置灰", "この値未満では補正を無効化";
    AmbRefDiscourage: "Correction discouraged above", "高于此值不推荐校正", "この値を超える場合は補正を推奨しない";
    AmbThisSample: "This sample", "本样本", "本サンプル";
    AmbTopGenesTitle: "Genes most affected by ambient RNA (by expected ambient counts)", "受环境 RNA 影响最大的基因（按期望环境计数）", "アンビエント RNA の影響が大きい遺伝子（期待アンビエントカウント順）";
    AmbGene: "Gene", "基因", "遺伝子";
    AmbObserved: "Observed", "观测计数", "観測カウント";
    AmbExpected: "Expected ambient", "期望环境计数", "期待アンビエントカウント";
    AmbShare: "Ambient share", "环境占比", "アンビエント割合";
    AmbCorrTitle: "Ambient RNA correction applied: {0}", "环境 RNA 校正（已执行：{0}）", "アンビエント RNA 補正（実行済み：{0}）";
    AmbCorrForced: "This correction was forced by the user against a greyed-out or discouraged recommendation. Its reliability is reduced; state this limitation when reporting results.",
        "本次校正是在\"置灰 / 不推荐\"建议下由用户强制执行的，校正结果的可信度下降，请在结果解读中注明。",
        "この補正は、無効化または非推奨の推奨に反してユーザーが強制実行したものです。信頼性が低下するため、結果の解釈時に明記してください。";
    AmbCorrMethod: "Method", "方法", "手法";
    AmbCorrRho: "ρ used", "所用 ρ", "使用した ρ";
    AmbCorrRhoValue: "mean {0} (range {1}–{2})", "均值 {0}（范围 {1}–{2}）", "平均 {0}（範囲 {1}–{2}）";
    AmbCorrTotals: "Total UMI before → after", "总 UMI 校正前 → 后", "総 UMI（補正前 → 後）";
    AmbCorrTotalsValue: "{0} → {1} (removed {2})", "{0} → {1}（扣除 {2}）", "{0} → {1}（差し引き {2}）";
    AmbCorrCells: "Cells", "细胞数", "細胞数";
    AmbCorrShortfall: "Cells where the target deduction could not be met", "目标扣除量未达成的细胞数", "目標の差し引き量に達しなかった細胞数";
    AmbCorrOutput: "Output", "输出", "出力";
    AmbCorrOutputValue: "decontaminated/ ({0}); the uncorrected matrix remains in filtered/", "decontaminated/（{0}）；未校正矩阵保留在 filtered/", "decontaminated/（{0}）；未補正の行列は filtered/ に残ります";
    AmbCorrInteger: "integer counts", "整数计数", "整数カウント";
    AmbCorrReal: "non-integer expected counts", "非整数期望计数", "非整数の期待カウント";
    AmbCorrBefore: "Before", "校正前", "補正前";
    AmbCorrAfter: "After", "校正后", "補正後";
    AmbCorrRemovedFrac: "Removed", "扣除比例", "差し引き割合";
    AmbCorrCluster: "Coarse cluster", "粗簇", "粗いクラスター";
    AmbCorrClusterCells: "Cells / before", "细胞数 / 校正前", "細胞数 / 補正前";
    AmbNoCorrection: "Counts were not corrected.", "未对计数做校正。", "カウントは補正していません。";
    AmbCaveat: "Every ambient RNA correction biases expression: highly expressed genes are corrected more strongly than lowly expressed ones. A corrected matrix must never be corrected again (constraint C8). The uncorrected matrix is always kept.",
        "任何环境 RNA 校正都会引入表达量偏倚：高表达基因被扣得更多，低表达基因被扣得更少。校正过的矩阵不得再次校正（硬约束 C8）。未校正矩阵始终保留。",
        "アンビエント RNA の補正はいずれも発現に偏りをもたらします（高発現遺伝子ほど強く補正されます）。補正済みの行列を再度補正してはなりません（制約 C8）。未補正の行列は常に保存されます。";
    AmbNoReference: "No published reference range for this assay, so no high/low verdict is given.",
        "本平台没有已发表的参考基准，因此不做高低判定。",
        "このアッセイには公表された参照範囲がないため、高低の判定は行いません。";
    AmbSevere1: "Estimated contamination {0} is well above the reference range for this assay (mean {1}). At this level computational correction becomes less reliable and introduces a clear expression bias.",
        "本样本污染度估计 {0}，显著高于参考范围（本平台平均 {1}）。在此污染水平下，计算校正的可靠性下降，且会引入明显的表达量偏倚。",
        "本サンプルの推定混入率 {0} は、このアッセイの参照範囲（平均 {1}）を大きく上回ります。この水準では計算による補正の信頼性が低下し、明確な発現の偏りが生じます。";
    AmbSevere2: "Check the dissociation protocol and cell viability.", "核查样本解离流程与细胞活率。", "解離プロトコルと細胞生存率を確認してください。";
    AmbSevere3: "If feasible, consider preparing the library again.", "如条件允许，考虑重新制备文库。", "可能であればライブラリの再調製を検討してください。";
    AmbSevere4: "If the data must be used, correct conservatively and state the limitation when reporting results.",
        "若必须使用当前数据，建议仅做保守校正，并在结果解读中注明此局限。",
        "このデータを使用せざるを得ない場合は保守的に補正し、結果の解釈時にこの限界を明記してください。";
    AmbSkipped: "Ambient RNA was not estimated for this sample.", "本样本未估计环境 RNA。", "本サンプルではアンビエント RNA を推定していません。";

    // ---- 一致性 / 评级 / 校正状态 ----
    VerdictAgree: "estimators agree", "一致", "一致";
    VerdictDisagree: "estimators disagree", "分歧", "不一致";
    VerdictSingle: "only one estimate available", "仅一个估计可用", "利用できる推定値は 1 つのみ";
    VerdictUnavailable: "not available", "不可用", "利用不可";
    RatingNormal: "normal", "正常", "正常";
    RatingElevated: "elevated", "偏高", "やや高い";
    RatingSevere: "severe", "严重", "深刻";
    RatingUnrated: "not rated (no reference range)", "不判定（无参考基准）", "判定なし（参照範囲なし）";
    RatingUnavailable: "not available", "不可用", "利用不可";
    CorrGreyed: "greyed out (correction would do more harm than good at this level)", "置灰（当前污染水平下校正弊大于利）", "無効（この水準では補正は利益より害が大きい）";
    CorrAvailable: "available (off by default; compare before and after)", "可用（默认关闭，建议对比前后结果）", "利用可（既定はオフ。前後を比較してください）";
    CorrDiscouraged: "not recommended", "不推荐", "非推奨";
    CorrUnrated: "user's decision", "由用户判断", "ユーザー判断";

    // ---- 第 5 节 ----
    DdqcDims: "Criteria used: {0}.", "使用的维度：{0}。", "使用した基準：{0}。";
    DimCounts: "Total UMI (lower bound)", "总 UMI（下限）", "総 UMI（下限）";
    DimGenes: "Gene count (lower bound)", "基因数（下限）", "遺伝子数（下限）";
    DimMito: "Mitochondrial fraction (upper bound)", "线粒体比例（上限）", "ミトコンドリア割合（上限）";
    DimRibo: "Ribosomal fraction (upper bound)", "核糖体比例（上限）", "リボソーム割合（上限）";
    DdqcInput: "Cells entering this step", "输入细胞数", "このステップに入った細胞数";
    DdqcCoarse: "Removed by the coarse pre-filter (genes < {0} or mitochondrial > {1})", "粗过滤剔除（基因 < {0} 或线粒体 > {1}）", "粗フィルタで除外（遺伝子 < {0} またはミトコンドリア > {1}）";
    DdqcRemovedTotal: "Removed in total", "ddqc 剔除合计", "除外合計";
    DdqcKept: "Retained", "保留", "保持";
    DdqcFixedAlone: "Fixed thresholds alone would remove", "固定阈值单独会剔除", "固定閾値のみで除外される数";
    DdqcStrictExtra: "Passed the adaptive thresholds but stopped by the fixed thresholds (strict mode)", "自适应阈值放过、但被固定阈值硬下限拦下（严格模式）", "適応閾値は通過したが固定閾値で除外（厳格モード）";
    DdqcLooseNote: "Fixed thresholds are reported for comparison only and did not filter", "固定阈值仅作对照，未参与过滤", "固定閾値は比較のみで、フィルタには使用していません";
    DdqcExtraKept: "Retained by ddqc but removed by fixed thresholds", "ddqc 比固定阈值多保留", "ddqc が固定閾値より多く保持した数";
    DdqcExtraKeptValue: "{0} (median gene count {1})", "{0}（这些细胞的基因数中位数 {1}）", "{0}（これらの細胞の検出遺伝子数の中央値 {1}）";
    DdqcMad: "MAD multiplier", "MAD 倍数", "MAD 倍率";
    DdqcRemovedByDim: "Removed by criterion", "各维度剔除数", "基準ごとの除外数";
    DdqcThresholdsTitle: "Per-cluster thresholds (fine clustering)", "各簇阈值（精细聚类）", "クラスターごとの閾値（細かいクラスタリング）";
    DdqcThClusterDim: "Cluster / criterion", "簇 / 维度", "クラスター / 基準";
    DdqcThCellsMedian: "Cells / median", "细胞数 / 中位数", "細胞数 / 中央値";
    DdqcThMadThreshold: "MAD / threshold", "MAD / 阈值", "MAD / 閾値";
    DdqcThRemoved: "Removed", "剔除", "除外";
    DdqcInactive: "{0} → inactive", "{0} → 不生效", "{0} → 無効";
    DdqcClusterLabel: "Cluster {0} · {1}", "簇 {0} · {1}", "クラスター {0} · {1}";
    DdqcNoteRiboDisabled: "The ribosomal criterion is disabled for this assay: {0}", "核糖体比例维度已由平台配置禁用：{0}", "このアッセイではリボソーム基準を無効化しています：{0}";
    DdqcNoteNoGuardrail: "No mitochondrial guardrail is applied for this assay (none has been calibrated); per-cluster adaptive thresholds are used alone.",
        "线粒体维度不加护栏（本平台未标定），仅用簇内自适应阈值。",
        "このアッセイにはミトコンドリアのガードレールを設けていません（未較正のため）。クラスター内の適応閾値のみを使用します。";
    DdqcNoteGuardrail: "Mitochondrial guardrail: the per-cluster threshold is never set below {0}; the gene-count threshold is never set above 200.",
        "线粒体阈值护栏：簇内阈值不低于 {0}；基因数阈值不高于 200。",
        "ミトコンドリアのガードレール：クラスター内の閾値は {0} を下回らず、遺伝子数の閾値は 200 を超えません。";
    DdqcNoteMinDiff: "The mitochondrial criterion uses a minimum difference of {0} so that a MAD of zero does not remove every cell.",
        "线粒体维度使用最小差值 {0}（MAD 为 0 时避免全量剔除）。",
        "ミトコンドリア基準では最小差分 {0} を使用し、MAD が 0 のときに全細胞が除外されるのを防ぎます。";

    // ---- 第 6 节 ----
    DblCells: "Cells tested (after low-quality filtering)", "检测细胞数（低质量过滤后）", "検定した細胞数（低品質フィルタ後）";
    DblSamples: "Samples (multiplexed: by Probe Barcode)", "样本数（多重化按 Probe Barcode）", "サンプル数（多重化時は Probe Barcode ごと）";
    DblExpectedRate: "Expected multiplet rate", "预期双细胞率", "想定マルチプレット率";
    DblCxds: "Called by cxds", "cxds 判定双细胞", "cxds が判定";
    DblDf: "Called by DoubletFinder", "DoubletFinder 判定双细胞", "DoubletFinder が判定";
    DblDfValue: "{0} (pK = {1})", "{0}（pK = {1}）", "{0}（pK = {1}）";
    DblHigh: "High-confidence doublets (called by both, removed)", "高置信双细胞（两方法均判为双细胞，剔除）", "高信頼ダブレット（両手法が判定、除外）";
    DblBorderline: "Borderline (called by one estimator only)", "边界情况（仅一个方法判为双细胞）", "境界例（片方の手法のみが判定）";
    DblJaccard: "Agreement between estimators (Jaccard)", "两方法一致性（Jaccard）", "手法間の一致度（Jaccard）";
    DblFinalRate: "Final doublet rate (high-confidence / cells tested)", "最终双细胞率（高置信 / 检测细胞数）", "最終ダブレット率（高信頼 / 検定細胞数）";
    DblFormulaTitle: "Expected multiplet rate assumption", "所用预期双细胞率假设", "想定マルチプレット率の前提";
    DblFormula: "expected rate = {0} / 1000 × n_loaded({1}) / 1000 ÷ multiplexing({2}) = {3}",
        "预期率 = {0} / 1000 × n_loaded({1}) / 1000 ÷ 多重化重数({2}) = {3}",
        "想定率 = {0} / 1000 × n_loaded({1}) / 1000 ÷ 多重化数({2}) = {3}";
    DblAssumption: "The expected multiplet rate is an empirical assumption from the loaded-cell count, not an estimate from the data. There is no consensus method for estimating it directly; the rate is adjustable.",
        "预期双细胞率是依据上样细胞数的经验假设，不是从数据估计出来的。目前没有从数据直接估计的共识方法；该比率可调整。",
        "想定マルチプレット率は投入細胞数に基づく経験的な前提であり、データから推定した値ではありません。データから直接推定する合意された手法は存在せず、この値は調整可能です。";
    DblBorderlineRemoved: "removed as configured", "按参数剔除", "設定に従い除外";
    DblBorderlineKept: "retained and flagged", "保留并标注", "保持して印を付けます";
    DblBorderlineLine: "Borderline doublets: {0}.", "边界双细胞：{0}。", "境界のダブレット：{0}。";
    DblByMethod: "cxds / DoubletFinder / both", "cxds / DoubletFinder / 两者一致", "cxds / DoubletFinder / 両者一致";
    DblByMethodValue: "{0} / {1} / {2}", "{0} / {1} / {2}", "{0} / {1} / {2}";
    DblPerSampleTitle: "Per sample (Probe Barcode)", "各样本（Probe Barcode）", "サンプルごと（Probe Barcode）";
    DblThSample: "Sample", "样本", "サンプル";
    DblThCellsExpected: "Cells / expected", "细胞数 / 预期", "細胞数 / 想定";
    DblThCalls: "cxds / DoubletFinder", "cxds / DoubletFinder 判定", "cxds / DoubletFinder の判定";
    DblThHighBorderPk: "High-confidence / borderline / pK", "高置信 / 边界 / pK", "高信頼 / 境界 / pK";

    // ---- 第 7 节 ----
    WfRawBarcodes: "All barcodes (incl. empty droplets)", "raw barcode（含空液滴）", "全バーコード（空ドロップレットを含む）";
    WfCellRanger: "Cells called by Cell Ranger", "Cell Ranger 判定细胞", "Cell Ranger が判定した細胞";
    WfScqc: "After cell calling (scQC re-check)", "细胞判定后（scQC 复核）", "セル判定後（scQC による再判定）";
    WfAfterDdqc: "After low-quality filtering", "低质量过滤后", "低品質フィルタ後";
    WfAfterDoublet: "After doublet removal", "双细胞剔除后", "ダブレット除外後";
    WfAfterDoubletFinal: "After doublet removal (= final)", "双细胞剔除后（= 最终保留）", "ダブレット除外後（= 最終）";
    WfFinal: "Final", "最终保留", "最終";
    RemovedByStepTitle: "Removed by step", "各步骤剔除", "ステップごとの除外数";
    WfAmbientNote: "Ambient RNA correction changes counts, not the number of cells, so it is not a step in this chain.",
        "环境 RNA 校正改的是计数、不改变细胞数，因此不在这条链里。",
        "アンビエント RNA 補正は細胞数ではなくカウントを変えるため、この流れには現れません。";
    StepCellCalling: "Cell calling (Cell Ranger cells called empty)", "真细胞鉴定（Cell Ranger 细胞被判为空液滴）", "セルコーリング（Cell Ranger の細胞を空と判定）";
    StepDdqc: "Low-quality filtering (ddqc)", "低质量细胞过滤（ddqc）", "低品質細胞のフィルタリング（ddqc）";
    StepDoublet: "Doublets (high-confidence)", "双细胞（高置信）", "ダブレット（高信頼）";
    FinalRatio: "Final / Cell Ranger", "最终保留 / Cell Ranger", "最終 / Cell Ranger";
    FinalRatioValue: "{0} / {1} ({2})", "{0} / {1}（{2}）", "{0} / {1}（{2}）";
    FinalOutputs: "Output", "输出", "出力";
    FinalOutputsValue: "filtered/ (cleaned matrix), cells_passed.txt, cells_removed.csv (per-cell reason), qc_metrics.csv",
        "filtered/（清洗后矩阵）、cells_passed.txt、cells_removed.csv（逐细胞剔除原因）、qc_metrics.csv",
        "filtered/（クリーニング済み行列）、cells_passed.txt、cells_removed.csv（細胞ごとの除外理由）、qc_metrics.csv";

    // ---- 运行警示（按 code 本地化；数字在报告正文里）----
    WarnRawMissing: "No raw matrix: cell calling was not re-evaluated and ambient RNA was not estimated.",
        "无 raw 矩阵：未重新判定真细胞，也未估计环境 RNA。",
        "raw マトリックスがないため、セルコーリングの再判定とアンビエント RNA の推定を行っていません。";
    WarnNonGex: "Non–Gene Expression features were excluded from QC and written back to filtered_all_features/ for the retained cells.",
        "非 Gene Expression 特征未参与质控，已按保留细胞写入 filtered_all_features/。",
        "Gene Expression 以外のフィーチャーは QC から除外し、保持した細胞について filtered_all_features/ に書き出しました。";
    WarnFeatureAligned: "The raw and filtered matrices list different features; QC used the intersection by feature ID.",
        "raw 与 filtered 的特征列表不同，质控使用按特征 ID 取的交集。",
        "raw と filtered のフィーチャー一覧が異なるため、フィーチャー ID の共通部分で QC を行いました。";
    WarnAmbientSkipped: "Ambient RNA was not estimated.", "未估计环境 RNA。", "アンビエント RNA は推定していません。";
    WarnAmbientDisagree: "The two ambient estimators disagree; correction is not recommended.",
        "两个环境 RNA 估计器分歧，不建议校正。",
        "2 つのアンビエント推定器が不一致のため、補正は推奨しません。";
    WarnAmbientSevere: "Ambient RNA is far above the usual range for this assay.",
        "环境 RNA 远高于本实验类型的常见范围。",
        "アンビエント RNA がこのアッセイの通常範囲を大きく超えています。";
    WarnAmbientUnreliable: "One ambient estimate reached the edge of its search range and was excluded from the verdict.",
        "有一个环境 RNA 估计落在搜索区间端点，已排除出判定。",
        "一方のアンビエント推定値が探索範囲の端に達したため、判定から除外しました。";
    WarnDecontxNotConverged: "The per-cell mixture model reached the iteration limit without converging.",
        "逐细胞混合模型达到迭代上限仍未收敛。",
        "細胞ごとの混合モデルが反復上限に達しても収束しませんでした。";
    WarnDdqcMitoNoise: "Mitochondrial thresholds in some clusters sat at the noise floor; the assay guardrail was used instead.",
        "部分簇的线粒体阈值落在噪声水平，已改用实验类型护栏值。",
        "一部クラスターのミトコンドリア閾値がノイズ水準にあったため、アッセイのガードレール値を使用しました。";
    WarnGuardrailOverridden: "The assay mitochondrial guardrail was overridden.",
        "实验类型的线粒体护栏已被覆盖。",
        "アッセイのミトコンドリア・ガードレールが上書きされました。";
    WarnClusteringFragmented: "Clustering produced an unusually large number of small clusters, which makes per-cluster thresholds noisy.",
        "聚类产生了异常多的小簇，会使簇内阈值不稳定。",
        "クラスタリングで小さなクラスターが異常に多く生成され、クラスター内閾値が不安定になります。";
    WarnDoubletRateHigh: "The doublet rate is higher than expected for the number of cells loaded.",
        "双细胞率高于按上样细胞数推算的预期。",
        "投入細胞数から予想される値よりダブレット率が高くなっています。";
    WarnCorrectionSkipped: "Ambient correction was not applied.", "未执行环境 RNA 校正。", "アンビエント補正は実行していません。";
    WarnCorrectionForced: "Ambient correction was applied against the recommendation.",
        "在不推荐的情况下执行了环境 RNA 校正。",
        "推奨に反してアンビエント補正を実行しました。";
    WarnAllFeaturesPartial: "Some retained cells were recovered from the raw matrix and have no non–Gene Expression counts, so they are absent from filtered_all_features/.",
        "部分保留细胞由本工具从 raw 矩阵找回，没有非 Gene Expression 特征的计数，因此未包含在 filtered_all_features/ 中。",
        "一部の保持細胞は raw マトリックスから回収されたもので Gene Expression 以外のカウントを持たないため、filtered_all_features/ には含まれていません。";

    // ---- 平台限制说明 ----
    CaveatSnLower: "The single-nucleus EmptyDrops lower = 50 is a recommended value and has not been calibrated against measured data.",
        "单核 EmptyDrops 的 lower=50 为建议值，尚未用实测数据标定。",
        "シングルニュクレアスの EmptyDrops における lower=50 は推奨値であり、実測データによる較正は行っていません。";
    CaveatSnMito: "The mitochondrial contamination estimator (estimator C) has not been formally calibrated and is offered only as a cross-check.",
        "线粒体法污染度估计（估计器 C）尚未正式标定，仅作交叉参考。",
        "ミトコンドリア法による混入率推定（推定器 C）は正式に較正されておらず、相互参照としてのみ提供しています。";
    CaveatFlexCellCalling: "Cell calling is not performed independently for Flex in this release; Cell Ranger's cell calls are used as-is (calibration pending).",
        "Flex 平台的真细胞鉴定未独立执行，本版沿用 Cell Ranger 的细胞判定（待标定）。",
        "本リリースでは Flex のセルコーリングを独立に実行せず、Cell Ranger の判定をそのまま使用します（較正は未実施）。";
    CaveatFlexMito: "The probe set barely covers mitochondrial genes, so the mitochondrial fraction reads low and is not comparable with whole-transcriptome values. A 5% floor is applied to the adaptive threshold.",
        "探针集几乎不覆盖线粒体基因，线粒体比例偏低，不能与全转录组的数值比较；自适应阈值加了 5% 的下限。",
        "プローブセットがミトコンドリア遺伝子をほとんどカバーしないため、ミトコンドリア割合は低く出て、全トランスクリプトームの値とは比較できません。適応閾値には 5% の下限を設けています。";
    CaveatFlexNoRef: "No public ambient RNA reference range exists for Flex, so contamination is reported as a number without a high/low verdict.",
        "Flex 平台目前没有公开的环境 RNA 参考基准，污染度只报告数值、不做高低判定。",
        "Flex には公開されたアンビエント RNA の参照範囲がないため、混入率は数値のみを報告し高低の判定は行いません。";
    CaveatFlexProbeSet: "The Flex probe set excludes ribosomal protein, non-coding RNA, TCR/IG and HLA/KIR genes, so its feature universe differs from whole-transcriptome gene expression.",
        "Flex 探针集排除核糖体蛋白、非编码 RNA、TCR/IG、HLA/KIR 基因，基因宇宙与 GEX 不同。",
        "Flex のプローブセットはリボソームタンパク質・非コード RNA・TCR/IG・HLA/KIR 遺伝子を除外するため、フィーチャーの構成が全トランスクリプトームの遺伝子発現とは異なります。";
    CaveatFlexMultiplexRate: "The expected doublet rate for multiplexed Flex is divided by the multiplexing level. This is a heuristic rather than an exact formula and can be adjusted in the parameters.",
        "多重化 Flex 的预期双细胞率按 1/重数 修正，这是启发式而非精确公式，可在参数中调整。",
        "多重化 Flex の想定ダブレット率は多重化数で割って補正します。これは正確な式ではなくヒューリスティックであり、パラメータで調整できます。";
    RiboHiddenFlex: "The Flex probe set contains no ribosomal protein genes, so the ribosomal fraction is always zero and cannot serve as a QC metric.",
        "Flex 探针集不含核糖体蛋白基因，核糖体比例恒为 0，无法作为质控指标。",
        "Flex のプローブセットにはリボソームタンパク質遺伝子が含まれないため、リボソーム割合は常に 0 であり QC 指標として使えません。";

    // ---- 分歧原因与校正理由 ----
    AmbReasonA: "(a) the clustering resolution is not appropriate; recomputing with coarser cell-population labels may help",
        "(a) 聚类分辨率不合适，建议使用更粗的细胞群标签重算",
        "(a) クラスタリングの解像度が適切でない可能性があります。より粗い細胞集団ラベルで再計算してください";
    AmbReasonB: "(b) cell types differ markedly in how easily they dissociate, making contamination highly uneven across cells",
        "(b) 不同细胞类型解离难易度差异大，导致污染在细胞间高度不均匀",
        "(b) 細胞種によって解離のしやすさが大きく異なり、混入が細胞間で著しく不均一になっている可能性があります";
    AmbReasonC: "(c) a cell population closely resembles the ambient profile",
        "(c) 存在与环境谱高度相似的细胞群",
        "(c) アンビエントプロファイルに酷似した細胞集団が存在する可能性があります";
    AmbWhyUnavailable: "Neither estimator is available, so no recommendation can be made.",
        "两个估计器均不可用，无法判断是否需要校正。",
        "どちらの推定器も利用できないため、推奨を行えません。";
    AmbWhyDisagree: "The two estimators disagree markedly (relative difference {0}); correction is not recommended regardless of the level.",
        "两种方法分歧显著（相对差 {0}），不推荐校正（无论数值高低）。",
        "2 つの推定器が大きく食い違っています（相対差 {0}）。水準にかかわらず補正は推奨しません。";
    AmbWhyBothLow: "Both estimates are below {0}, which is low contamination. This assay has no reference range, but correction is not advisable at this level.",
        "两个估计值都低于 {0}，属于低污染；本平台虽无参考基准，但在这个水平上不建议校正。",
        "いずれの推定値も {0} 未満で、混入は低い水準です。このアッセイに参照範囲はありませんが、この水準では補正は勧められません。";
    AmbWhyUnrated: "No ambient RNA reference range has been published for this assay; values are reported without a high/low verdict.",
        "本平台没有公开的环境 RNA 参考基准，只报告数值，不做高低判定。",
        "このアッセイにはアンビエント RNA の参照範囲が公表されていないため、数値のみを報告し高低の判定は行いません。";
    AmbWhySevere: "Contamination is well above the reference range (> 20%); computational correction becomes less reliable, and the experimental steps are worth checking.",
        "污染度显著高于参考范围（> 20%），计算校正可靠性下降，建议核查实验环节。",
        "混入率が参照範囲を大きく超えています（> 20%）。計算による補正の信頼性が低下するため、実験工程の確認を推奨します。";
    AmbWhyElevated: "Contamination is elevated; correction is defensible. Compare results before and after.",
        "污染度偏高，可选择校正，建议对比校正前后结果。",
        "混入率がやや高めです。補正は妥当であり、前後の結果を比較してください。";
    AmbWhyNormal: "At this contamination level correction does more harm than good (every method over-corrects on clean data).",
        "当前污染水平下校正弊大于利（所有校正方法在干净数据上都会过度校正）。",
        "この混入水準では補正は利益より害が大きくなります（いずれの手法もクリーンなデータでは過補正します）。";
    AmbWhyNoEstimate: "No estimate available.", "无可用估计。", "利用可能な推定値がありません。";
    CorrMethodSoup: "SoupX-style global subtraction", "SoupX 式全局减法", "SoupX 法による全体的な差し引き";
    CorrMethodDecontx: "DecontX expected native counts", "DecontX 期望原生计数", "DecontX の期待固有カウント";

    // ---- 平台与物种 ----
    PlatformScRna: "Single Cell Gene Expression (3'/5')", "单细胞 RNA-seq（3'/5' 基因表达）", "Single Cell Gene Expression（3'/5'）";
    PlatformSnRna: "Single Nucleus Gene Expression", "单核 RNA-seq", "Single Nucleus Gene Expression";
    PlatformFlexSingle: "Flex (singleplex)", "Flex（单重）", "Flex（シングルプレックス）";
    PlatformFlexMulti: "Flex (multiplexed)", "Flex（多重化）", "Flex（マルチプレックス）";
    SpeciesHuman: "Human", "人", "ヒト";
    SpeciesMouse: "Mouse", "小鼠", "マウス";
    SpeciesOther: "Other (user-specified)", "其他（用户指定）", "その他（ユーザー指定）";

    // ---- SVG 轴标签与直方图 ----
    SvgBarcodeRank: "barcode rank (log10)", "barcode 秩（log10）", "バーコード順位（log10）";
    SvgTotalUmi: "UMI counts (log10)", "总 UMI（log10）", "UMI 数（log10）";
    SvgMaxPerBin: "max {0} cells/bin", "最多 {0} 个细胞/箱", "最大 {0} 細胞/ビン";
    HistUmi: "UMI counts per cell", "每细胞总 UMI", "細胞あたり UMI 数";
    HistGenes: "Genes detected per cell", "每细胞检测到的基因数", "細胞あたり検出遺伝子数";
    HistMito: "Mitochondrial read fraction", "线粒体读数比例", "ミトコンドリアリード割合";
    HistRibo: "Ribosomal protein gene read fraction", "核糖体蛋白基因读数比例", "リボソームタンパク質遺伝子リード割合";

    // ---- 方法学附录的参数标签 ----
    ParamMode: "Mode", "模式", "モード";
    ParamHvg: "Highly variable genes", "HVG 数", "高変動遺伝子数";
    ParamPcs: "Principal components", "主成分数", "主成分数";
    ParamK: "k (kNN)", "k（kNN）", "k（kNN）";
    ParamRes: "Resolution (fine / coarse)", "精细 / 粗分辨率", "解像度（細かい / 粗い）";
    ParamActualClusters: "Clusters found (fine / coarse)", "实际簇数（精细 / 粗）", "得られたクラスター数（細かい / 粗い）";
    ParamAmbientUpper: "Ambient profile UMI upper bound", "环境谱 UMI 上界", "アンビエントプロファイルの UMI 上限";
    ParamSameAsLower: "same as lower", "与 lower 相同", "lower と同じ";
    ParamTfidf: "tf-idf threshold / ambient abundance quantile", "tf-idf 门槛 / 环境丰度分位", "tf-idf 閾値 / アンビエント存在量の分位";
    ParamTfidfValue: "1.0 / top 10%", "1.0 / 前 10%", "1.0 / 上位 10%";
    ParamPrior: "Prior", "先验", "事前分布";
    ParamPriorValue: "Gamma(mean 0.05, sd 0.1)", "Gamma(均值 0.05, sd 0.1)", "Gamma（平均 0.05、sd 0.1）";
    ParamMaxIter: "Max iterations / convergence criterion", "最大迭代 / 收敛判据", "最大反復回数 / 収束基準";
    ParamClustering: "Clustering", "聚类", "クラスタリング";
    ParamCoarseClusters: "coarse-resolution clusters", "粗分辨率簇", "粗い解像度のクラスター";
    ParamDisagreeRule: "Disagreement rule", "分歧判据", "不一致の判定基準";
    ParamDisagreeValue: "relative difference ≥ 50%", "相对差 ≥ 50%", "相対差 ≥ 50%";
    ParamMethod: "Method", "方法", "手法";
    ParamForce: "Forced", "强制执行", "強制実行";
    ParamCoarseFilter: "Coarse pre-filter", "粗过滤", "粗フィルタ";
    ParamCoarseFilterValue: "genes < {0} or mitochondrial > {1}", "基因 < {0} 或线粒体 > {1}", "遺伝子 < {0} またはミトコンドリア > {1}";
    ParamDims: "Criteria", "维度", "基準";
    ParamMitoGuardrail: "Mitochondrial guardrail", "线粒体护栏", "ミトコンドリアのガードレール";
    ParamAuto100: "automatic (100)", "自动（100）", "自動（100）";
    ParamPk: "pK (selected by BCmvn)", "pK（BCmvn 选定）", "pK（BCmvn により選択）";
    ParamExpectedRate: "Expected multiplet rate", "预期双细胞率", "想定マルチプレット率";

    // ---- 第 8 / 9 节与页脚 ----
    MethodsNote: "Implementation notes and licences are on the Credits page of the application.",
        "实现说明与许可证在软件的版权页里。",
        "実装の詳細とライセンスはアプリの著作権ページに記載しています。";
    KnownLimitations: "Known limitations", "已知局限性", "既知の限界";
    GlobalLimitations: "Overall known limitations", "总体已知局限性", "全体的な既知の限界";
    ParamsNote: "Same as run_params.json.", "与 run_params.json 一致。", "run_params.json と同一です。";
    Footer: "scQC {0} · report_data.json", "scQC {0} · report_data.json", "scQC {0} · report_data.json";
    ListSep: ", ", "、", "、";
    RunLine: "{0} · seed {1} · {2} s", "{0} · 随机种子 {1} · {2} s", "{0} · 乱数シード {1} · {2} s";
    InputDirLine: "{0} · {1} files, sizes and SHA-256 in run_record.json",
        "{0} · 共 {1} 个文件，大小与 SHA-256 见 run_record.json",
        "{0} · 計 {1} ファイル、サイズと SHA-256 は run_record.json に記載";
}

/// 警示码 → 该语言下的一句话。未知的码回落到引擎给的原文（保证不丢信息）。
pub fn warning_text(code: &str, lang: Lang, fallback: &str) -> String {
    let m = match code {
        "RAW_MISSING" => M::WarnRawMissing,
        "NON_GEX_FEATURES_EXCLUDED" => M::WarnNonGex,
        "FEATURE_SET_ALIGNED" => M::WarnFeatureAligned,
        "AMBIENT_SKIPPED" => M::WarnAmbientSkipped,
        "AMBIENT_METHODS_DISAGREE" => M::WarnAmbientDisagree,
        "AMBIENT_SEVERE" => M::WarnAmbientSevere,
        "AMBIENT_ESTIMATOR_UNRELIABLE" => M::WarnAmbientUnreliable,
        "AMBIENT_DECONTX_NOT_CONVERGED" => M::WarnDecontxNotConverged,
        "DDQC_MITO_THRESHOLD_AT_NOISE" => M::WarnDdqcMitoNoise,
        "MITO_GUARDRAIL_OVERRIDDEN" => M::WarnGuardrailOverridden,
        "CLUSTERING_FRAGMENTED" => M::WarnClusteringFragmented,
        "DOUBLET_RATE_HIGH" => M::WarnDoubletRateHigh,
        "CORRECTION_SKIPPED" => M::WarnCorrectionSkipped,
        "CORRECTION_FORCED" => M::WarnCorrectionForced,
        "ALL_FEATURES_PARTIAL" => M::WarnAllFeaturesPartial,
        _ => return fallback.to_string(),
    };
    m.t(lang).to_string()
}

/// 平台识别依据：`CodedText` → 该语言的一句话。界面与报告用同一套 code（SPEC §10.5），
/// 因此报告的英文正文里不会再出现中文原句。未知 code 回落到引擎给的中文原句。
pub fn evidence_text(code: &str, args: &[String], lang: Lang, fallback: &str) -> String {
    let m = match code {
        "metrics_field" => M::EvMetricsField,
        "metrics_no_chem" => M::EvMetricsNoChem,
        "metrics_absent" => M::EvMetricsAbsent,
        "features_probe_col" => M::EvFeaturesProbeCol,
        "features_no_ribo" => M::EvFeaturesNoRibo,
        "features_ribo" => M::EvFeaturesRibo,
        "barcode_multiplex" => M::EvBarcodeMultiplex,
        "barcode_16nt" => M::EvBarcode16nt,
        "barcode_atypical" => M::EvBarcodeAtypical,
        "n_gene_expression" => M::EvNGeneExpression,
        "species_counts" => M::EvSpeciesCounts,
        "species_mixed" => M::EvSpeciesMixed,
        "species_unknown" => M::EvSpeciesUnknown,
        _ => return fallback.to_string(),
    };
    let refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    m.f(lang, &refs)
}

/// 用户覆盖项：`CodedText` → 该语言的一句话。与识别依据同一套机制（SPEC §10.5）。
pub fn override_text(code: &str, args: &[String], lang: Lang) -> String {
    let m = match code {
        "override_platform_confirmed" => M::OvPlatformConfirmed,
        "override_platform_differs" => M::OvPlatformDiffers,
        "override_species" => M::OvSpecies,
        "override_multiplex_n" => M::OvMultiplexN,
        "override_mito_guardrail" => M::OvMitoGuardrail,
        _ => return code.to_string(),
    };
    let refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    m.f(lang, &refs)
}

/// 模板里用到的固定文案（minijinja 用 `{{ t.xxx }}` 取）。
/// 与 `M` 同源，这里只是给模板一个扁平的取值面。
pub fn template_labels(lang: Lang) -> std::collections::BTreeMap<&'static str, String> {
    use M::*;
    let mut m = std::collections::BTreeMap::new();
    let mut put = |k: &'static str, v: M| {
        m.insert(k, v.t(lang).to_string());
    };
    put("report_title", ReportTitle);
    put("assay", Assay);
    put("species", Species);
    put("generated_utc", GeneratedUtc);
    put("language", LanguageSwitch);
    put("show_both", ShowBoth);
    put("toc1", Toc1);
    put("toc2", Toc2);
    put("toc3", Toc3);
    put("toc4", Toc4);
    put("toc5", Toc5);
    put("toc6", Toc6);
    put("toc7", Toc7);
    put("toc8", Toc8);
    put("toc9", Toc9);
    put("s1_title", S1Title);
    put("s2_title", S2Title);
    put("s3_title", S3Title);
    put("s4_title", S4Title);
    put("s5_title", S5Title);
    put("s6_title", S6Title);
    put("s7_title", S7Title);
    put("s8_title", S8Title);
    put("s9_title", S9Title);
    put("platform_type", PlatformType);
    put("user_confirmed", UserConfirmed);
    put("detection_evidence", DetectionEvidence);
    put("multiplex_n", MultiplexN);
    put("not_multiplexed", NotMultiplexed);
    put("raw_barcodes", RawBarcodes);
    put("cellranger_cells", CellRangerCells);
    put("n_genes", NGenes);
    put("raw_not_provided", RawNotProvided);
    put("run_and_version", RunAndVersion);
    put("random_seed", RandomSeed);
    put("total_elapsed", TotalElapsed);
    put("platform_caveats", PlatformCaveats);
    put("knee_section", KneeSectionTitle);
    put("final_metrics", FinalMetricsTitle);
    put("ribo_hidden_title", RiboHiddenTitle);
    put("cc_mode", CcMode);
    put("amb_estimator", AmbEstimator);
    put("amb_rho", AmbRho);
    put("amb_note", AmbNote);
    put("amb_disagree_reasons", AmbDisagreeReasons);
    put("amb_severe_title", AmbSevereTitle);
    put("amb_reference_title", AmbReferenceTitle);
    put("amb_top_genes_title", AmbTopGenesTitle);
    put("amb_gene", AmbGene);
    put("amb_observed", AmbObserved);
    put("amb_expected", AmbExpected);
    put("amb_share", AmbShare);
    put("amb_corr_forced", AmbCorrForced);
    put("amb_corr_before", AmbCorrBefore);
    put("amb_corr_after", AmbCorrAfter);
    put("amb_corr_removed", AmbCorrRemovedFrac);
    put("amb_corr_cluster", AmbCorrCluster);
    put("amb_corr_cluster_cells", AmbCorrClusterCells);
    put("amb_no_correction", AmbNoCorrection);
    put("ddqc_removed_by_dim", DdqcRemovedByDim);
    put("ddqc_thresholds_title", DdqcThresholdsTitle);
    put("ddqc_th_cluster_dim", DdqcThClusterDim);
    put("ddqc_th_cells_median", DdqcThCellsMedian);
    put("ddqc_th_mad_threshold", DdqcThMadThreshold);
    put("ddqc_th_removed", DdqcThRemoved);
    put("dbl_formula_title", DblFormulaTitle);
    put("dbl_per_sample_title", DblPerSampleTitle);
    put("dbl_th_sample", DblThSample);
    put("dbl_th_cells_expected", DblThCellsExpected);
    put("dbl_th_calls", DblThCalls);
    put("dbl_th_high_border_pk", DblThHighBorderPk);
    put("removed_by_step_title", RemovedByStepTitle);
    put("wf_ambient_note", WfAmbientNote);
    put("sub_low_quality", SubLowQuality);
    put("sub_doublets", SubDoublets);
    put("methods_step", MethodsStep);
    put("methods_citation", MethodsCitation);
    put("methods_note", MethodsNote);
    put("known_limitations", KnownLimitations);
    put("global_limitations", GlobalLimitations);
    put("params_note", ParamsNote);
    m
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn formats_placeholders() {
        assert_eq!(f("{0} / {1}", &["a", "b"]), "a / b");
        assert_eq!(f("no args", &[]), "no args");
        assert_eq!(f("{0}{0}", &["x"]), "xx");
        assert_eq!(f("missing {3}", &["x"]), "missing ");
        assert_eq!(f("brace { not a slot", &[]), "brace { not a slot");
    }

    #[test]
    fn every_message_has_three_languages() {
        // Toc9 / S9Title 是章节重排后留下的空位，故意为空
        let allow_empty = [M::Toc9, M::S9Title];
        for m in M::ALL {
            if allow_empty.contains(m) {
                continue;
            }
            for l in [Lang::En, Lang::Zh, Lang::Ja] {
                assert!(!m.t(l).is_empty(), "{m:?} 缺少 {l:?} 文案");
            }
        }
    }

    /// 报告的英文正文里不许出现中日文字符——**包括标点**。
    ///
    /// 上一轮踩过两次：一次是引擎把中文原句直接塞进报告，另一次更隐蔽——
    /// 模板里写死了全角括号和顿号（`（10x-Flex-singleplex）`、`A、B、C`），
    /// 只数汉字的检查根本发现不了。所以这里连 CJK 标点和全角字符一起查，
    /// 查的对象是**所有英文文案 + 共用模板**，而不是某一次跑出来的报告。
    #[test]
    fn english_text_has_no_cjk() {
        fn cjk(c: char) -> bool {
            matches!(c,
                '\u{3000}'..='\u{303f}'   // CJK 标点（、。「」等）
                | '\u{3040}'..='\u{30ff}' // 平假名 / 片假名
                | '\u{4e00}'..='\u{9fff}' // 汉字
                | '\u{ff01}'..='\u{ff60}' // 全角 ASCII（！（）：等）
                | '\u{ffe0}'..='\u{ffe6}'
            )
        }
        fn check(what: &str, text: &str) {
            let bad = text.chars().find(|c| cjk(*c));
            assert!(
                bad.is_none(),
                "英文内容 {what} 里出现了 CJK 字符 {:?}：{text}",
                bad.unwrap()
            );
        }
        for m in M::ALL {
            check(&format!("{m:?}"), m.t(Lang::En));
        }
        for (k, v) in super::template_labels(Lang::En) {
            check(k, &v);
        }
        for e in super::super::methods::entries(Lang::En) {
            check(&e.key, &e.name);
            check(&e.key, &e.citation);
            check(&e.key, &e.description);
            for l in &e.limitations {
                check(&e.key, l);
            }
        }
        for l in super::super::methods::global_limitations(Lang::En) {
            check("limitations_global", &l);
        }
        // 模板对三种语言共用，所以渲染出来的部分一个 CJK 字符都不能有。
        // `{# ... #}` 是给维护者看的注释，不进输出，先去掉再查。
        // CSS / JS 注释也是给维护者看的，同样不进正文
        fn strip_code_comments(src: &str) -> String {
            let mut out = String::with_capacity(src.len());
            let mut rest = src;
            while let Some(i) = rest.find("/*") {
                out.push_str(&rest[..i]);
                match rest[i..].find("*/") {
                    Some(j) => rest = &rest[i + j + 2..],
                    None => {
                        rest = "";
                        break;
                    }
                }
            }
            out.push_str(rest);
            out
        }
        fn strip_jinja_comments(src: &str) -> String {
            let mut out = String::with_capacity(src.len());
            let mut rest = src;
            while let Some(i) = rest.find("{#") {
                out.push_str(&rest[..i]);
                match rest[i..].find("#}") {
                    Some(j) => rest = &rest[i + j + 2..],
                    None => {
                        rest = "";
                        break;
                    }
                }
            }
            out.push_str(rest);
            out
        }
        check(
            "section.html",
            &strip_code_comments(&strip_jinja_comments(super::super::SECTION)),
        );
        check(
            "template.html",
            &strip_code_comments(&strip_jinja_comments(super::super::TEMPLATE)),
        );
    }

    #[test]
    fn lang_parses() {
        assert_eq!(Lang::parse("EN"), Some(Lang::En));
        assert_eq!(Lang::parse("zh-CN"), Some(Lang::Zh));
        assert_eq!(Lang::parse("ja"), Some(Lang::Ja));
        assert_eq!(Lang::parse("de"), None);
    }
}
