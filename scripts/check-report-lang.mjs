/**
 * 报告语言隔离自检：英文正文里不许出现中日文字符（**包括标点**）。
 *
 * `cargo test` 里那条 `english_text_has_no_cjk` 查的是文案表与模板；
 * 这条查的是**真的跑出来的那份 report.html** —— 数值、SVG 里的标注、
 * `format!` 拼出来的句子都只有在真跑一遍之后才看得到。
 *
 * 踩过的坑：
 *  - 引擎把中文原句直接塞进报告（平台识别依据）；
 *  - 模板里写死全角括号和顿号（`（10x-Flex-singleplex）`、`A、B、C`）；
 *  - 瀑布图 SVG 的标注写成 `4633（−316035）`。
 * 前两条只数汉字发现不了，第三条连模板检查也发现不了。
 *
 * 跑法：node scripts/check-report-lang.mjs <report.html> [...]
 */
import { readFileSync } from 'node:fs'

const CJK =
  /[　-〿぀-ヿㇰ-ㇿ一-鿿！-｠￠-￦]/

/** 去掉 <script>/<style>，它们的内容不是正文 */
function stripCode(html) {
  return html.replace(/<(script|style)\b[^>]*>[\s\S]*?<\/\1>/gi, '')
}

/** 按 `<section class="doc" data-lang="xx">` 切开每种语言的正文 */
function docsOf(html) {
  const out = []
  const re = /<section class="doc"[^>]*data-lang="([a-z-]+)"[^>]*>/g
  const hits = [...html.matchAll(re)]
  for (let i = 0; i < hits.length; i++) {
    const start = hits[i].index + hits[i][0].length
    const end = i + 1 < hits.length ? hits[i + 1].index : html.length
    out.push({ lang: hits[i][1], body: html.slice(start, end) })
  }
  return out
}

const files = process.argv.slice(2)
if (files.length === 0) {
  console.error('用法：node scripts/check-report-lang.mjs <report.html> [...]')
  process.exit(2)
}

let failed = 0
for (const file of files) {
  const html = stripCode(readFileSync(file, 'utf8'))
  const docs = docsOf(html)
  if (docs.length === 0) {
    console.error(`✗ ${file}：找不到 <section class="doc" data-lang="…">`)
    failed++
    continue
  }
  const en = docs.find((d) => d.lang === 'en')
  if (!en) {
    console.error(`✗ ${file}：没有英文正文（报告必须始终包含英语，SPEC §12）`)
    failed++
    continue
  }
  const hits = []
  for (const line of en.body.split(/\n|(?<=>)(?=<)/)) {
    if (CJK.test(line)) hits.push(line.trim().slice(0, 160))
    if (hits.length >= 5) break
  }
  if (hits.length) {
    console.error(`✗ ${file}：英文正文里有 CJK 字符`)
    for (const h of hits) console.error(`    ${h}`)
    failed++
  } else {
    console.log(`✓ ${file}：${docs.length} 种语言（${docs.map((d) => d.lang).join(', ')}），英文正文无 CJK`)
  }
}

if (failed) {
  console.error(`\n❌ 报告语言自检失败 ${failed} 份`)
  process.exit(1)
}
console.log('\n✅ 报告语言自检通过')
