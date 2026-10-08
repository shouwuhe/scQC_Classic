import { useCallback, useEffect, useRef, useState } from 'react'
import { AppShell, type ScreenId } from '@/components/AppShell'
import { ImportScreen } from '@/screens/Import'
import { RunningScreen } from '@/screens/Running'
import { OverviewScreen } from '@/screens/Overview'
import { TuningScreen, settingsFromReport } from '@/screens/Tuning'
import { ExportScreen } from '@/screens/Export'
import { LangContext, initialLang, type Lang } from '@/i18n'
import { host } from '@/lib/api'
import { DEFAULT_SETTINGS, toEngineParams } from '@/lib/defaults'
import type {
  AmbientFacts,
  EngineEvent,
  ProgressEvent,
  ReportData,
  RunSettings,
  SampleJob,
  WarningEvent,
} from '@/lib/types'

/**
 * 应用状态机：输入 → 运行 → 查看 → 输出，外加"调参后重跑"。
 *
 * 单样本与批量走同一条路径：`jobs` 永远是一个数组，长度为 1 时界面自动退化成
 * 原来的单样本样子。重跑一律写到**新目录**（硬约束 C7）。
 */
/** 同时停着等决定的进程上限：再多就退回"只问第一个、答复套用全批"（每个停着的进程都占内存）。 */
const MAX_PARKED = 8

type Answer = { method: 'none' | 'soup' | 'decontx'; force: boolean }
type RunResult = { ok: boolean; code?: number; stderr?: string; error?: string }

/**
 * 把一批样本的污染水平合成一份给用户看：各估计量取均值，判定取最保守的那一档，
 * 受影响基因按平均占比排序。批次通常来自同一次实验，这样看整体比看某一个样本可靠。
 */
function averageFacts(list: AmbientFacts[]): AmbientFacts {
  const mean = (pick: (f: AmbientFacts) => number | null) => {
    const v = list.map(pick).filter((x): x is number => x != null && Number.isFinite(x))
    return v.length ? v.reduce((a, b) => a + b, 0) / v.length : null
  }
  const rank: Record<string, number> = { discouraged: 3, greyed: 2, unrated: 1, available: 0 }
  const state = list
    .map((f) => f.ambient_correction_state)
    .reduce((a, b) => ((rank[b] ?? 0) > (rank[a] ?? 0) ? b : a), 'available' as AmbientFacts['ambient_correction_state'])
  const genes = new Map<string, number[]>()
  for (const f of list) for (const g of f.ambient_top_genes) genes.set(g.gene, [...(genes.get(g.gene) ?? []), g.fraction])
  return {
    ambient_available: true,
    rho_soup: mean((f) => f.rho_soup),
    rho_soup_unreliable: list.some((f) => f.rho_soup_unreliable),
    rho_decontx: mean((f) => f.rho_decontx),
    rho_mito: mean((f) => f.rho_mito),
    ambient_verdict: list.some((f) => f.ambient_verdict === 'disagree') ? 'disagree' : (list[0]?.ambient_verdict ?? 'agree'),
    ambient_rating: list.some((f) => f.ambient_rating === 'severe') ? 'severe' : (list[0]?.ambient_rating ?? 'normal'),
    ambient_correction_state: state,
    ambient_reference_mean: mean((f) => f.ambient_reference_mean),
    ambient_reference_grey_below: mean((f) => f.ambient_reference_grey_below),
    ambient_reference_discourage_above: mean((f) => f.ambient_reference_discourage_above),
    ambient_top_genes: [...genes.entries()]
      .map(([gene, v]) => ({ gene, fraction: v.reduce((a, b) => a + b, 0) / v.length }))
      .sort((a, b) => b.fraction - a.fraction)
      .slice(0, 12),
  }
}

