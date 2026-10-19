import type { KeyboardEvent } from 'react'
import type { PeerNode, NetworkLink } from '../types'
import { HIGH_LOAD_THRESHOLD } from '../lib/networkSummary'

interface TopologyProps {
  nodes: PeerNode[]
  links: NetworkLink[]
  localNodeId: string
  selectedId: string
  onSelect: (id: string) => void
}

const VIEW_W = 800
const VIEW_H = 330
const NODE_W = 184
const NODE_H = 56
const CENTER_X = 400
const CENTER_Y = 165
const RADIUS_X = 292
const RADIUS_Y = 112

interface PlacedNode {
  node: PeerNode
  x: number
  y: number
}

function layout(nodes: PeerNode[], localNodeId: string): Map<string, PlacedNode> {
  const ordered = [...nodes].sort((a, b) => Number(b.id === localNodeId) - Number(a.id === localNodeId))
  const step = (Math.PI * 2) / Math.max(ordered.length, 1)
  const map = new Map<string, PlacedNode>()

  ordered.forEach((node, i) => {
    const angle = Math.PI + i * step // このPCを左端から配置
    map.set(node.id, {
      node,
      x: CENTER_X + RADIUS_X * Math.cos(angle),
      y: CENTER_Y + RADIUS_Y * Math.sin(angle),
    })
  })

  return map
}

export function Topology({ nodes, links, localNodeId, selectedId, onSelect }: TopologyProps) {
  const nodeMap = layout(nodes, localNodeId)

  const handleKey = (event: KeyboardEvent, id: string) => {
    if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault()
      onSelect(id)
    }
  }

  return (
    <svg
      className="topology-svg"
      viewBox={`0 0 ${VIEW_W} ${VIEW_H}`}
      role="group"
      aria-label="PC間の接続トポロジー"
    >
      {/* 定義された接続関係（リンク）の描画 */}
      <g>
        {links.map((link) => {
          const a = nodeMap.get(link.sourceId)
          const b = nodeMap.get(link.targetId)
          if (!a || !b) return null

          const involvesLocal = a.node.id === localNodeId || b.node.id === localNodeId
          const isDisconnected = link.status === 'disconnected'
          const isSelected = a.node.id === selectedId || b.node.id === selectedId
          const cls = [
            'topo-link',
            involvesLocal ? 'topo-link--local' : 'topo-link--peer',
            isDisconnected ? 'topo-link--down' : '',
            isSelected && !isDisconnected ? 'topo-link--selected' : '',
          ]
            .filter(Boolean)
            .join(' ')

          return (
            <line
              key={`${link.sourceId}-${link.targetId}`}
              className={cls}
              x1={a.x}
              y1={a.y}
              x2={b.x}
              y2={b.y}
            />
          )
        })}
      </g>

      {/* リンク上の遅延測定値ラベル */}
      <g>
        {links
          .filter((link) => link.sourceId === localNodeId || link.targetId === localNodeId)
          .map((link) => {
            const a = nodeMap.get(link.sourceId)
            const b = nodeMap.get(link.targetId)
            if (!a || !b) return null

            const isDown = link.status === 'disconnected'
            const text = isDown ? '切断' : link.latencyMs === null ? '—' : `${link.latencyMs} ms`
            const mx = (a.x + b.x) / 2
            const my = (a.y + b.y) / 2
            const w = text.length * 7 + 12

            return (
              <g
                key={`label-${link.sourceId}-${link.targetId}`}
                className={`topo-latency ${isDown ? 'is-down' : ''}`}
              >
                <rect x={mx - w / 2} y={my - 9} width={w} height={18} rx={2} />
                <text x={mx} y={my + 4} textAnchor="middle">
                  {text}
                </text>
              </g>
            )
          })}
      </g>

      {/* 各PCノード */}
      {Array.from(nodeMap.values()).map(({ node, x, y }) => {
        const isLocal = node.id === localNodeId
        const isSelected = node.id === selectedId
        const online = node.status === 'online'
        const cls = [
          'topo-node',
          isLocal ? 'topo-node--local' : '',
          isSelected ? 'topo-node--selected' : '',
          online ? '' : 'topo-node--offline',
        ]
          .filter(Boolean)
          .join(' ')

        const cpuHigh = (node.cpuPercent ?? 0) >= HIGH_LOAD_THRESHOLD
        const surplusAvailable =
          node.surplus.status === 'evaluated' &&
          (node.surplus.allocatableCpuScore ?? 0) > 0

        return (
          <g
            key={node.id}
            className={cls}
            transform={`translate(${x - NODE_W / 2} ${y - NODE_H / 2})`}
            role="button"
            tabIndex={0}
            aria-pressed={isSelected}
            aria-label={`${node.name}${isLocal ? '（このPC）' : ''} ${online ? 'オンライン' : 'オフライン'}`}
            onClick={() => onSelect(node.id)}
            onKeyDown={(e) => handleKey(e, node.id)}
          >
            <rect className="topo-node-box" width={NODE_W} height={NODE_H} rx={3} />
            {isLocal && <rect className="topo-node-flag" width={3} height={NODE_H} />}
            <circle className={`topo-status topo-status--${node.status}`} cx={15} cy={19} r={3.5} />
            <text className="topo-node-name" x={26} y={23}>
              {node.name}
            </text>
            {isLocal && (
              <text className="topo-node-tag" x={NODE_W - 9} y={23} textAnchor="end">
                このPC
              </text>
            )}

            {/* 1行目：使用率実測 */}
            <text className="topo-node-meta" x={15} y={39}>
              {online ? (
                <>
                  <tspan className={cpuHigh ? 'is-high' : ''}>CPU {node.cpuPercent ?? '—'}%</tspan>
                  <tspan dx={8}>MEM {node.memoryPercent ?? '—'}%</tspan>
                </>
              ) : (
                `オフライン`
              )}
            </text>

            {/* 2行目：評価された余剰資源（最重要指標） */}
            <text className="topo-node-surplus" x={15} y={51}>
              {online ? (
                surplusAvailable ? (
                  <tspan className="surplus-ok">
                    余剰: CPU {node.surplus.allocatableCpuScore?.toLocaleString()}pt / {node.surplus.allocatableMemoryGb}GB
                  </tspan>
                ) : (
                  <tspan className="surplus-none">
                    余剰: {node.surplus.status === 'evaluated' ? '割当枠なし' : '未評価'}
                  </tspan>
                )
              ) : (
                <tspan className="surplus-none">評価不能</tspan>
              )}
            </text>
          </g>
        )
      })}
    </svg>
  )
}
