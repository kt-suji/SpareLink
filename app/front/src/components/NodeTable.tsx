import type { DataSource, PeerNode } from '../types'
import { Meter } from './Meter'

interface NodeTableProps {
  nodes: PeerNode[]
  localNodeId: string
  selectedId: string
  source: DataSource
  onSelect: (id: string) => void
}

function UsageCell({ value, source, label }: { value: number | null; source: DataSource; label: string }) {
  if (value === null) return <span className="value-missing">未取得</span>
  return (
    <span className="usage-cell">
      <Meter value={value} source={source} label={label} />
      <span className="num">{value}%</span>
    </span>
  )
}

function SurplusCell({ node }: { node: PeerNode }) {
  const { surplus } = node
  if (surplus.status === 'insufficient_data') {
    return <span className="value-missing">評価不能</span>
  }
  if (surplus.status === 'unsupported') {
    return <span className="value-missing">未実装</span>
  }
  if (surplus.status === 'evaluating') {
    return <span className="value-muted">測定中…</span>
  }

  const hasCapacity = (surplus.allocatableCpuScore ?? 0) > 0

  return (
    <span className={`surplus-summary-cell ${hasCapacity ? 'has-capacity' : 'no-capacity'}`}>
      <span className="num font-bold">
        {(surplus.allocatableCpuScore ?? 0).toLocaleString()}
        <small className="unit"> pt</small>
      </span>
      <span className="surplus-divider">/</span>
      <span className="num">
        {surplus.allocatableMemoryGb ?? 0}
        <small className="unit"> GB</small>
      </span>
    </span>
  )
}

export function NodeTable({ nodes, localNodeId, selectedId, source, onSelect }: NodeTableProps) {
  return (
    <div className="table-scroll">
      <table className="node-table">
        <thead>
          <tr>
            <th scope="col">PC名</th>
            <th scope="col">状態</th>
            <th scope="col">CPU使用率</th>
            <th scope="col">メモリ使用率</th>
            <th scope="col" className="col-surplus">
              余剰計算資源 (割当可能CPU / メモリ)
            </th>
            <th scope="col" className="col-num">応答時間</th>
            <th scope="col" className="col-optional">性能スコア</th>
          </tr>
        </thead>
        <tbody>
          {nodes.map((node) => {
            const isLocal = node.id === localNodeId
            const isSelected = node.id === selectedId
            return (
              <tr
                key={node.id}
                className={isSelected ? 'is-selected' : ''}
                onClick={() => onSelect(node.id)}
              >
                <th scope="row">
                  <button type="button" className="row-select" aria-pressed={isSelected}>
                    {node.name}
                  </button>
                  {isLocal && <span className="local-tag">このPC</span>}
                </th>
                <td>
                  <span className={`status status--${node.status}`}>
                    {node.status === 'online' ? 'オンライン' : 'オフライン'}
                  </span>
                </td>
                <td>
                  <UsageCell value={node.cpuPercent} source={source} label={`${node.name} のCPU使用率`} />
                </td>
                <td>
                  <UsageCell value={node.memoryPercent} source={source} label={`${node.name} のメモリ使用率`} />
                </td>
                <td className="col-surplus">
                  <SurplusCell node={node} />
                </td>
                <td className="col-num num">
                  {isLocal ? (
                    <span className="value-muted">自身</span>
                  ) : node.latencyMs === null ? (
                    <span className="value-missing">—</span>
                  ) : (
                    `${node.latencyMs} ms`
                  )}
                </td>
                <td className="col-optional num value-muted">
                  {node.cpuBenchmarkScore !== null ? `${node.cpuBenchmarkScore.toLocaleString()} pt` : '未測定'}
                </td>
              </tr>
            )
          })}
        </tbody>
      </table>
    </div>
  )
}
