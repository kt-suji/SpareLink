import { useState, useCallback, useEffect, useRef } from 'react'
import type {
  NetworkLink,
  NetworkSnapshot,
  NodeStatus,
  PeerNode,
  PeerNodeState,
  SurplusCapacity,
  TailscaleTelemetry,
} from '../types'
import { sampleSnapshot } from '../data/sampleData'

/** Agent(Rust/Python)から送信されるリアルタイムシステム情報・余剰評価 */
export interface SystemMetricsMessage {
  pc_name: string
  os: string
  cpu_cores: number
  cpu_percent: number
  memory_percent: number
  memory_total_gb: number
  memory_available_gb: number
  cpu_benchmark_score?: number | null
  surplus?: SurplusCapacity
  peers?: PeerNodeState[]
  tailscale?: TailscaleTelemetry | null
}

/**
 * ネットワークスナップショットを管理し、Local Agent (Rust/Python) との
 * WebSocket 接続経由でリアルタイム実測値およびP2Pピア状態を反映するカスタムフック。
 */
export function useNetworkSnapshot() {
  const [snapshot, setSnapshot] = useState<NetworkSnapshot>(sampleSnapshot)
  const [agentConnected, setAgentConnected] = useState<boolean>(false)
  const [tailscale, setTailscale] = useState<TailscaleTelemetry | null>(null)

  /**
   * Agentからの実測値を受信した際、自PCおよび全ピアの情報を更新する
   */
  const updateMetrics = useCallback((metrics: SystemMetricsMessage) => {
    if (metrics.tailscale !== undefined) {
      setTailscale(metrics.tailscale)
    }

    setSnapshot((prev) => {
      // 1. 自PC（Local Node）の実測反映
      const localNode: PeerNode = {
        id: prev.localNodeId,
        name: metrics.pc_name || '自PC (Local Node)',
        status: 'online',
        address: '127.0.0.1',
        os: metrics.os || null,
        cpuCores: metrics.cpu_cores,
        cpuPercent: metrics.cpu_percent,
        memoryPercent: metrics.memory_percent,
        memoryTotalGb: metrics.memory_total_gb,
        memoryAvailableGb: metrics.memory_available_gb,
        cpuBenchmarkScore:
          metrics.cpu_benchmark_score !== undefined
            ? metrics.cpu_benchmark_score
            : null,
        surplus: metrics.surplus ?? {
          status: 'evaluated',
          confidence: 0.9,
          allocatableCpuScore: null,
          allocatableMemoryGb: null,
          algorithmVersion: 'prototype_v1',
          statusNote: '稼働中',
        },
        latencyMs: 0,
        lastSeen: 'リアルタイム',
        dataSource: 'live',
      }

      // 2. ピア一覧の反映 (Tailscale / WireGuard 等で検出されたピア)
      let peerNodes: PeerNode[] = []
      let links: NetworkLink[] = []

      if (metrics.peers && metrics.peers.length > 0) {
        peerNodes = metrics.peers.map((p) => ({
          id: p.id,
          name: p.name,
          status: (p.status === 'online' ? 'online' : 'offline') as NodeStatus,
          address: p.address,
          os: p.os ?? null,
          cpuCores: p.cpuCores ?? null,
          memoryTotalGb: p.memoryTotalGb ?? null,
          cpuBenchmarkScore: p.cpuBenchmarkScore ?? null,
          cpuPercent: p.cpuPercent ?? null,
          memoryPercent: p.memoryPercent ?? null,
          memoryAvailableGb: p.memoryAvailableGb ?? null,
          surplus: p.surplus,
          latencyMs: p.latencyMs ?? null,
          lastSeen: p.lastSeenSecsAgo != null ? `${p.lastSeenSecsAgo}秒前` : (p.status === 'online' ? 'リアルタイム' : '未疎通'),
          dataSource: 'live' as const,
        }))

        // 自PCと各ピア間のリンクを動的に生成
        links = peerNodes.map((p) => ({
          sourceId: prev.localNodeId,
          targetId: p.id,
          latencyMs: p.latencyMs ?? null,
          status: p.status === 'online' ? ('active' as const) : ('disconnected' as const),
        }))
      } else {
        // ピアが未検出の場合は初期サンプル構成のノード（自PC以外）を維持
        peerNodes = prev.nodes.filter((node) => node.id !== prev.localNodeId)
        links = prev.links
      }

      return {
        ...prev,
        source: 'live',
        capturedAt: new Date().toLocaleTimeString('ja-JP'),
        nodes: [localNode, ...peerNodes],
        links: links.length > 0 ? links : prev.links,
      }
    })
  }, [])

  const updateMetricsRef = useRef(updateMetrics)
  useEffect(() => {
    updateMetricsRef.current = updateMetrics
  }, [updateMetrics])

  // Agent (ws://<host>:3001) への自動接続・再接続ライフサイクル
  useEffect(() => {
    let ws: WebSocket | null = null
    let reconnectTimeout: ReturnType<typeof setTimeout> | null = null
    let active = true

    const connect = () => {
      if (!active) return

      const host = window.location.hostname || 'localhost'
      const wsUrl = `ws://${host}:3001`
      console.log('[SpareLink] WebSocket接続試行:', wsUrl)

      try {
        ws = new WebSocket(wsUrl)

        ws.onopen = () => {
          if (!active) {
            ws?.close()
            return
          }
          console.log('[SpareLink] WebSocket接続確立!')
          setAgentConnected(true)
        }

        ws.onmessage = (event) => {
          if (!active) return
          try {
            const data: SystemMetricsMessage = JSON.parse(event.data)
            updateMetricsRef.current(data)
          } catch (err) {
            console.error('[SpareLink] WebSocketメッセージ解析エラー:', err)
          }
        }

        ws.onclose = (event) => {
          if (!active) return
          console.log('[SpareLink] WebSocket切断:', event.code, event.reason)
          setAgentConnected(false)
          reconnectTimeout = setTimeout(connect, 2000)
        }

        ws.onerror = (err) => {
          console.warn('[SpareLink] WebSocket通信エラー:', err)
        }
      } catch (err) {
        console.error('[SpareLink] WebSocket初期化エラー:', err)
        if (active) {
          reconnectTimeout = setTimeout(connect, 2000)
        }
      }
    }

    connect()

    return () => {
      active = false
      if (reconnectTimeout) clearTimeout(reconnectTimeout)
      if (ws) {
        ws.onopen = null
        ws.onmessage = null
        ws.onclose = null
        ws.onerror = null
        ws.close()
      }
    }
  }, [])

  return {
    snapshot,
    agentConnected,
    tailscale,
    setSnapshot,
  }
}
