import type { DataSource } from '../types'

const LABELS: Record<DataSource, string> = {
  live: '実測値',
  sample: 'サンプル',
  unavailable: '未実装',
}

/** パネル見出しなどに付ける「データの出どころ」表示 */
export function SourceTag({ source }: { source: DataSource }) {
  return <span className={`source-tag source-tag--${source}`}>{LABELS[source]}</span>
}
