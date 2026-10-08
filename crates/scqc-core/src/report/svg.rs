//! 报告内联 SVG 小图（DESIGN §5.16）：knee 曲线、直方图、瀑布图。
//! 只画"小图"；大点集（PCA/UMAP）留给界面用 Plotly 渲染。
//! 所有文本经 `esc` 转义；数值格式固定，输出逐位可复现。

use std::fmt::Write as _;

pub fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

const W: f64 = 640.0;
const H: f64 = 320.0;
const ML: f64 = 64.0;
const MR: f64 = 16.0;
const MT: f64 = 20.0;
const MB: f64 = 44.0;

fn nice_ticks_log(lo: f64, hi: f64) -> Vec<f64> {
    let mut v = Vec::new();
    let mut e = lo.log10().floor() as i32;
    while (10f64).powi(e) <= hi * 1.0001 {
        let t = (10f64).powi(e);
        if t >= lo * 0.9999 {
            v.push(t);
        }
        e += 1;
    }
    v
}

fn fmt_tick(v: f64) -> String {
    if v >= 1e6 {
        format!("{:.0}M", v / 1e6)
    } else if v >= 1e3 {
        format!("{:.0}k", v / 1e3)
    } else {
        format!("{v:.0}")
    }
}

/// knee 曲线：x = 秩（log），y = 总 UMI（log）；标出 knee / inflection / lower 三条水平线。
pub fn knee_plot(
    rank: &[f64],
    total: &[u64],
    knee: u64,
    inflection: u64,
    lower: u64,
    x_label: &str,
    y_label: &str,
) -> String {
    let n = rank.len().min(total.len());
    if n < 2 {
        return String::new();
    }
    let xs: Vec<f64> = rank[..n].iter().map(|r| r.max(1.0)).collect();
    let ys: Vec<f64> = total[..n].iter().map(|&t| (t.max(1)) as f64).collect();
    let (xmin, xmax) = (1.0f64, xs.iter().copied().fold(1.0f64, f64::max));
    let (ymin, ymax) = (
        ys.iter().copied().fold(f64::INFINITY, f64::min).max(1.0),
        ys.iter().copied().fold(1.0, f64::max),
    );
    let pw = W - ML - MR;
    let ph = H - MT - MB;
    let sx =
        |x: f64| ML + (x.log10() - xmin.log10()) / (xmax.log10() - xmin.log10()).max(1e-9) * pw;
    let sy = |y: f64| {
        MT + ph - (y.log10() - ymin.log10()) / (ymax.log10() - ymin.log10()).max(1e-9) * ph
    };
    let mut s = String::new();
    let _ = write!(
        s,
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {W} {H}" width="{W}" height="{H}" role="img" aria-label="barcode rank plot">"##
    );
    let _ = write!(
        s,
        r##"<rect x="0" y="0" width="{W}" height="{H}" fill="#fff"/>"##
    );
    // 网格与刻度
    for t in nice_ticks_log(xmin, xmax) {
        let x = sx(t);
        let _ = write!(
            s,
            r##"<line x1="{x:.1}" y1="{MT}" x2="{x:.1}" y2="{:.1}" stroke="#eee"/><text x="{x:.1}" y="{:.1}" font-size="11" text-anchor="middle" fill="#555">{}</text>"##,
            MT + ph,
            MT + ph + 16.0,
            fmt_tick(t)
        );
    }
    for t in nice_ticks_log(ymin, ymax) {
        let y = sy(t);
        let _ = write!(
            s,
            r##"<line x1="{ML}" y1="{y:.1}" x2="{:.1}" y2="{y:.1}" stroke="#eee"/><text x="{:.1}" y="{:.1}" font-size="11" text-anchor="end" fill="#555">{}</text>"##,
            ML + pw,
            ML - 6.0,
            y + 4.0,
            fmt_tick(t)
        );
    }
    // 曲线（点数多时抽稀到 ≤ 1500 个点）
    let step = (n / 1500).max(1);
    let mut path = String::new();
    for i in (0..n).step_by(step) {
        let _ = write!(
            path,
            "{}{:.1},{:.1}",
            if path.is_empty() { "M" } else { " L" },
            sx(xs[i]),
            sy(ys[i])
        );
    }
    if !(n - 1).is_multiple_of(step) {
        let _ = write!(path, " L{:.1},{:.1}", sx(xs[n - 1]), sy(ys[n - 1]));
    }
    let _ = write!(
        s,
        r##"<path d="{path}" fill="none" stroke="#1f77b4" stroke-width="1.8"/>"##
    );
    for (v, color, label) in [
        (knee, "#d62728", "knee"),
        (inflection, "#ff7f0e", "inflection"),
        (lower, "#7f7f7f", "lower"),
    ] {
        if v == 0 {
            continue;
        }
        let y = sy(v as f64);
        let _ = write!(
            s,
            r##"<line x1="{ML}" y1="{y:.1}" x2="{:.1}" y2="{y:.1}" stroke="{color}" stroke-dasharray="4 3"/><text x="{:.1}" y="{:.1}" font-size="11" text-anchor="end" fill="{color}">{label} = {v}</text>"##,
            ML + pw,
            ML + pw - 4.0,
            y - 4.0
        );
    }
    let _ = write!(
        s,
        r##"<text x="{:.1}" y="{:.1}" font-size="12" text-anchor="middle" fill="#333">{}</text>"##,
        ML + pw / 2.0,
        H - 8.0,
        esc(x_label)
    );
    let _ = write!(
        s,
        r##"<text transform="translate(14,{:.1}) rotate(-90)" font-size="12" text-anchor="middle" fill="#333">{}</text>"##,
        MT + ph / 2.0,
        esc(y_label)
    );
    s.push_str("</svg>");
    s
}

