/**
 * データの出どころ区分
 * - live: Agentから実際に取得した実測データ
 * - sample: 画面確認・動作確認用の仮データ
 * - unavailable: 未取得または未実装
 */
export type DataSource = 'live' | 'sample' | 'unavailable'

export type NodeStatus = 'online' | 'offline'

/** 余剰計算資源の評価ステータス */
export type SurplusEvaluationStatus =
  | 'evaluated'         // 性能測定と現在負荷から評価完了
  | 'evaluating'        // ベンチマーク・測定中
  | 'insufficient_data' // 負荷または性能情報が不足（未計測・オフライン）
  | 'unsupported'       // 評価未対応・未実装

/**
 * 余剰計算資源の評価結果
 * ※ 単純な「100% - 使用率」ではなく、性能指標と安全余裕を考慮して算出される実効能力
 */
export interface SurplusCapacity {
  status: SurplusEvaluationStatus
  /** 信頼性スコア (0.0〜1.0) / 情報不足時は null */
  confidence: number | null
  /**
   * 割り当て可能と評価された実効CPU資源（正規化スコアまたは実効能力値）
   * 評価未完了時は null
   */
  allocatableCpuScore: number | null
  /**
   * 割り当て可能と評価されたメモリ容量 (GB)
   * OSや既存プロセスの安全マージンを除いた実質利用可能枠
   */
  allocatableMemoryGb: number | null
  /** 算出に用いた評価基準・モデル名（例: "linear_headroom_v1" など） */
  algorithmVersion: string | null
  /** 補足説明（未評価の理由や安全余裕の適用状況など） */
  statusNote: string
}

/** PC間の接続関係（トポロジーのエッジ） */
export interface NetworkLink {
  sourceId: string
  targetId: string
  latencyMs: number | null
  status: 'active' | 'degraded' | 'disconnected'
}

/** P2Pエージェントから配信されるピア追跡情報 */
export interface PeerNodeState {
  id: string
  name: string
  address: string
  os: string | null
  status: string
  latencyMs: number | null
  lastSeenSecsAgo: number | null
  cpuCores: number | null
  cpuPercent: number | null
  memoryPercent: number | null
  memoryTotalGb: number | null
  memoryAvailableGb: number | null
  cpuBenchmarkScore: number | null
  surplus: SurplusCapacity
  transport: string
}

/** SpareLink Agent が配置された1台のPC */
export interface PeerNode {
  id: string
  name: string
  status: NodeStatus
  address: string | null
  os: string | null

  // --- ハードウェア基礎仕様 ---
  cpuCores: number | null
  memoryTotalGb: number | null
  /** CPU性能指標（ローカルベンチマーク値 / 未測定時は null） */
  cpuBenchmarkScore: number | null

  // --- 現在の負荷実測値 ---
  cpuPercent: number | null
  memoryPercent: number | null
  memoryAvailableGb: number | null // OSから見た空き＋スタンバイ等の空き物理メモリ

  // --- 余剰計算資源（本システムの最重要指標） ---
  surplus: SurplusCapacity

  // --- ネットワーク ---
  latencyMs: number | null
  lastSeen: string | null
  dataSource: DataSource
}

export interface NetworkSnapshot {
  source: DataSource
  capturedAt: string | null
  localNodeId: string
  nodes: PeerNode[]
  links: NetworkLink[]
}

export interface EventLogEntry {
  time: string
  level: 'info' | 'warn'
  nodeName: string
  message: string
}

/** Tailscale 稼働ステータス */
export type TailscaleState =
  | 'running'
  | 'needs_login'
  | 'stopped'
  | 'not_installed'
  | 'error'

/** Tailscale 連携状態テレメトリ */
export interface TailscaleTelemetry {
  state: TailscaleState
  selfIp: string | null
  selfHostname: string | null
  authUrl: string | null
  message: string
}
