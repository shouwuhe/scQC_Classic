//! 命令行参数（SPEC §10.1；DESIGN §9.1）。

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Parser, Debug)]
#[command(
    name = "scqc-engine",
    version,
    about = "scQC 单细胞质检引擎（命令行）",
    long_about = "scQC 引擎。stdout 只输出 NDJSON 事件，日志走 stderr。"
)]
pub struct Cli {
    /// 输出更详细的 stderr 日志
    #[arg(long, global = true)]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// 打印版本、git 哈希、编译目标
    Version,
    /// 只做平台识别，输出识别结果与依据
    Detect(InputArgs),
    /// 运行全流程
    Run(RunArgs),
    /// 运行金标准测试与平台专项测试，打印人类可读表格
    Verify(VerifyArgs),
    /// 打印所用算法的论文出处与实现说明（界面的版权页读它）
    Credits(CreditsArgs),
}

#[derive(Args, Debug, Clone)]
pub struct CreditsArgs {
    /// 语言：en / zh / ja
    #[arg(long, default_value = "en")]
    pub lang: String,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlatformArg {
    Auto,
    #[value(name = "10x-scRNA")]
    TenxScRna,
    #[value(name = "10x-snRNA")]
    TenxSnRna,
    #[value(name = "10x-Flex-singleplex")]
    TenxFlexSingleplex,
    #[value(name = "10x-Flex-multiplex")]
    TenxFlexMultiplex,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpeciesArg {
    Auto,
    Human,
    Mouse,
    Other,
}

#[derive(Args, Debug, Clone)]
pub struct InputArgs {
    /// raw（未过滤）矩阵目录；缺失时真细胞鉴定与环境 RNA 诊断不可用
    #[arg(long)]
    pub raw: Option<PathBuf>,

    /// filtered 矩阵目录
    #[arg(long)]
    pub filtered: PathBuf,

    /// Cell Ranger 的 metrics_summary.csv（可选，用于平台识别）
    #[arg(long)]
    pub metrics: Option<PathBuf>,

    #[arg(long, value_enum, default_value = "auto")]
    pub platform: PlatformArg,

    #[arg(long, value_enum, default_value = "auto")]
    pub species: SpeciesArg,

    /// 物种为 other 时：线粒体基因名前缀（如 "MT-"）
    #[arg(long)]
    pub mito_prefix: Option<String>,

    /// 物种为 other 时：核糖体蛋白基因名前缀，逗号分隔（如 "RPS,RPL"）
    #[arg(long)]
    pub ribo_prefixes: Option<String>,

    /// 线粒体阈值护栏（0–1）：ddqc 的簇内线粒体阈值不低于此值。
    /// 覆盖 Platform Profile 的取值（物种为 other 时也用它）。SPEC §7.2.4 要求该参数可调。
    #[arg(long)]
    pub mito_guardrail: Option<f64>,

    /// 多重化 Flex 的重数
    #[arg(long)]
    pub multiplex_n: Option<u32>,
}

#[derive(Args, Debug, Clone)]
pub struct RunArgs {
    #[command(flatten)]
    pub input: InputArgs,

    /// 输出目录（必须为新目录或空目录，绝不覆盖输入）
    #[arg(long)]
    pub output: PathBuf,

    /// 参数 JSON 文件（缺省使用 Profile 默认值）
    #[arg(long)]
    pub params: Option<PathBuf>,

    /// 进度输出格式（目前只支持 ndjson）
    #[arg(long, default_value = "ndjson")]
    pub progress_format: String,

    /// 随机种子（默认固定，保证可复现）
    #[arg(long)]
    pub seed: Option<u64>,

    /// 线程数（0 = 自动）
    #[arg(long, default_value_t = 0)]
    pub threads: usize,

    /// 环境 RNA 校正方法（none / soup / decontx；覆盖参数文件的 correction.method）。
    /// 只有步骤 5 的建议为"可用"时才执行，否则跳过并告警；永不串联两种方法。
    #[arg(long)]
    pub correct: Option<String>,

    /// 即使步骤 5 建议不推荐也执行校正（写入报告）
    #[arg(long, default_value_t = false)]
    pub force_correct: bool,

    /// 跑完第 7 步后停下来，发 `await_correction` 事件并等 stdin 给出校正决定，
    /// 然后才跑第 8、9 步。给桌面界面用：改这一个决定不必重算前七步。
    #[arg(long, default_value_t = false)]
    pub await_correction: bool,

    /// 报告语言：en / zh / ja。报告始终输出英语，再加上这里指定的语言
    /// （指定 en 时只出英语）。SPEC §12。
    #[arg(long, default_value = "en")]
    pub report_lang: String,
}

#[derive(Args, Debug, Clone)]
pub struct VerifyArgs {
    /// 金标准数据目录
    #[arg(long, default_value = "tests/golden")]
    pub golden_dir: PathBuf,
}
