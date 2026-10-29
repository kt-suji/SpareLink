import { useState, useEffect, useRef } from 'react'
import type { ComputeJob, JobStatus, PeerNode } from '../types'

interface JobManagementPanelProps {
  localNode?: PeerNode
  nodes: PeerNode[]
  agentConnected: boolean
}

interface JobTemplate {
  name: string
  job_type: 'python' | 'command'
  command: string
  description: string
}

const TEMPLATES: JobTemplate[] = [
  {
    name: '🔥 30秒間 全コア高負荷ストレステスト (分散動作検証用)',
    job_type: 'python',
    command: `import sys
import os
import time
import subprocess

duration = 30
cores = os.cpu_count() or 4
host_name = os.environ.get('COMPUTERNAME', os.environ.get('HOSTNAME', 'Unknown'))

print("=" * 60)
print(f"[SpareLink Worker] ★ 実行先ホスト名: {host_name}")
print(f"[SpareLink Worker] 全 {cores} コアを使って {duration} 秒間の高負荷並列計算を開始します...")
print(f"[SpareLink Worker] ※ ターゲットPCのタスクマネージャーでCPU使用率が100%に急上昇します。")
print(f"[SpareLink Worker] ※ 指示を出した自PCのCPUは数%のまま静かであることを確認してください。")
print("=" * 60)

worker_code = f"""
import time
t_end = time.time() + {duration}
while time.time() < t_end:
    _ = [x*x for x in range(10000)]
"""

t0 = time.time()
procs = [subprocess.Popen([sys.executable, "-c", worker_code]) for _ in range(cores)]

for p in procs:
    p.wait()

elapsed = time.time() - t0
print("=" * 60)
print(f"[SpareLink Worker] ★ 30秒ストレステスト完了! 実効時間: {elapsed:.2f}秒")
print(f"[SpareLink Worker] ホストPC '{host_name}' の全 {cores} コアによる分散処理が正常終了しました。")
print("=" * 60)
`,
    description: '全CPUコアを30秒間フル稼働させ、相手PCのタスクマネージャー急上昇と自PCの低負荷を視覚確認するテスト',
  },
  {
    name: '素数探索ベンチマーク (2,000,000以下)',
    job_type: 'python',
    command: `import time

def count_primes(limit):
    sieve = [True] * (limit + 1)
    sieve[0] = sieve[1] = False
    for i in range(2, int(limit**0.5) + 1):
        if sieve[i]:
            for j in range(i * i, limit + 1, i):
                sieve[j] = False
    return sum(sieve)

print("[SpareLink Worker] 素数探索を開始します (上限: 2,000,000)...")
t0 = time.time()
primes_count = count_primes(2_000_000)
elapsed = time.time() - t0
print(f"[SpareLink Worker] 完了! 発見した素数の数: {primes_count:,} 個")
print(f"[SpareLink Worker] 実効計算時間: {elapsed:.3f} 秒")
`,
    description: 'エラトステネスの篩による200万以下の素数計数タスク',
  },
  {
    name: 'モンテカルロ円周率近似 (10,000,000回試行)',
    job_type: 'python',
    command: `import random
import time

print("[SpareLink Worker] モンテカルロ円周率計算開始 (10,000,000回試行)...")
t0 = time.time()
inside = 0
total = 10_000_000

for _ in range(total):
    x = random.random()
    y = random.random()
    if x*x + y*y <= 1.0:
        inside += 1

pi_estimate = 4.0 * inside / total
elapsed = time.time() - t0
print(f"[SpareLink Worker] 試行完了: pi ≒ {pi_estimate:.6f} (理論値: 3.141592)")
print(f"[SpareLink Worker] 誤差: {abs(pi_estimate - 3.1415926535):.6f}")
print(f"[SpareLink Worker] 計算所要時間: {elapsed:.3f} 秒")
`,
    description: '1000万回の疑似乱数投点による円周率モンテカルロ推定',
  },
  {
    name: 'マルチスレッド行列積ストレステスト',
    job_type: 'python',
    command: `import time

print("[SpareLink Worker] 行列積計算によるCPUストレステストを開始します...")
t0 = time.time()
n = 500
A = [[(i + j) % 10 for j in range(n)] for i in range(n)]
B = [[(i * j) % 10 for j in range(n)] for i in range(n)]
C = [[0] * n for _ in range(n)]

for i in range(n):
    for k in range(n):
        for j in range(n):
            C[i][j] += A[i][k] * B[k][j]

elapsed = time.time() - t0
print(f"[SpareLink Worker] {n}x{n} 行列積乗算完了 (チェックサム: {C[0][0]})")
print(f"[SpareLink Worker] 計算所要時間: {elapsed:.3f} 秒")
`,
    description: 'CPUコアの浮動小数/整数演算能力をフル活用する行列乗算',
  },
  {
    name: 'ホスト環境システム診断バッチ',
    job_type: 'command',
    command: 'hostname & echo [OS Version] & ver & echo [Processor Cores] & echo %NUMBER_OF_PROCESSORS% & echo [Disk Space] & dir C:\\ | findstr "バイトの空き領域"',
    description: '実行PCのホスト名、OSバージョン、論理コア数、空き容量を取得',
  },
]

