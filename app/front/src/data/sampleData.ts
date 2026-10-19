import type { EventLogEntry, NetworkSnapshot } from '../types'

/*
 * 画面デザイン・動作確認用のサンプルデータ。
 * 実際のPC・Agent・Tailscale とは連携していない仮のデータ。
 * 
 * 方針:
 * - 単純な「100% - 使用率」を余剰計算資源としない。
 * - 性能差（cpuBenchmarkScore）とメモリ容量、安全マージンを考慮した仮の評価結果を提示。
 * - 一部ノードは「未評価」「情報不足」とし、実世界での多様な状態を再現。
 */
export const sampleSnapshot: NetworkSnapshot = {
  source: 'sample',
  capturedAt: null,
  localNodeId: 'desktop-main',
  nodes: [
    {
      id: 'desktop-main',
      name: 'DESKTOP-MAIN',
      status: 'online',
      address: '100.84.26.42',
      os: 'Windows 11 Pro',
      cpuCores: 16,
      memoryTotalGb: 32,
      cpuBenchmarkScore: 920, // 基準性能スコア
      cpuPercent: 18,
      memoryPercent: 41,
      memoryAvailableGb: 18.8,
      surplus: {
        status: 'evaluated',
        confidence: 0.92,
        allocatableCpuScore: 570, // 性能と安全余裕から算出した実効割当可能値
        allocatableMemoryGb: 12.0, // OS安定動作のためのマージンを除いた実利用可能枠
        algorithmVersion: 'prototype_v1',
        statusNote: '通常稼働中。安全マージン（CPU 20%, メモリ 6GB）を確保して評価',
      },
      latencyMs: null,
      lastSeen: null,
      dataSource: 'sample',
    },
    {
      id: 'workstation-02',
      name: 'WORKSTATION-02',
      status: 'online',
      address: '100.64.0.2',
      os: 'Windows 11 Pro',
      cpuCores: 24,
      memoryTotalGb: 64,
      cpuBenchmarkScore: 1450, // 高性能24コアマシン
      cpuPercent: 7,
      memoryPercent: 28,
      memoryAvailableGb: 46.0,
      surplus: {
        status: 'evaluated',
        confidence: 0.95,
        allocatableCpuScore: 1060, // 余裕十分な余剰計算資源
        allocatableMemoryGb: 36.0,
        algorithmVersion: 'prototype_v1',
        statusNote: '低負荷かつ広帯域。大規模バッチJobの割り当て推奨',
      },
      latencyMs: 4,
      lastSeen: '1 秒前',
      dataSource: 'sample',
    },
    {
      id: 'laptop-sato',
      name: 'LAPTOP-SATO',
      status: 'online',
      address: '100.64.0.5',
      os: 'Windows 11 Home',
      cpuCores: 8,
      memoryTotalGb: 16,
      cpuBenchmarkScore: 480, // 8コア薄型ノート
      cpuPercent: 63,
      memoryPercent: 72,
      memoryAvailableGb: 4.4,
      surplus: {
        status: 'evaluated',
        confidence: 0.85,
        allocatableCpuScore: 80, // 高負荷のため割り当て可能枠は限定的
        allocatableMemoryGb: 1.5,
        algorithmVersion: 'prototype_v1',
        statusNote: '高負荷のため新規Jobの割り当て制限中',
      },
      latencyMs: 21,
      lastSeen: '2 秒前',
      dataSource: 'sample',
    },
    {
      id: 'mini-pc-lab',
      name: 'MINI-PC-LAB',
      status: 'online',
      address: '100.64.0.7',
      os: 'Windows 10 Pro',
      cpuCores: 4,
      memoryTotalGb: 8,
      cpuBenchmarkScore: 240, // 4コア省電力ミニPC
      cpuPercent: 91,
      memoryPercent: 58,
      memoryAvailableGb: 3.3,
      surplus: {
        status: 'evaluated',
        confidence: 0.90,
        allocatableCpuScore: 0, // 安全閾値(80%)超過により割当不可
        allocatableMemoryGb: 0,
        algorithmVersion: 'prototype_v1',
        statusNote: 'CPU使用率が安全閾値(80%)を超えているため余剰なし',
      },
      latencyMs: 9,
      lastSeen: '1 秒前',
      dataSource: 'sample',
    },
    {
      id: 'old-tower',
      name: 'OLD-TOWER',
      status: 'offline',
      address: '100.64.0.9',
      os: 'Windows 10 Pro',
      cpuCores: 6,
      memoryTotalGb: 16,
      cpuBenchmarkScore: null,
      cpuPercent: null,
      memoryPercent: null,
      memoryAvailableGb: null,
      surplus: {
        status: 'insufficient_data',
        confidence: null,
        allocatableCpuScore: null,
        allocatableMemoryGb: null,
        algorithmVersion: null,
        statusNote: 'ノードがオフラインのため計算資源の評価不能',
      },
      latencyMs: null,
      lastSeen: '2 時間前',
      dataSource: 'sample',
    },
  ],
  // 明示的な接続トポロジー（総当たりではない実際の疎通関係）
  links: [
    { sourceId: 'desktop-main', targetId: 'workstation-02', latencyMs: 4, status: 'active' },
    { sourceId: 'desktop-main', targetId: 'laptop-sato', latencyMs: 21, status: 'active' },
    { sourceId: 'desktop-main', targetId: 'mini-pc-lab', latencyMs: 9, status: 'active' },
    { sourceId: 'workstation-02', targetId: 'laptop-sato', latencyMs: 18, status: 'active' },
    { sourceId: 'workstation-02', targetId: 'mini-pc-lab', latencyMs: 12, status: 'active' },
    { sourceId: 'desktop-main', targetId: 'old-tower', latencyMs: null, status: 'disconnected' },
  ],
}

export const sampleEvents: EventLogEntry[] = [
  { time: '11:19:52', level: 'warn', nodeName: 'MINI-PC-LAB', message: 'CPU負荷上昇により余剰計算資源の割り当てを停止しました' },
  { time: '11:12:07', level: 'info', nodeName: 'LAPTOP-SATO', message: 'ネットワークに参加・性能評価を完了しました' },
  { time: '09:20:31', level: 'warn', nodeName: 'OLD-TOWER', message: '応答途絶のため余剰資源評価を破棄しました' },
  { time: '09:02:15', level: 'info', nodeName: 'WORKSTATION-02', message: '余剰計算資源の評価完了（割当可能CPU: 1280pt, メモリ: 36GB）' },
  { time: '09:00:03', level: 'info', nodeName: 'DESKTOP-MAIN', message: 'Local Agent 起動（余剰資源評価サービス開始）' },
]