/// 直方图（固定 40 箱）。`log10` 为 true 时横轴取 log10(x+1)。
pub fn histogram(
    values: &[f64],
    log10: bool,
    x_label: &str,
    color: &str,
    max_per_bin_label: &str,
) -> String {
    let vals: Vec<f64> = values
        .iter()
        .copied()
        .filter(|v| v.is_finite())
        .map(|v| if log10 { (v + 1.0).log10() } else { v })
        .collect();
    if vals.is_empty() {
        return String::new();
    }
    let lo = vals.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = vals.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let nb = 40usize;
    let width = ((hi - lo) / nb as f64).max(1e-9);
    let mut bins = vec![0usize; nb];
    for &v in &vals {
        let b = (((v - lo) / width).floor() as usize).min(nb - 1);
        bins[b] += 1;
    }
    let bmax = bins.iter().copied().max().unwrap_or(1).max(1) as f64;
    let (w, h, ml, mr, mt, mb) = (420.0, 220.0, 48.0, 12.0, 14.0, 40.0);
    let pw = w - ml - mr;
    let ph = h - mt - mb;
    let mut s = String::new();
    let _ = write!(
        s,
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" width="{w}" height="{h}" role="img" aria-label="{}">"##,
        esc(x_label)
    );
    let _ = write!(
        s,
        r##"<rect x="0" y="0" width="{w}" height="{h}" fill="#fff"/>"##
    );
    for (i, &c) in bins.iter().enumerate() {
        let x = ml + i as f64 / nb as f64 * pw;
        let bh = c as f64 / bmax * ph;
        let _ = write!(
            s,
            r##"<rect x="{x:.1}" y="{:.1}" width="{:.1}" height="{bh:.1}" fill="{color}"/>"##,
            mt + ph - bh,
            pw / nb as f64 - 1.0
        );
    }
    // 横轴 5 个刻度
    for k in 0..=4 {
        let v = lo + (hi - lo) * k as f64 / 4.0;
        let x = ml + k as f64 / 4.0 * pw;
        let label = if log10 {
            fmt_tick((10f64).powf(v) - 1.0)
        } else if hi <= 1.0 {
            format!("{:.0}%", v * 100.0)
        } else {
            fmt_tick(v)
        };
        let _ = write!(
            s,
            r##"<text x="{x:.1}" y="{:.1}" font-size="11" text-anchor="middle" fill="#555">{label}</text>"##,
            mt + ph + 14.0
        );
    }
    let _ = write!(
        s,
        r##"<text x="{:.1}" y="{:.1}" font-size="11" text-anchor="end" fill="#555">{}</text>"##,
        ml + pw,
        mt + 10.0,
        esc(&max_per_bin_label.replace("{0}", &(bmax as usize).to_string()))
    );
    let _ = write!(
        s,
        r##"<text x="{:.1}" y="{:.1}" font-size="12" text-anchor="middle" fill="#333">{}</text>"##,
        ml + pw / 2.0,
        h - 8.0,
        esc(x_label)
    );
    s.push_str("</svg>");
    s
}

