import { useState, useEffect } from 'react'
import type { AppConfig, PeerNode } from '../types'

interface SettingsPanelProps {
  localNode?: PeerNode
  agentConnected: boolean
}

const DEFAULT_CONFIG: AppConfig = {
  agent: {
    node_name: '',
    host: '0.0.0.0',
    port: 3001,
    broadcast_interval_ms: 2000,
  },
  discovery: {
    transport: 'tailscale',
    config_path: null,
    scan_interval_secs: 10,
    probe_interval_secs: 4,
    static_peers: [],
  },
  evaluator: {
    high_load_threshold: 80.0,
    safety_headroom: 20.0,
    min_memory_margin_gb: 4.0,
  },
  benchmark: {
    enabled: true,
    iterations_per_core: 20000000,
    manual_score: 0,
  },
}

export function SettingsPanel({ localNode, agentConnected }: SettingsPanelProps) {
  const [config, setConfig] = useState<AppConfig>(DEFAULT_CONFIG)
  const [staticPeersText, setStaticPeersText] = useState<string>('')
  const [isLoading, setIsLoading] = useState<boolean>(true)
  const [isSaving, setIsSaving] = useState<boolean>(false)
  const [message, setMessage] = useState<{ type: 'success' | 'error'; text: string } | null>(null)

  // API endpoint URL
  const getApiUrl = (endpoint: string) => {
    const host = window.location.hostname || 'localhost'
    const port = window.location.port || '5180'
    return `http://${host}:${port}${endpoint}`
  }

  // 設定の読み込み
  const fetchConfig = async () => {
    setIsLoading(true)
    setMessage(null)
    try {
      const res = await fetch(getApiUrl('/api/config'))
      if (!res.ok) {
        throw new Error(`HTTP ${res.status}: ${res.statusText}`)
      }
      const data: AppConfig = await res.json()
      setConfig(data)
      setStaticPeersText((data.discovery.static_peers || []).join('\n'))
    } catch (err) {
      console.warn('[SpareLink] 設定取得失敗 (初期値を使用):', err)
      setConfig(DEFAULT_CONFIG)
      setStaticPeersText(DEFAULT_CONFIG.discovery.static_peers.join('\n'))
    } finally {
      setIsLoading(false)
    }
  }

  useEffect(() => {
    fetchConfig()
  }, [])

  // 設定の保存と即時反映
  const handleSave = async (e: React.FormEvent) => {
    e.preventDefault()
    setIsSaving(true)
    setMessage(null)

    // static_peers のパース
    const parsedPeers = staticPeersText
      .split(/[\r\n,]+/)
      .map((s) => s.trim())
      .filter((s) => s.length > 0)

    const updatedConfig: AppConfig = {
      ...config,
      discovery: {
        ...config.discovery,
        static_peers: parsedPeers,
      },
    }

    try {
      const res = await fetch(getApiUrl('/api/config'), {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(updatedConfig),
      })
      const result = await res.json()
      if (res.ok && result.status === 'ok') {
        setMessage({ type: 'success', text: result.message || '設定を保存し、エージェントへ即時反映しました！' })
        setConfig(updatedConfig)
      } else {
        throw new Error(result.message || '保存に失敗しました')
      }
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err)
      setMessage({ type: 'error', text: `設定保存エラー: ${msg}` })
    } finally {
      setIsSaving(false)
    }
  }

  // デフォルト値に戻す
  const handleResetToDefault = () => {
    if (window.confirm('設定をデフォルト値（推奨設定）に戻しますか？')) {
      setConfig(DEFAULT_CONFIG)
      setStaticPeersText(DEFAULT_CONFIG.discovery.static_peers.join('\n'))
    }
  }

  // リアルタイム余剰資源シミュレーション
  const currentCpu = localNode?.cpuPercent ?? 15.0
  const currentAvailMem = localNode?.memoryAvailableGb ?? 16.0
  const baseScore = localNode?.cpuBenchmarkScore ?? (localNode?.cpuCores ? localNode.cpuCores * 60 : 1000)

  const isCutoff = currentCpu >= config.evaluator.high_load_threshold
  const effectiveRatio = isCutoff
    ? 0
    : Math.max(0, (100 - currentCpu - config.evaluator.safety_headroom) / 100)
  const simCpuScore = Math.round(baseScore * effectiveRatio)
  const simMemGb = Math.max(0, Math.round((currentAvailMem - config.evaluator.min_memory_margin_gb) * 10) / 10)

  return (
    <div className="settings-page">
      <div className="page-head">
        <div>
          <h1>エージェント設定 (Configuration)</h1>
          <p className="page-desc">
            Local Agent（{localNode?.name || 'このPC'}）の余剰計算資源の評価基準、安全マージン、P2Pネットワーク探索ルールを細かく調整します。
          </p>
        </div>
        <div className="settings-head-actions">
          <button
            type="button"
            onClick={fetchConfig}
            className="btn btn--secondary"
            disabled={isLoading || isSaving}
          >
            🔄 再読み込み
          </button>
        </div>
      </div>

      {message && (
        <div className={`settings-alert settings-alert--${message.type}`}>
          <span className="alert-icon">{message.type === 'success' ? '✓' : '⚠'}</span>
          <span>{message.text}</span>
        </div>
      )}

      {!agentConnected && (
        <div className="settings-alert settings-alert--warn">
          <span className="alert-icon">ℹ</span>
          <span>
            Local Agent（:3001 / :5180）と接続されていません。設定は表示されますが、保存するにはエージェントを起動してください。
          </span>
        </div>
      )}

      <form onSubmit={handleSave} className="settings-form">
        {/* ========================================================
            セクション1: 余剰計算資源・安全マージン制御 (最重要)
           ======================================================== */}
        <section className="settings-card">
          <div className="settings-card-header">
            <div className="card-badge">最重要</div>
            <h2>余剰計算資源の評価・安全マージン設定</h2>
            <p className="card-sub">
              ホストPCの普段の作業が絶対に重くならないよう、確保する安全余裕と貸し出し上限を設定します。
            </p>
          </div>

          <div className="settings-grid">
            {/* CPU安全マージン */}
            <div className="form-group">
              <div className="form-label-row">
                <label htmlFor="safety_headroom">
                  <strong>CPU 安全マージン (Safety Headroom)</strong>
                </label>
                <span className="val-preview num">{config.evaluator.safety_headroom}%</span>
              </div>
              <input
                id="safety_headroom"
                type="range"
                min="0"
                max="50"
                step="1"
                value={config.evaluator.safety_headroom}
                onChange={(e) =>
                  setConfig({
                    ...config,
                    evaluator: { ...config.evaluator, safety_headroom: Number(e.target.value) },
                  })
                }
                className="slider"
              />
              <p className="form-desc">
                作業の突発的なバースト負荷に備えて、常に自分のためにキープしておくCPU余白。
                実効余剰率は <code>(100% − CPU使用率 − {config.evaluator.safety_headroom}%)</code> で算出されます。
              </p>
            </div>

            {/* 高負荷自動遮断閾値 */}
            <div className="form-group">
              <div className="form-label-row">
                <label htmlFor="high_load_threshold">
                  <strong>高負荷カットオフ閾値 (High Load Cutoff)</strong>
                </label>
                <span className="val-preview num is-warn">{config.evaluator.high_load_threshold}%</span>
              </div>
              <input
                id="high_load_threshold"
                type="range"
                min="50"
                max="95"
                step="1"
                value={config.evaluator.high_load_threshold}
                onChange={(e) =>
                  setConfig({
                    ...config,
                    evaluator: { ...config.evaluator, high_load_threshold: Number(e.target.value) },
                  })
                }
                className="slider slider--warn"
              />
              <p className="form-desc">
                ホストPCのCPU使用率がこの値を超えた場合、新規Jobや他PCからのリクエストを強制停止し、余剰割当を自動で <code>0 pt</code> に遮断します。
              </p>
            </div>

            {/* メモリ安全マージン */}
            <div className="form-group">
              <div className="form-label-row">
                <label htmlFor="min_memory_margin_gb">
                  <strong>メモリ安全マージン (Reserved RAM)</strong>
                </label>
                <span className="val-preview num">{config.evaluator.min_memory_margin_gb} GB</span>
              </div>
              <input
                id="min_memory_margin_gb"
                type="range"
                min="1.0"
                max="16.0"
                step="0.5"
                value={config.evaluator.min_memory_margin_gb}
                onChange={(e) =>
                  setConfig({
                    ...config,
                    evaluator: { ...config.evaluator, min_memory_margin_gb: Number(e.target.value) },
                  })
                }
                className="slider"
              />
              <p className="form-desc">
                OSや起動中アプリのクラッシュを防ぐため、OSが報告する空き物理メモリから無条件に差し引く予約容量です。
              </p>
            </div>
          </div>

          {/* リアルタイム・シミュレータボックス */}
          <div className="sim-box">
            <div className="sim-title">
              <span>⚡ リアルタイム計算シミュレーション（現在のこのPCで試算）</span>
              <span className="sim-meta">
                実測CPU: {currentCpu.toFixed(1)}% | 空きメモリ: {currentAvailMem.toFixed(1)} GB
              </span>
            </div>
            <div className="sim-result-grid">
              <div className="sim-cell">
                <span className="sim-cell-label">実効割当可能CPU</span>
                <span className={`sim-cell-val num ${isCutoff ? 'is-cutoff' : ''}`}>
                  {isCutoff ? '0 pt (カットオフ)' : `${simCpuScore.toLocaleString()} pt`}
                </span>
                <span className="sim-cell-sub">
                  {isCutoff
                    ? `使用率が閾値 (${config.evaluator.high_load_threshold}%) を超えているため遮断中`
                    : `基準性能 ${baseScore.toLocaleString()} pt × ヘッドルーム ${(effectiveRatio * 100).toFixed(1)}%`}
                </span>
              </div>
              <div className="sim-cell">
                <span className="sim-cell-label">実効割当可能メモリ</span>
                <span className="sim-cell-val num">
                  {isCutoff ? '0.0 GB' : `${simMemGb.toFixed(1)} GB`}
                </span>
                <span className="sim-cell-sub">
                  空き {currentAvailMem.toFixed(1)} GB − マージン {config.evaluator.min_memory_margin_gb.toFixed(1)} GB
                </span>
              </div>
            </div>
          </div>
        </section>

        {/* ========================================================
            セクション2: ネットワーク ＆ P2P 探索設定
           ======================================================== */}
        <section className="settings-card">
          <div className="settings-card-header">
            <h2>P2P 探索 ＆ ネットワーク設定</h2>
            <p className="card-sub">PC同士の接続方式やポーリング周期を設定します。</p>
          </div>

          <div className="settings-grid">
            <div className="form-group">
              <label htmlFor="node_name">
                <strong>ノード表示名 (Node Name)</strong>
              </label>
              <input
                id="node_name"
                type="text"
                value={config.agent.node_name}
                placeholder="空欄の場合はOSホスト名を自動検出"
                onChange={(e) =>
                  setConfig({
                    ...config,
                    agent: { ...config.agent, node_name: e.target.value },
                  })
                }
                className="text-input"
              />
              <p className="form-desc">WebダッシュボードやP2Pメッシュ内で表示されるこのPCの識別名です。</p>
            </div>

            <div className="form-group">
              <label htmlFor="transport">
                <strong>ピア探索方式 (Transport Provider)</strong>
              </label>
              <select
                id="transport"
                value={config.discovery.transport}
                onChange={(e) =>
                  setConfig({
                    ...config,
                    discovery: { ...config.discovery, transport: e.target.value },
                  })
                }
                className="select-input"
              >
                <option value="tailscale">Tailscale (推奨: 自動検出・NAT越え)</option>
                <option value="wireguard">WireGuard (固定VPN構成)</option>
                <option value="static">Static (固定IPアドレス指定)</option>
                <option value="lan">LAN Direct (ローカルネットワーク)</option>
              </select>
              <p className="form-desc">PC間の通信トンネルおよびピア検出方式を選択します。</p>
            </div>

            <div className="form-group">
              <label htmlFor="port">
                <strong>WebSocket / P2P 待受ポート</strong>
              </label>
              <input
                id="port"
                type="number"
                min="1024"
                max="65535"
                value={config.agent.port}
                onChange={(e) =>
                  setConfig({
                    ...config,
                    agent: { ...config.agent, port: Number(e.target.value) },
                  })
                }
                className="text-input"
              />
              <p className="form-desc">エージェント間のP2P Ping/PongおよびWeb UI配信用ポート（標準: 3001）。</p>
            </div>

            <div className="form-group">
              <label htmlFor="scan_interval_secs">
                <strong>ピア自動スキャン周期 (秒)</strong>
              </label>
              <input
                id="scan_interval_secs"
                type="number"
                min="2"
                max="60"
                value={config.discovery.scan_interval_secs}
                onChange={(e) =>
                  setConfig({
                    ...config,
                    discovery: { ...config.discovery, scan_interval_secs: Number(e.target.value) },
                  })
                }
                className="text-input"
              />
              <p className="form-desc">Tailscale CLI等から新着ピアを検索する間隔です。</p>
            </div>

            <div className="form-group form-group--full">
              <label htmlFor="static_peers">
                <strong>固定接続先ピア (Static Peers)</strong>
              </label>
              <textarea
                id="static_peers"
                rows={3}
                value={staticPeersText}
                placeholder="100.64.0.2:3001&#10;192.168.1.50:3001"
                onChange={(e) => setStaticPeersText(e.target.value)}
                className="textarea-input"
              />
              <p className="form-desc">
                自動検出されない固定PCを手動で追加したい場合に、1行に1つの <code>IP:Port</code> または <code>IP</code> を入力します。
              </p>
            </div>
          </div>
        </section>

        {/* ========================================================
            セクション3: ベンチマーク ＆ 性能評価設定
           ======================================================== */}
        <section className="settings-card">
          <div className="settings-card-header">
            <h2>CPU マイクロベンチマーク設定</h2>
            <p className="card-sub">PC起動時の性能測定およびスコア手動上書き設定。</p>
          </div>

          <div className="settings-grid">
            <div className="form-group">
              <label className="checkbox-label">
                <input
                  type="checkbox"
                  checked={config.benchmark.enabled}
                  onChange={(e) =>
                    setConfig({
                      ...config,
                      benchmark: { ...config.benchmark, enabled: e.target.checked },
                    })
                  }
                />
                <strong>起動時に並列CPUベンチマークを自動実行する</strong>
              </label>
              <p className="form-desc">
                起動直後に全論理コアで約0.1秒の高速複合演算（ILP+Cache+FMA）を行い、基準性能スコア（pt）を確定します。
              </p>
            </div>

            <div className="form-group">
              <label htmlFor="manual_score">
                <strong>手動スコア固定値 (Manual Score Override)</strong>
              </label>
              <input
                id="manual_score"
                type="number"
                min="0"
                max="500000"
                value={config.benchmark.manual_score}
                onChange={(e) =>
                  setConfig({
                    ...config,
                    benchmark: { ...config.benchmark, manual_score: Number(e.target.value) },
                  })
                }
                className="text-input"
              />
              <p className="form-desc">
                <code>0</code> の場合は実測ベンチマーク値を使用します。特定の固定スコア（pt）を強制したい場合に入力します。
              </p>
            </div>
          </div>
        </section>

        {/* アクションボタンバー */}
        <div className="settings-actions-bar">
          <button
            type="submit"
            className="btn btn--primary btn--lg"
            disabled={isSaving || !agentConnected}
          >
            {isSaving ? '保存中...' : '💾 設定を保存してエージェントに即時反映'}
          </button>
          <button
            type="button"
            onClick={handleResetToDefault}
            className="btn btn--secondary"
            disabled={isSaving}
          >
            初期値に戻す
          </button>
        </div>
      </form>
    </div>
  )
}