export default function App() {
  const [lang, setLangState] = useState<Lang>(initialLang)
  const setLang = useCallback((l: Lang) => {
    setLangState(l)
    try {
      localStorage.setItem('scqc.lang', l)
    } catch {
      /* 隐私模式下可能不可写，忽略 */
    }
  }, [])

  const [screen, setScreen] = useState<ScreenId>('import')
  const [jobs, setJobs] = useState<SampleJob[]>([])
  const [baseOut, setBaseOut] = useState<string | null>(null)
  /** 输出目录该建在哪里、叫什么（由扫描结果给出，重跑时要用同一套） */
  const [outHint, setOutHint] = useState<{ parent: string; label: string } | null>(null)
  const [settings, setSettings] = useState<RunSettings>(DEFAULT_SETTINGS)
  /** 产生当前报告的那次运行所用的设置（调参页据此判断"改没改"） */
  const [runSettings, setRunSettings] = useState<RunSettings>(DEFAULT_SETTINGS)
  const [progress, setProgress] = useState<ProgressEvent | null>(null)
  const [warnings, setWarnings] = useState<WarningEvent[]>([])
  const [error, setError] = useState<string | null>(null)
  const [active, setActive] = useState(0)
  const [runIdx, setRunIdx] = useState(0)
  const [tuning, setTuning] = useState(false)
  /** 引擎跑完第 1–7 步、停下来等"要不要校正"时带过来的事实；null = 没在等 */
  const [awaiting, setAwaiting] = useState<AmbientFacts | null>(null)
  /** 这一次跑不跑第 8 步；null = 还没问 */
  const [correctDecision, setCorrectDecision] = useState<boolean | null>(null)
  const runSeq = useRef(0)
  /** 最近一次引擎 error 事件的 code（进程失败时拿它当原因，而不是 stderr 原文） */
  const lastErrCode = useRef<string | null>(null)
  /** 批量时只问一次：第一个样本停下来问，答复原样应用到这一批其余样本 */
  const batchAnswer = useRef<Answer | null>(null)
  // 经函数读取：直接读 .current 会被 TS 按上面那句赋 null 收窄成 never
  const answerSoFar = useCallback(() => batchAnswer.current, [])
  /** 停在第 7 步的各样本报上来的污染水平 */
  const pausedFacts = useRef<AmbientFacts[]>([])
  const pauseWaiter = useRef<(() => void) | null>(null)
  const answerWaiter = useRef<((a: Answer) => void) | null>(null)

  useEffect(() => {
    return host().onEvent((e: EngineEvent) => {
      if (e.type === 'progress') setProgress(e)
      else if (e.type === 'await_correction') {
        pausedFacts.current = [...pausedFacts.current, e.facts]
        pauseWaiter.current?.()
      }
      else if (e.type === 'warning') setWarnings((w) => [...w, e])
      // 只记 code：引擎的 message 是中文（给命令行和 stderr 日志用的）
      else if (e.type === 'error') {
        lastErrCode.current = e.code
        setError(e.code)
      }
    })
  }, [])

  const start = useCallback(
    async (s: RunSettings, tag?: string, back: ScreenId = 'review', ask = false) => {
      const h = host()
      const included = jobs.filter((j) => j.include)
      const first = included[0]
      if (!first) return
      const seq = ++runSeq.current
      setProgress(null)
      setWarnings([])
      setError(null)
      setTuning(false)
      setRunIdx(0)
      setAwaiting(null)
      pausedFacts.current = []
      pauseWaiter.current = null
      answerWaiter.current = null
      // 样本数超过这个上限就不再让它们全部停着等 —— 每个停着的进程都占着自己的中间态内存
      const park = ask && included.length <= MAX_PARKED
      const askFirstOnly = ask && !park
      setCorrectDecision(ask ? null : s.correction.method !== 'none')
      setScreen('run')

      let base = baseOut
      if (!base || tag) {
        const hint = outHint ?? { parent: first.filteredDir, label: first.name }
        base = await h.suggestOutput(hint.parent, hint.label, tag)
      }
      setBaseOut(base)

      let work: SampleJob[] = jobs.map((j) =>
        j.include ? { ...j, status: 'pending', report: null, error: null, outDir: null } : j,
      )
      setJobs(work)

      const dirs: string[] = []
      for (let i = 0; i < included.length; i++) {
        dirs.push(included.length > 1 ? await h.subOutput(base, included[i]!.name) : base)
      }

      const launch = (i: number, awaitCorrection: boolean) => {
        const job = included[i]!
        lastErrCode.current = null
        return h.run({
          rawDir: job.rawDir,
          filteredDir: job.filteredDir,
          metricsCsv: job.metricsCsv,
          platform: job.platform,
          species: job.species,
          multiplexN: job.multiplexN,
          mitoGuardrail: s.mitoGuardrail,
          correct: answerSoFar()?.method ?? s.correction.method,
          forceCorrect: answerSoFar()?.force ?? s.correction.force,
          awaitCorrection,
          runId: job.id,
          outputDir: dirs[i]!,
          params: toEngineParams(s),
          reportLang: lang,
        })
      }

      const collect = async (i: number, r: RunResult) => {
        const job = included[i]!
        if (!r.ok) {
          work = work.map((x) =>
            x.id === job.id ? { ...x, status: 'error', error: lastErrCode.current ?? 'RUN_FAILED' } : x,
          )
          setJobs(work)
          return
        }
        const data = (await h.readReport(dirs[i]!)) as ReportData
        work = work.map((x) => (x.id === job.id ? { ...x, status: 'done', report: data } : x))
        setJobs(work)
        if (i === 0) {
          // 引擎可能按平台补过参数（例如线粒体护栏），以报告为准再叠加本次实际用的设置
          const used: RunSettings = { ...settingsFromReport(data), ...s }
          setRunSettings(used)
          setSettings(used)
        }
      }

      const markRunning = (i: number) => {
        setRunIdx(i)
        setProgress(null)
        setWarnings([])
        work = work.map((x) => (x.id === included[i]!.id ? { ...x, status: 'running', outDir: dirs[i]! } : x))
        setJobs(work)
      }

      if (park) {
        // 第一阶段：逐个起进程，各自跑完第 1–7 步后停在那里等决定。
        // 同一时刻只有一个在算（起下一个之前先等它停下），其余只是占着内存。
        const pending: Promise<RunResult>[] = []
        for (let i = 0; i < included.length; i++) {
          if (seq !== runSeq.current) return
          markRunning(i)
          const promise = launch(i, true)
          pending.push(promise)
          // 停下来（发了 await_correction）或直接结束（没有环境表达谱，引擎不问）都算这一个走完
          await Promise.race([
            new Promise<void>((resolve) => {
              pauseWaiter.current = resolve
            }),
            promise.then(() => undefined),
          ])
          pauseWaiter.current = null
        }
        if (seq !== runSeq.current) return

        // 凑齐了：把这一批的污染水平合到一起问一次
        const facts = pausedFacts.current
        if (facts.length > 0) {
          setAwaiting(averageFacts(facts))
          const ans = await new Promise<Answer>((resolve) => {
            answerWaiter.current = resolve
          })
          answerWaiter.current = null
          if (seq !== runSeq.current) return
          await h.answerCorrection(ans.method, ans.force)
        }

        // 第二阶段：各进程跑完第 8、9 步
        for (let i = 0; i < included.length; i++) {
          const r = await pending[i]!
          if (seq !== runSeq.current) return
          await collect(i, r)
        }
      } else {
        for (let i = 0; i < included.length; i++) {
          if (seq !== runSeq.current) return
          markRunning(i)
          const r = await launch(i, askFirstOnly && i === 0)
          if (seq !== runSeq.current) return
          await collect(i, r)
        }
      }

      const firstDone = work.findIndex((x) => x.status === 'done')
      if (firstDone >= 0) {
        setActive(firstDone)
        setScreen(back)
      } else {
        setError(work.find((x) => x.error)?.error ?? 'run_failed')
      }
    },
    [jobs, baseOut, outHint, lang, answerSoFar],
  )

  const cancel = useCallback(() => {
    runSeq.current++
    void host().cancel()
    setScreen('import')
  }, [])

  const restart = useCallback(() => {
    runSeq.current++
    setJobs([])
    setBaseOut(null)
    setSettings(DEFAULT_SETTINGS)
    setRunSettings(DEFAULT_SETTINGS)
    setActive(0)
    setScreen('import')
  }, [])

  const done = jobs.filter((j) => j.status === 'done')
  const activeJob = jobs[active] ?? done[0] ?? null
  const report = activeJob?.report ?? null

  const canJump = useCallback(
    (s: ScreenId) => {
      if (s === 'import') return screen !== 'run'
      if (s === 'run') return false
      if (s === 'review') return done.length > 0
      if (s === 'export') return done.length > 0
      return false
    },
    [screen, done.length],
  )

  return (
    <LangContext.Provider value={{ lang, setLang }}>
      <AppShell
        screen={tuning ? 'review' : screen}
        canJump={canJump}
        onJump={(s) => {
          setTuning(false)
          setScreen(s)
        }}
      >
        {screen === 'import' && (
          <ImportScreen
            jobs={jobs}
            setJobs={setJobs}
            baseOut={baseOut}
            setBaseOut={setBaseOut}
            setOutHint={setOutHint}
            onStart={() => void start(settings, undefined, 'review', true)}
          />
        )}
        {screen === 'run' && (
          <RunningScreen
            progress={progress}
            warnings={warnings}
            error={error}
            correct={correctDecision}
            awaiting={awaiting}
            onAnswerCorrection={(method, force) => {
              batchAnswer.current = { method, force }
              setAwaiting(null)
              setCorrectDecision(method !== 'none')
              setSettings((s) => ({ ...s, correction: { method, force } }))
              // 停着等的那一批由 start() 统一回答；单样本在这里直接回答
              if (answerWaiter.current) answerWaiter.current({ method, force })
              else void host().answerCorrection(method, force)
            }}
            jobs={jobs.filter((j) => j.include)}
            current={runIdx}
            onCancel={cancel}
          />
        )}
        {screen === 'review' &&
          report &&
          activeJob &&
          (tuning ? (
            <TuningScreen
              report={report}
              settings={settings}
              baseline={runSettings}
              setSettings={setSettings}
              onBack={() => setTuning(false)}
              onRerun={() => void start(settings, `r${runSeq.current + 1}`)}
            />
          ) : (
            <OverviewScreen
              report={report}
              outDir={activeJob.outDir}
              jobs={jobs}
              activeId={activeJob.id}
              onSelect={(id) => setActive(jobs.findIndex((j) => j.id === id))}
              onTune={() => setTuning(true)}
              onNext={() => setScreen('export')}
            />
          ))}
        {screen === 'export' && report && activeJob?.outDir && (
          <ExportScreen
            report={report}
            outDir={activeJob.outDir}
            jobs={jobs}
            activeId={activeJob.id}
            onSelect={(id) => setActive(jobs.findIndex((j) => j.id === id))}
            onBack={() => setScreen('review')}
            onRestart={restart}
          />
        )}
      </AppShell>
    </LangContext.Provider>
  )
}
