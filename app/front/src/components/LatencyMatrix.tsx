import type { PeerNode } from '../types'

interface LatencyMatrixProps {
  nodes: PeerNode[]
  localNodeId: string
  selectedId: string
  onSelect: (id: string) => void
}

/**
 * 2つのノード間の実測Ping遅延（ms）を取得する
 */
export function getLatencyBetween(
  a: PeerNode,
  b: PeerNode,
  localNodeId: string
): { latencyMs: number | null; isOffline: boolean } {
  if (a.id === b.id) {
    return { latencyMs: 0, isOffline: false }
  }

  const isOffline = a.status !== 'online' || b.status !== 'online'

  // 自PCとの通信の場合
  if (a.id === localNodeId) {
    return { latencyMs: isOffline ? null : b.latencyMs, isOffline }
  }
  if (b.id === localNodeId) {
    return { latencyMs: isOffline ? null : a.latencyMs, isOffline }
  }

  // ピアPC同士の場合：aから見たb、またはbから見たaの遅延
  const latAtoB =
    a.peerLatencies?.[b.id] ??
    a.peerLatencies?.[b.name.toLowerCase()] ??
    (b.address ? a.peerLatencies?.[b.address] : undefined)

  const latBtoA =
    b.peerLatencies?.[a.id] ??
    b.peerLatencies?.[a.name.toLowerCase()] ??
    (a.address ? b.peerLatencies?.[a.address] : undefined)

  const latencyMs = latAtoB ?? latBtoA ?? null
  return { latencyMs: isOffline ? null : latencyMs, isOffline }
}

export function LatencyMatrix({ nodes, localNodeId, selectedId, onSelect }: LatencyMatrixProps) {
  // 自PCを先頭にソート
  const sortedNodes = [...nodes].sort(
    (a, b) => Number(b.id === localNodeId) - Number(a.id === localNodeId)
  )

  const getLatencyBadgeClass = (ms: number | null, isOffline: boolean) => {
    if (isOffline) return 'matrix-cell--offline'
    if (ms === null) return 'matrix-cell--unknown'
    if (ms <= 10) return 'matrix-cell--fast'
    if (ms <= 40) return 'matrix-cell--good'
    if (ms <= 100) return 'matrix-cell--fair'
    return 'matrix-cell--slow'
  }

  return (
    <div className="latency-matrix-card">
      <div className="matrix-toolbar">
        <div>
          <h3 className="matrix-title">相互Ping遅延マトリクス (Full Mesh Latency)</h3>
          <p className="matrix-desc">
            クラスタ内の全PC間（自PCおよび他PC同士）のリアルタイム往復応答時間（Ping）の一覧です。
          </p>
        </div>
        <div className="matrix-legend">
          <span className="legend-chip legend-chip--fast">&le;10ms (超高速)</span>
          <span className="legend-chip legend-chip--good">&le;40ms (快適)</span>
          <span className="legend-chip legend-chip--fair">&le;100ms (通常)</span>
          <span className="legend-chip legend-chip--slow">&gt;100ms (高遅延)</span>
        </div>
      </div>

      <div className="matrix-table-scroll">
        <table className="matrix-table">
          <thead>
            <tr>
              <th className="matrix-corner">送信元 ＼ 宛先</th>
              {sortedNodes.map((target) => (
                <th
                  key={`col-${target.id}`}
                  className={`matrix-col-header ${target.id === selectedId ? 'is-selected' : ''}`}
                  onClick={() => onSelect(target.id)}
                  title={`${target.name} (${target.id === localNodeId ? 'このPC' : 'ピア'})`}
                >
                  <span className={`status-dot status-dot--${target.status}`} />
                  <span className="matrix-node-name">{target.name}</span>
                  {target.id === localNodeId && <span className="matrix-local-badge">自PC</span>}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {sortedNodes.map((source) => {
              const isSourceSelected = source.id === selectedId
              return (
                <tr key={`row-${source.id}`} className={isSourceSelected ? 'is-selected-row' : ''}>
                  <th
                    className={`matrix-row-header ${isSourceSelected ? 'is-selected' : ''}`}
                    onClick={() => onSelect(source.id)}
                  >
                    <span className={`status-dot status-dot--${source.status}`} />
                    <span className="matrix-node-name">{source.name}</span>
                    {source.id === localNodeId && <span className="matrix-local-badge">自PC</span>}
                  </th>
                  {sortedNodes.map((target) => {
                    const isSelf = source.id === target.id
                    if (isSelf) {
                      return (
                        <td key={`cell-${source.id}-${target.id}`} className="matrix-cell matrix-cell--self">
                          <span className="matrix-self-label">— (自身)</span>
                        </td>
                      )
                    }

                    const { latencyMs, isOffline } = getLatencyBetween(source, target, localNodeId)
                    const cellCls = getLatencyBadgeClass(latencyMs, isOffline)
                    const isCellSelected = source.id === selectedId || target.id === selectedId

                    return (
                      <td
                        key={`cell-${source.id}-${target.id}`}
                        className={`matrix-cell ${cellCls} ${isCellSelected ? 'is-cell-highlight' : ''}`}
                        title={`${source.name} ➔ ${target.name}: ${isOffline ? 'オフライン' : latencyMs !== null ? `${latencyMs} ms` : '計測中'}`}
                      >
                        {isOffline ? (
                          <span className="matrix-val matrix-val--down">切断</span>
                        ) : latencyMs === null ? (
                          <span className="matrix-val matrix-val--probing">測定中…</span>
                        ) : (
                          <span className="matrix-val num">
                            <strong>{latencyMs}</strong>
                            <small className="unit"> ms</small>
                          </span>
                        )}
                      </td>
                    )
                  })}
                </tr>
              )
            })}
          </tbody>
        </table>
      </div>
    </div>
  )
}