export function JobManagementPanel({ localNode, nodes, agentConnected }: JobManagementPanelProps) {
  const [jobs, setJobs] = useState<ComputeJob[]>([])
  const [isLoading, setIsLoading] = useState<boolean>(true)
  const [isSubmitting, setIsSubmitting] = useState<boolean>(false)
  const [message, setMessage] = useState<{ type: 'success' | 'error'; text: string } | null>(null)
  const [autoRefresh, setAutoRefresh] = useState<boolean>(true)
  const [filterStatus, setFilterStatus] = useState<string>('all')
  const [selectedJob, setSelectedJob] = useState<ComputeJob | null>(null)

  // フォーム入力項目
  const [jobName, setJobName] = useState<string>(TEMPLATES[0].name)
  const [jobType, setJobType] = useState<'python' | 'command'>('python')
  const [targetNodeId, setTargetNodeId] = useState<string>('auto')
  const [commandStr, setCommandStr] = useState<string>(TEMPLATES[0].command)

  const pollIntervalRef = useRef<number | null>(null)

  const getApiUrl = (endpoint: string) => {
    const host = window.location.hostname || 'localhost'
    const port = window.location.port || '5180'
    return `http://${host}:${port}${endpoint}`
  }

  // ジョブ一覧の取得
  const fetchJobs = async () => {
    try {
      const res = await fetch(getApiUrl('/api/jobs'))
      if (!res.ok) {
        throw new Error(`HTTP ${res.status}: ${res.statusText}`)
      }
      const data: ComputeJob[] = await res.json()
      setJobs(data)
      // 選択中ジョブがあれば最新データに更新
      if (selectedJob) {
        const updated = data.find((j) => j.id === selectedJob.id)
        if (updated) setSelectedJob(updated)
      }
    } catch (err) {
      console.warn('[SpareLink Jobs] ジョブ一覧取得失敗:', err)
    } finally {
      setIsLoading(false)
    }
  }

  // 初回取得と定期ポーリング
  useEffect(() => {
    fetchJobs()
    if (autoRefresh) {
      pollIntervalRef.current = window.setInterval(fetchJobs, 1500)
    }
    return () => {
      if (pollIntervalRef.current) {
        clearInterval(pollIntervalRef.current)
      }
    }
  }, [autoRefresh, selectedJob?.id])

  // テンプレート適用
  const applyTemplate = (tpl: JobTemplate) => {
    setJobName(tpl.name)
    setJobType(tpl.job_type)
    setCommandStr(tpl.command)
    setMessage({
      type: 'success',
      text: `テンプレート「${tpl.name}」をセットしました`,
    })
  }

  // 新規ジョブの投入
  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    if (!commandStr.trim()) {
      setMessage({ type: 'error', text: '実行スクリプトまたはコマンドを入力してください' })
      return
    }

    setIsSubmitting(true)
    setMessage(null)

    const randomId = Math.random().toString(36).substring(2, 10)
    const newJob: ComputeJob = {
      id: `job-${randomId}`,
      name: jobName.trim() || `Job-${randomId}`,
      command: commandStr,
      job_type: jobType,
      target_node_id: targetNodeId,
      status: 'pending',
      submitted_at: '',
      stdout: '',
      stderr: '',
    }

    try {
      const res = await fetch(getApiUrl('/api/jobs/submit'), {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(newJob),
      })

      if (!res.ok) {
        const errJson = await res.json().catch(() => ({}))
        throw new Error(errJson.message || `HTTP ${res.status}`)
      }

      const submitted: ComputeJob = await res.json()
      setMessage({
        type: 'success',
        text: `ジョブ「${submitted.name}」を正常に投入しました！(実行先: ${submitted.assigned_node_name || '割当中'})`,
      })
      // 即座に一覧を更新してモーダル・ログを表示
      await fetchJobs()
      setSelectedJob(submitted)
    } catch (err) {
      setMessage({
        type: 'error',
        text: `ジョブ投入に失敗しました: ${err instanceof Error ? err.message : String(err)}`,
      })
    } finally {
      setIsSubmitting(false)
    }
  }

  // 統計集計
  const totalCount = jobs.length
  const runningCount = jobs.filter((j) => j.status === 'running').length
  const completedCount = jobs.filter((j) => j.status === 'completed').length
  const failedCount = jobs.filter((j) => j.status === 'failed').length

  // 最高余剰PCの探索
  const topSurplusNode = [...nodes].sort(
    (a, b) => (b.surplus.allocatableCpuScore || 0) - (a.surplus.allocatableCpuScore || 0)
  )[0]

  // フィルタ済みジョブ一覧
  const filteredJobs = jobs.filter((j) => {
    if (filterStatus === 'all') return true
    return j.status === filterStatus
  })

  const getStatusBadge = (status: JobStatus) => {
    switch (status) {
      case 'running':
        return <span className="job-badge job-badge--running">⚡ 実行中</span>
      case 'completed':
        return <span className="job-badge job-badge--completed">✓ 完了</span>
      case 'failed':
        return <span className="job-badge job-badge--failed">✕ 失敗</span>
      case 'cancelled':
        return <span className="job-badge job-badge--cancelled">― 中止</span>
      default:
        return <span className="job-badge job-badge--pending">⏳ 待機中</span>
    }
  }

  return (
    <div className="job-panel">
      {/* ページヘッダー */}
      <div className="page-head">
        <div>
          <h1>共通Job管理 (P2Pタスク分散・実行エンジン)</h1>
          <p className="page-desc">
            重い計算負荷（PythonスクリプトやOSコマンド）を、ネットワーク内で最も余剰能力が高いPCへ自動オフロードして並列分散実行します。
          </p>
        </div>
        <div className="job-head-actions">
          <label className="auto-refresh-toggle">
            <input
              type="checkbox"
              checked={autoRefresh}
              onChange={(e) => setAutoRefresh(e.target.checked)}
            />
            <span>自動更新 (1.5秒間隔)</span>
          </label>
          <button type="button" className="btn btn--secondary" onClick={fetchJobs}>
            🔄 最新に更新
          </button>
        </div>
      </div>

      {/* ステータスカード群 */}
      <div className="job-stats-grid">
        <div className="job-stat-card">
          <div className="stat-label">総投入ジョブ数</div>
          <div className="stat-value num">{totalCount}</div>
          <div className="stat-sub">分散実行キュー履歴</div>
        </div>
        <div className="job-stat-card">
          <div className="stat-label">実行中 (Running)</div>
          <div className="stat-value num stat-value--running">{runningCount}</div>
          <div className="stat-sub">現在ノードで処理中</div>
        </div>
        <div className="job-stat-card">
          <div className="stat-label">正常完了 (Completed)</div>
          <div className="stat-value num stat-value--completed">{completedCount}</div>
          <div className="stat-sub">結果受信済み</div>
        </div>
        <div className="job-stat-card">
          <div className="stat-label">失敗・拒否 (Failed)</div>
          <div className="stat-value num stat-value--failed">{failedCount}</div>
          <div className="stat-sub">高負荷安全カットオフ等</div>
        </div>
        <div className="job-stat-card job-stat-card--highlight">
          <div className="stat-label">⚡ 自動選定の第一候補PC</div>
          <div className="stat-value stat-value--node">
            {topSurplusNode ? topSurplusNode.name : '検出中'}
          </div>
          <div className="stat-sub num">
            余剰実効能力: {topSurplusNode?.surplus.allocatableCpuScore?.toLocaleString() ?? 0} pt
            {topSurplusNode?.id === localNode?.id ? ' (この自PC)' : ' (リモートピア)'}
          </div>
        </div>
      </div>

      {/* メッセージトースト */}
      {message && (
        <div className={`notice notice--${message.type === 'success' ? 'live' : 'warn'}`}>
          <strong>{message.type === 'success' ? '✓ 成功: ' : '⚠ エラー: '}</strong>
          {message.text}
        </div>
      )}

      {/* ジョブ投入フォームカード */}
      <section className="card job-submit-card">
        <div className="card-header">
          <h2>🚀 新規ジョブの投入・分散オフロード</h2>
          <span className="card-badge">リアルタイムP2Pディスパッチ</span>
        </div>

        {/* テンプレートクイック適用バー */}
        <div className="template-bar">
          <span className="template-title">クイック試行テンプレート:</span>
          <div className="template-chips">
            {TEMPLATES.map((tpl, idx) => (
              <button
                key={idx}
                type="button"
                className="template-chip"
                onClick={() => applyTemplate(tpl)}
                title={tpl.description}
              >
                {tpl.job_type === 'python' ? '🐍 ' : '💻 '}
                {tpl.name}
              </button>
            ))}
          </div>
        </div>

        <form onSubmit={handleSubmit} className="job-form">
          <div className="job-form-row">
            <div className="form-group flex-2">
              <label htmlFor="job-name">ジョブ名 / タスク名称</label>
              <input
                id="job-name"
                type="text"
                value={jobName}
                onChange={(e) => setJobName(e.target.value)}
                placeholder="例: 素数探索ベンチマーク #1"
                required
              />
            </div>

            <div className="form-group flex-1">
              <label htmlFor="job-type">実行環境 / 言語種別</label>
              <select
                id="job-type"
                value={jobType}
                onChange={(e) => setJobType(e.target.value as 'python' | 'command')}
              >
                <option value="python">Python 3 (ローカルPython環境)</option>
                <option value="command">OS コマンド (cmd / sh)</option>
              </select>
            </div>

            <div className="form-group flex-2">
              <label htmlFor="target-node">実行先PCの割り当て</label>
              <select
                id="target-node"
                value={targetNodeId}
                onChange={(e) => setTargetNodeId(e.target.value)}
              >
                <option value="auto">
                  ⚡ 【自動選定】余剰能力トップPCへ自動オフロード（推奨）
                </option>
                <option value="local">
                  💻 この自PCのみで実行 ({localNode?.name ?? '自PC'}:{' '}
                  {localNode?.surplus.allocatableCpuScore?.toLocaleString() ?? 0} pt)
                </option>
                {nodes
                  .filter((n) => n.id !== localNode?.id)
                  .map((peer) => (
                    <option key={peer.id} value={peer.id}>
                      🌐 {peer.name} ({peer.surplus.allocatableCpuScore?.toLocaleString() ?? 0} pt,{' '}
                      {peer.latencyMs != null ? `${peer.latencyMs}ms` : 'online'})
                    </option>
                  ))}
              </select>
            </div>
          </div>

          <div className="form-group">
            <div className="code-label-row">
              <label htmlFor="job-code">
                実行スクリプト / コマンド内容
                <span className="code-hint">
                  {jobType === 'python'
                    ? '（Pythonコードを直接記述。標準出力 print() はダッシュボードで即時監視可能）'
                    : '（Windows cmd / Linux sh コマンドラインを直接実行）'}
                </span>
              </label>
            </div>
            <textarea
              id="job-code"
              rows={8}
              className="code-textarea"
              value={commandStr}
              onChange={(e) => setCommandStr(e.target.value)}
              placeholder={
                jobType === 'python'
                  ? 'print("Hello from SpareLink distributed job!")'
                  : 'hostname'
              }
              required
            />
          </div>

          <div className="job-form-actions">
            <button
              type="submit"
              className="btn btn--primary job-submit-btn"
              disabled={isSubmitting || !agentConnected}
            >
              {isSubmitting ? 'ディスパッチ中...' : '🚀 ジョブを投入して分散実行を開始'}
            </button>
            {!agentConnected && (
              <span className="warn-text">※ Local Agentに未接続のためジョブ投入できません</span>
            )}
          </div>
        </form>
      </section>

      {/* ジョブ一覧テーブルカード */}
      <section className="card">
        <div className="card-header">
          <div className="header-left">
            <h2>📋 ジョブ実行履歴・進捗状況</h2>
            <span className="count-tag">{filteredJobs.length} 件</span>
          </div>

          {/* 状態フィルタータブ */}
          <div className="filter-tabs">
            <button
              type="button"
              className={`filter-tab ${filterStatus === 'all' ? 'is-active' : ''}`}
              onClick={() => setFilterStatus('all')}
            >
              全て ({totalCount})
            </button>
            <button
              type="button"
              className={`filter-tab ${filterStatus === 'running' ? 'is-active' : ''}`}
              onClick={() => setFilterStatus('running')}
            >
              実行中 ({runningCount})
            </button>
            <button
              type="button"
              className={`filter-tab ${filterStatus === 'completed' ? 'is-active' : ''}`}
              onClick={() => setFilterStatus('completed')}
            >
              完了 ({completedCount})
            </button>
            <button
              type="button"
              className={`filter-tab ${filterStatus === 'failed' ? 'is-active' : ''}`}
              onClick={() => setFilterStatus('failed')}
            >
              失敗 ({failedCount})
            </button>
          </div>
        </div>

        {filteredJobs.length === 0 ? (
          <div className="empty-jobs">
            <div className="empty-icon">📂</div>
            <p className="empty-title">
              {isLoading ? 'ジョブ履歴を取得中...' : '対象のジョブはありません'}
            </p>
            <p className="empty-desc">
              上の「クイック試行テンプレート」からワンクリックで計算ジョブを投入できます。
            </p>
          </div>
        ) : (
          <div className="table-wrapper">
            <table className="jobs-table">
              <thead>
                <tr>
                  <th>状態</th>
                  <th>ジョブ名 / ID</th>
                  <th>種別</th>
                  <th>投入元 PC</th>
                  <th>割り当て実行先 PC</th>
                  <th>投入時刻</th>
                  <th>終了コード</th>
                  <th>出力・操作</th>
                </tr>
              </thead>
              <tbody>
                {filteredJobs.map((job) => (
                  <tr
                    key={job.id}
                    className={`job-row ${selectedJob?.id === job.id ? 'is-selected' : ''}`}
                  >
                    <td>{getStatusBadge(job.status)}</td>
                    <td>
                      <div className="job-name-cell">
                        <strong>{job.name}</strong>
                        <span className="job-id-mono">{job.id}</span>
                      </div>
                    </td>
                    <td>
                      <span className="type-pill">
                        {job.job_type === 'python' ? '🐍 Python' : '💻 Command'}
                      </span>
                    </td>
                    <td>
                      <span className="node-badge">
                        {job.requester_node_name || job.requester_node_id || 'ローカル'}
                      </span>
                    </td>
                    <td>
                      <span className="node-badge node-badge--assigned">
                        {job.assigned_node_name || job.assigned_node_id || '自動選定中...'}
                      </span>
                    </td>
                    <td className="num">{job.submitted_at || '―'}</td>
                    <td>
                      {job.exit_code !== undefined && job.exit_code !== null ? (
                        <span
                          className={`exit-code ${
                            job.exit_code === 0 ? 'exit-code--zero' : 'exit-code--nonzero'
                          }`}
                        >
                          code: {job.exit_code}
                        </span>
                      ) : (
                        <span className="exit-code exit-code--running">処理中</span>
                      )}
                    </td>
                    <td>
                      <button
                        type="button"
                        className="btn btn--small btn--terminal"
                        onClick={() => setSelectedJob(job)}
                      >
                        📟 実行ログ
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </section>

      {/* 実行ログ詳細モーダル / コンソールビューア */}
      {selectedJob && (
        <div className="modal-backdrop" onClick={() => setSelectedJob(null)}>
          <div className="modal-window terminal-modal" onClick={(e) => e.stopPropagation()}>
            <div className="terminal-header">
              <div className="terminal-title">
                <span className="terminal-dot red" />
                <span className="terminal-dot yellow" />
                <span className="terminal-dot green" />
                <span className="terminal-name">
                  📟 出力ログコンソール: {selectedJob.name} ({selectedJob.id})
                </span>
              </div>
              <button
                type="button"
                className="terminal-close"
                onClick={() => setSelectedJob(null)}
                aria-label="閉じる"
              >
                ✕
              </button>
            </div>

            <div className="terminal-meta">
              <span>ステータス: {getStatusBadge(selectedJob.status)}</span>
              <span>
                実行先PC: <strong>{selectedJob.assigned_node_name || '未割当'}</strong>
              </span>
              <span>
                終了コード:{' '}
                {selectedJob.exit_code !== undefined && selectedJob.exit_code !== null
                  ? selectedJob.exit_code
                  : '未完了'}
              </span>
              <span>投入時刻: {selectedJob.submitted_at}</span>
            </div>

            <div className="terminal-body">
              <div className="terminal-section">
                <div className="terminal-section-title">【実行スクリプト】</div>
                <pre className="terminal-code">{selectedJob.command}</pre>
              </div>

              <div className="terminal-section">
                <div className="terminal-section-title">【標準出力 (stdout)】</div>
                {selectedJob.stdout ? (
                  <pre className="terminal-output terminal-stdout">{selectedJob.stdout}</pre>
                ) : (
                  <div className="terminal-empty">
                    {selectedJob.status === 'running'
                      ? '⚡ プロセス実行中... 出力を待機しています'
                      : '（標準出力はありませんでした）'}
                  </div>
                )}
              </div>

              {selectedJob.stderr && (
                <div className="terminal-section">
                  <div className="terminal-section-title">
                    【標準エラー / 実行結果情報 (stderr)】
                  </div>
                  <pre className="terminal-output terminal-stderr">{selectedJob.stderr}</pre>
                </div>
              )}
            </div>

            <div className="terminal-footer">
              <button
                type="button"
                className="btn btn--secondary"
                onClick={() => setSelectedJob(null)}
              >
                閉じる
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}
