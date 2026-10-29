import type { DataSource, PeerNode } from '../types'
import { Meter } from './Meter'
import { SourceTag } from './SourceTag'
import { getLatencyBetween } from './LatencyMatrix'

interface NodeDetailProps {
  node: PeerNode
  isLocal: boolean
  source: DataSource
  allNodes?: PeerNode[]
  localNodeId?: string
}

function Gauge({
  label,
  value,
  detail,
  source,
}: {
  label: string
  value: number | null
  detail: string
  source: DataSource
}) {
  return (
    <div className="gauge">
      <div className="gauge-head">
        <span className="gauge-label">{label}</span>
        <span className="gauge-value num">
          {value === null ? <span className="value-missing">未取得</span> : <>{value}<small>%</small></>}
        </span>
      </div>
      <Meter value={value} source={source} label={label} size="md" />
      <span className="gauge-detail">{detail}</span>
    </div>
  )
}

export function NodeDetail({ node, isLocal, source, allNodes, localNodeId }: NodeDetailProps) {
  const { surplus } = node
  const usedMemoryGb =
    node.memoryTotalGb !== null && node.memoryPercent !== null
      ? (node.memoryTotalGb * node.memoryPercent) / 100
      : null

  return (
    <>
      <header className="panel-head">
        <h2>選択中のPC</h2>
        <SourceTag source={source} />
      </header>

      <div className="detail-title">
        <span className={`status-dot status-dot--${node.status}`} aria-hidden="true" />
        <strong>{node.name}</strong>
        <span className="detail-role">{isLocal ? 'このPC' : 'ピア'}</span>
      </div>

      {/* 1. 現在の負荷（使用率メーター） */}
      <div className="detail-section-label">現在の負荷状況</div>
      <div className="gauges">
        <Gauge
          label="CPU使用率"
          value={node.cpuPercent}
          source={source}
          detail={node.cpuCores === null ? 'コア数 不明' : `${node.cpuCores} 論理コア`}
        />
        <Gauge
          label="メモリ使用率"
          value={node.memoryPercent}
          source={source}
          detail={
            node.memoryTotalGb === null
              ? '搭載量 不明'
              : usedMemoryGb === null
                ? `${node.memoryTotalGb} GB 搭載`
                : `${usedMemoryGb.toFixed(1)} / ${node.memoryTotalGb} GB 使用中`
          }
        />
      </div>

      {/* 2. 余剰計算資源（本システムの最重要指標） */}
      <section className="surplus-eval-card" aria-labelledby="surplus-card-heading">
        <header className="surplus-card-head">
          <h3 id="surplus-card-heading">余剰計算資源の評価結果</h3>
          <span className={`surplus-badge surplus-badge--${surplus.status}`}>
            {surplus.status === 'evaluated'
              ? '評価完了'
              : surplus.status === 'evaluating'
                ? '測定中'
                : surplus.status === 'insufficient_data'
                  ? '情報不足'
                  : '未実装'}
          </span>
        </header>

        <p className="surplus-note-head">
          ※ 単純な「100% − 使用率」ではなく、基礎性能と安全余裕から算出した実効割当枠です。
        </p>

        <div className="surplus-metrics-grid">
          <div className="surplus-metric-box">
            <span className="sm-label">実効割当可能CPU</span>
            <span className="sm-val num">
              {surplus.allocatableCpuScore !== null ? (
                <>
                  {surplus.allocatableCpuScore.toLocaleString()}
                  <small> pt</small>
                </>
              ) : (
                <span className="value-missing">—</span>
              )}
            </span>
            <span className="sm-sub">
              基準性能: {node.cpuBenchmarkScore !== null ? `${node.cpuBenchmarkScore.toLocaleString()} pt` : '未測定'}
            </span>
          </div>

          <div className="surplus-metric-box">
            <span className="sm-label">実効割当可能メモリ</span>
            <span className="sm-val num">
              {surplus.allocatableMemoryGb !== null ? (
                <>
                  {surplus.allocatableMemoryGb}
                  <small> GB</small>
                </>
              ) : (
                <span className="value-missing">—</span>
              )}
            </span>
            <span className="sm-sub">
              空き物理: {node.memoryAvailableGb !== null ? `${node.memoryAvailableGb} GB` : '未取得'}
            </span>
          </div>
        </div>

        <div className="surplus-meta-row">
          <span>信頼性スコア:</span>
          <strong>
            {surplus.confidence !== null ? `${Math.round(surplus.confidence * 100)}%` : '—'}
          </strong>
          {surplus.algorithmVersion && (
            <span className="algo-tag">モデル: {surplus.algorithmVersion}</span>
          )}
        </div>

        <p className="surplus-status-desc">{surplus.statusNote}</p>
      </section>

      {/* 3. ハードウェア・ネットワーク仕様 */}
      <dl className="props">
        <div>
          <dt>接続状態</dt>
          <dd>
            <span className={`status status--${node.status}`}>
              {node.status === 'online' ? 'オンライン' : 'オフライン'}
            </span>
          </dd>
        </div>
        <div>
          <dt>このPCとのPing</dt>
          <dd className="num">
            {isLocal ? '—（自身）' : node.latencyMs === null ? '未取得' : `${node.latencyMs} ms`}
          </dd>
        </div>
        <div>
          <dt>Tailscale IP</dt>
          <dd className="num">{node.address ?? '—'}</dd>
        </div>
        <div>
          <dt>OS</dt>
          <dd>{node.os ?? '不明'}</dd>
        </div>
      </dl>

      {/* 4. クラスタ内 他PCとのPing (相互応答時間) */}
      {allNodes && allNodes.length > 1 && (
        <section className="peer-latencies-section" aria-label="他PCとのPing">
          <div className="detail-section-label">他PCとのPing (往復応答時間)</div>
          <div className="peer-latencies-list">
            {allNodes
              .filter((other) => other.id !== node.id)
              .map((other) => {
                const { latencyMs, isOffline } = getLatencyBetween(node, other, localNodeId || '')
                return (
                  <div key={`lat-${other.id}`} className="peer-lat-item">
                    <span className="peer-lat-name">
                      <span className={`status-dot status-dot--${other.status}`} />
                      {other.name}
                      {other.id === localNodeId && <span className="local-tag">自PC</span>}
                    </span>
                    <span className={`peer-lat-val num ${isOffline ? 'is-offline' : latencyMs === null ? 'is-probing' : ''}`}>
                      {isOffline ? '切断' : latencyMs !== null ? `${latencyMs} ms` : '測定中…'}
                    </span>
                  </div>
                )
              })}
          </div>
        </section>
      )}
    </>
  )
}
