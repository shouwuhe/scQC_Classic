import type { StepId } from '@/lib/types'

/**
 * 引擎的工序顺序（SPEC §8.7）。运行页按它画进度，环境 RNA 决策页按它画"这一次会跑哪几步"。
 *
 * 注意 `correction` 排在 `doublet` **之后**：ddqc 的阈值必须在未校正的原始计数上算，
 * 先校正会把质控指标本身带偏。所以"去污染"是输出前的最后一步，不是前置步骤。
 */
export const PIPELINE: StepId[] = [
  'read',
  'platform',
  'cell_calling',
  'cluster',
  'ambient',
  'ddqc',
  'doublet',
  'correction',
  'output',
]

/**
 * 本次运行实际会执行的工序。明确决定了不校正（`false`）时 `correction` 整步不存在；
 * 还没问用户（`null`）时先留着这一格，否则决定完会突然多出一行。
 */
export function pipelineFor(correct: boolean | null): StepId[] {
  return correct === false ? PIPELINE.filter((s) => s !== 'correction') : PIPELINE
}

/** `correction` 在完整工序里的序号（1 起）——文案里要写"第几步"。 */
export const CORRECTION_INDEX = PIPELINE.indexOf('correction') + 1
