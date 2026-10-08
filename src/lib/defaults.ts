import type { RunSettings } from './types'

/** 与引擎 `RunParams::default()` 保持一致（SPEC §16.1）。 */
export const DEFAULT_SETTINGS: RunSettings = {
  seed: 20260911,
  threads: 0,
  cellCalling: { niters: 10000, fdr: 0.01, lower: null },
  embed: { nHvg: 2000, nPcs: 50, k: 20, resFine: 1.3, resCoarse: 0.3 },
  ambient: { decontxMaxIter: 3000, soupExclFactor: 1.5 },
  ddqc: {
    madMultiplier: 2.0,
    fixedThresholdsAreHard: true,
    fixedMinGenes: 500,
    fixedMaxMito: 0.1,
  },
  doublet: { ratePer1000: null, nLoaded: null, removeBorderline: true },
  correction: { method: 'none', force: false },
  mitoGuardrail: null,
}

/** 界面设置 → 引擎 params JSON（字段名与 Rust 的 serde 对应）。 */
export function toEngineParams(s: RunSettings): Record<string, unknown> {
  return {
    seed: s.seed,
    threads: s.threads,
    cell_calling: { niters: s.cellCalling.niters, fdr: s.cellCalling.fdr, lower: s.cellCalling.lower },
    embed: {
      n_hvg: s.embed.nHvg,
      n_pcs: s.embed.nPcs,
      k: s.embed.k,
      res_fine: s.embed.resFine,
      res_coarse: s.embed.resCoarse,
    },
    ambient: {
      decontx_max_iter: s.ambient.decontxMaxIter,
      soup_excl_factor: s.ambient.soupExclFactor,
    },
    ddqc: {
      mad_multiplier: s.ddqc.madMultiplier,
      fixed_thresholds_are_hard: s.ddqc.fixedThresholdsAreHard,
      fixed_min_genes: s.ddqc.fixedMinGenes,
      fixed_max_mito: s.ddqc.fixedMaxMito,
    },
    doublet: {
      rate_per_1000: s.doublet.ratePer1000,
      n_loaded: s.doublet.nLoaded,
      remove_borderline: s.doublet.removeBorderline,
    },
    correction: { method: s.correction.method, force: s.correction.force },
  }
}