/// 千位分隔符（图上的数字也要好读）。
fn sep(v: usize) -> String {
    let d = v.to_string();
    let b = d.as_bytes();
    let mut out = String::with_capacity(d.len() + d.len() / 3);
    for (i, c) in b.iter().enumerate() {
        if i > 0 && (b.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(*c as char);
    }
    out
}

/// 瀑布图：从左到右每一级的数量（横向条形），显示相邻级的差值。
pub fn waterfall(rows: &[(String, usize)]) -> String {
    if rows.is_empty() {
        return String::new();
    }
    let max = rows.iter().map(|r| r.1).max().unwrap_or(1).max(1) as f64;
    let (w, row_h, ml, mr) = (640.0, 30.0, 200.0, 90.0);
    let h = row_h * rows.len() as f64 + 16.0;
    let pw = w - ml - mr;
    let mut s = String::new();
    let _ = write!(
        s,
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" width="{w}" height="{h}" role="img" aria-label="cells retained by step">"##
    );
    let _ = write!(
        s,
        r##"<rect x="0" y="0" width="{w}" height="{h}" fill="#fff"/>"##
    );
    let mut prev: Option<usize> = None;
    for (i, (label, n)) in rows.iter().enumerate() {
        let y = 8.0 + i as f64 * row_h;
        // 对数刻度：raw barcode（几十万）与最终细胞（几千）差两个数量级
        let bw = if *n == 0 {
            0.0
        } else {
            ((*n as f64).log10() + 1.0) / (max.log10() + 1.0) * pw
        };
        let _ = write!(
            s,
            r##"<text x="{:.1}" y="{:.1}" font-size="12" text-anchor="end" fill="#333">{}</text><rect x="{ml}" y="{y:.1}" width="{bw:.1}" height="{:.1}" fill="{}"/><text x="{:.1}" y="{:.1}" font-size="12" fill="#333">{}</text>"##,
            ml - 8.0,
            y + row_h * 0.65,
            esc(label),
            row_h - 8.0,
            if i + 1 == rows.len() {
                "#2ca02c"
            } else {
                "#1f77b4"
            },
            ml + bw + 6.0,
            y + row_h * 0.65,
            match prev {
                // ASCII 括号：这张图三种语言共用，全角括号会让英文报告里冒出中文标点
                Some(p) if p >= *n => format!("{} (-{})", sep(*n), sep(p - n)),
                Some(p) => format!("{} (+{})", sep(*n), sep(n - p)),
                None => n.to_string(),
            }
        );
        prev = Some(*n);
    }
    s.push_str("</svg>");
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn svgs_render_and_escape() {
        let k = knee_plot(
            &[1.0, 10.0, 100.0, 1000.0],
            &[5000, 1000, 100, 5],
            1000,
            100,
            50,
            "barcode rank (log10)",
            "UMI counts (log10)",
        );
        assert!(k.starts_with("<svg") && k.ends_with("</svg>"));
        assert!(k.contains("knee = 1000"));
        let h = histogram(
            &[1.0, 2.0, 3.0, 100.0],
            true,
            "总 UMI <a>",
            "#000",
            "max 3 cells/bin",
        );
        assert!(h.contains("&lt;a&gt;"));
        let wf = waterfall(&[("raw".into(), 1000), ("cells".into(), 100)]);
        assert!(wf.contains("100 (-900)"));
        assert!(histogram(&[], false, "x", "#000", "").is_empty());
    }
}
