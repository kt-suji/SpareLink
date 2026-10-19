import type { PeerNode } from '../types'

/** この値以上を「高負荷」として強調表示する（%） */
export const HIGH_LOAD_THRESHOLD = 80

function average(values: number[]): number | null {
  if (values.length === 0) return null
  return Math.round(values.reduce((sum, v) => sum + v, 0) / values.length)
}

function numbers(values: (number | null)[]): number[] {
  return values.filter((v): v is number => v !== null)
}

/** ノード一覧から概要値を算出する。 */
export function summarize(nodes: PeerNode[]) {
  const online = nodes.filter((n) => n.status === 'online')

  // CPU使用率の加重平均（論理コア数を考慮）
  let totalCores = 0
  let weightedCpuSum = 0
  for (const n of online) {
    if (n.cpuPercent !== null && n.cpuCores !== null) {
      totalCores += n.cpuCores
      weightedCpuSum += n.cpuPercent * n.cpuCores
    }
  }
  const weightedAvgCpu = totalCores > 0 ? Math.round(weightedCpuSum / totalCores) : null

  // 評価済みの総余剰計算資源の集計
  let totalAllocatableCpuScore = 0
  let totalAllocatableMemoryGb = 0
  let evaluatedNodeCount = 0

  for (const n of online) {
    if (n.surplus.status === 'evaluated') {
      evaluatedNodeCount += 1
      if (n.surplus.allocatableCpuScore !== null) {
        totalAllocatableCpuScore += n.surplus.allocatableCpuScore
      }
      if (n.surplus.allocatableMemoryGb !== null) {
        totalAllocatableMemoryGb += n.surplus.allocatableMemoryGb
      }
    }
  }

  return {
    total: nodes.length,
    online: online.length,
    avgLatencyMs: average(numbers(online.map((n) => n.latencyMs))),
    avgCpuPercent: weightedAvgCpu ?? average(numbers(online.map((n) => n.cpuPercent))),
    avgMemoryPercent: average(numbers(online.map((n) => n.memoryPercent))),
    highLoadCount: online.filter((n) => (n.cpuPercent ?? 0) >= HIGH_LOAD_THRESHOLD).length,
    // 余剰計算資源
    totalAllocatableCpuScore,
    totalAllocatableMemoryGb: Math.round(totalAllocatableMemoryGb * 10) / 10,
    evaluatedNodeCount,
  }
}
