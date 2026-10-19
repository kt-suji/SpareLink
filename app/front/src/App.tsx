import { useState } from 'react'
import './App.css'
import { NodeDetail } from './components/NodeDetail'
import { NodeTable } from './components/NodeTable'
import { SourceTag } from './components/SourceTag'
import { Topology } from './components/Topology'
import { sampleEvents } from './data/sampleData'
import { useNetworkSnapshot } from './hooks/useNetworkSnapshot'
import { summarize } from './lib/networkSummary'

const events = sampleEvents

const NAV_ITEMS = [
  { label: 'ネットワーク概要', implemented: true },
  { label: 'PC一覧', implemented: false },
  { label: '共通Job管理', implemented: false },
  { label: 'ログ', implemented: false },
  { label: '設定', implemented: false },
]

function App() {
  const { snapshot, agentConnected, tailscale } = useNetworkSnapshot()
  const [selectedId, setSelectedId] = useState(snapshot.localNodeId)

  const { nodes, links, localNodeId, source } = snapshot
  const localNode = nodes.find((n) => n.id === localNodeId)
  const selectedNode = nodes.find((n) => n.id === selectedId) ?? nodes[0]
  const summary = summarize(nodes)
  const isSample = source === 'sample'

  return (
    <div className="app">
      <header className="titlebar">
        <div className="brand">
          <span className="brand-mark" aria-hidden="true" />
          <span className="brand-name">SpareLink</span>
          <span className="brand-version">v0.1.0</span>
        </div>
        <div className="titlebar-status">
          <span className="ts-item">
            <span className="ts-label">このPC</span>
            <span className="ts-value">{localNode?.name ?? '不明'}</span>
            {localNode?.dataSource === 'sample' && <span className="ts-note">（サンプル）</span>}
          </span>
          <span className="ts-item">
            <span className="ts-label">Local Agent</span>
            <span className={`ts-value ${agentConnected ? '' : 'ts-value--off'}`}>
              <span
                className={`status-dot ${agentConnected ? 'status-dot--online' : 'status-dot--offline'}`}
                aria-hidden="true"
              />
              {agentConnected ? '稼働中 (実測)' : '未接続'}
            </span>
          </span>
          {agentConnected && tailscale && (
            <span className="ts-item">
              <span className="ts-label">Tailscale</span>
              <span
                className={`ts-value ${
                  tailscale.state === 'running' ? 'ts-value--ts-ok' : 'ts-value--ts-warn'
                }`}
              >
                <span
                  className={`status-dot ${
                    tailscale.state === 'running'
                      ? 'status-dot--online'
                      : 'status-dot--ts-warn'
                  }`}
                  aria-hidden="true"
                />
                {tailscale.state === 'running'
                  ? `接続中 (${tailscale.selfIp ?? '100.x'})`
                  : tailscale.state === 'needs_login'
                  ? '未ログイン'
                  : tailscale.state === 'not_installed'
                  ? '未検出'
                  : '停止中'}
              </span>
            </span>
          )}
          <span className="ts-item">
            <span className="ts-label">最終更新</span>
            <span className="ts-value num">{snapshot.capturedAt ?? '—'}</span>
          </span>
        </div>
      </header>

      <div className="body">
        <nav className="sidebar" aria-label="メインメニュー">
          <ul>
            {NAV_ITEMS.map((item, i) => (
              <li key={item.label}>
                <button
                  type="button"
                  className={`nav-item ${i === 0 ? 'is-active' : ''}`}
                  disabled={!item.implemented}
                  aria-current={i === 0 ? 'page' : undefined}
                >
                  <span>{item.label}</span>
                  {!item.implemented && <span className="nav-soon">未実装</span>}
                </button>
              </li>
            ))}
          </ul>
        </nav>

        <main className="main">
          <div className="page-head">
            <div>
              <h1>ネットワーク全体の状況</h1>
              <p className="page-desc">
                各PCの負荷と基礎性能を評価し、実際に処理へ割り当て可能な「余剰計算資源」を算出・可視化します。
              </p>
            </div>
            <ul className="data-legend" aria-label="表示値の凡例">
              <li>
                <span className="legend-swatch legend-swatch--live" aria-hidden="true" />
                実測値
              </li>
              <li>
                <span className="legend-swatch legend-swatch--sample" aria-hidden="true" />
                サンプル値
              </li>
              <li>
                <span className="legend-swatch legend-swatch--none" aria-hidden="true" />
                未取得・未評価
              </li>
            </ul>
          </div>

          {agentConnected ? (
            <p className="notice notice--live" role="note">
              <strong>Local Agent 実測連携中</strong>
              このPC（{localNode?.name}）の実測メトリクス、並列CPUベンチマーク（{localNode?.cpuBenchmarkScore?.toLocaleString() ?? '—'} pt）、および余剰計算資源をリアルタイム評価中。
              {nodes.length > 1 && (
                <span className="notice-sub">（P2Pネットワーク: {nodes.length - 1}台のピアを検出・同期中）</span>
              )}
            </p>
          ) : (
            <p className="notice" role="note">
              <strong>サンプルデータを表示中</strong>
              Agent とはまだ連携していません。PC名・使用率・余剰評価・応答時間などはすべて画面確認用の仮データです。
            </p>
          )}

          {agentConnected && tailscale && tailscale.state !== 'running' && (
            <div className="onboarding-card">
              <div className="onboarding-header">
                <div className="onboarding-icon" aria-hidden="true">
                  {tailscale.state === 'not_installed' ? '⬇' : '🔑'}
                </div>
                <div className="onboarding-main">
                  <h2 className="onboarding-title">
                    {tailscale.state === 'not_installed' && '【P2P接続】Tailscale のインストールが必要です'}
                    {tailscale.state === 'needs_login' && '【認証】Tailscale へのログインが必要です'}
                    {tailscale.state === 'stopped' && '【接続停止】Tailscale が停止しています'}
                    {tailscale.state === 'error' && '【エラー】Tailscale 連携エラー'}
                  </h2>
                  <p className="onboarding-desc">
                    {tailscale.state === 'not_installed' &&
                      'SpareLink はPC同士を安全に直接接続（P2P）して余剰計算資源を共有するために Tailscale を使用します。無料・数クリックで簡単に導入できます。'}
                    {tailscale.state === 'needs_login' &&
                      'Tailscale のアカウント認証が完了していません。下のボタンからブラウザでログイン認証を行うと、同一アカウントのPC同士が自動的に接続されます。'}
                    {tailscale.state === 'stopped' &&
                      'Tailscale サービスが無効または停止しています。Tailscaleアプリまたはコマンドから接続を開始してください。'}
                    {tailscale.state === 'error' && tailscale.message}
                  </p>
                </div>
              </div>

              <div className="onboarding-actions">
                {tailscale.state === 'not_installed' && (
                  <a
                    href="https://tailscale.com/download"
                    target="_blank"
                    rel="noreferrer"
                    className="onboarding-btn onboarding-btn--primary"
                  >
                    Tailscale 公式インストーラーを開く（無料） ↗
                  </a>
                )}
                {tailscale.state === 'needs_login' && (
                  <a
                    href={tailscale.authUrl || 'https://login.tailscale.com'}
                    target="_blank"
                    rel="noreferrer"
                    className="onboarding-btn onboarding-btn--primary"
                  >
                    ブラウザでログイン認証ページを開く ↗
                  </a>
                )}
                {tailscale.state === 'stopped' && (
                  <button
                    type="button"
                    onClick={() => window.location.reload()}
                    className="onboarding-btn onboarding-btn--primary"
                  >
                    接続状況を再確認する
                  </button>
                )}
              </div>

              <div className="onboarding-steps">
                <div className="onboarding-step">
                  <span className="step-num">1</span>
                  <div>
                    <strong>Tailscale を導入</strong>
                    <p>公式サイトからインストーラーを実行（数分で完了）</p>
                  </div>
                </div>
                <div className="onboarding-step">
                  <span className="step-num">2</span>
                  <div>
                    <strong>アカウントでログイン</strong>
                    <p>Google / Microsoft / GitHub 等のアカウントでサインイン</p>
                  </div>
                </div>
                <div className="onboarding-step">
                  <span className="step-num">3</span>
                  <div>
                    <strong>他PCでも同様にログイン</strong>
                    <p>接続したいもう1台のPCでも同じアカウントでTailscaleを起動するだけで、SpareLinkが自動検出・P2P接続を確立します</p>
                  </div>
                </div>
              </div>
            </div>
          )}

          {/* ネットワーク全体の概要値（負荷＋余剰計算資源を両立して提示） */}
          <section className="summary" aria-label="ネットワークの概要値">
            <div className="summary-cell">
              <span className="summary-label">オンラインPC</span>
              <span className="summary-value num">
                {summary.online}
                <small> / {summary.total} 台</small>
              </span>
              <span className="summary-sub">
                {summary.total - summary.online > 0
                  ? `${summary.total - summary.online} 台がオフライン`
                  : '全PCと疎通あり'}
              </span>
            </div>

            <div className="summary-cell">
              <span className="summary-label">平均CPU使用率 (負荷)</span>
              <span className="summary-value num">
                {summary.avgCpuPercent ?? '—'}
                <small> %</small>
              </span>
              <span className={`summary-sub ${summary.highLoadCount > 0 ? 'is-warn' : ''}`}>
                {summary.highLoadCount > 0 ? `高負荷 ${summary.highLoadCount} 台` : '全コア加重平均'}
              </span>
            </div>

            <div className="summary-cell">
              <span className="summary-label">平均メモリ使用率</span>
              <span className="summary-value num">
                {summary.avgMemoryPercent ?? '—'}
                <small> %</small>
              </span>
              <span className="summary-sub">オンラインPCの平均</span>
            </div>

            {/* 本システム最重要：総余剰計算資源（実効割当可能枠） */}
            <div className="summary-cell summary-cell--surplus-highlight">
              <span className="summary-label highlight-label">総余剰計算資源 (割当可能CPU)</span>
              <span className="summary-value highlight-val num">
                {summary.totalAllocatableCpuScore.toLocaleString()}
                <small> pt</small>
              </span>
              <span className="summary-sub highlight-sub">
                {summary.evaluatedNodeCount} 台の評価済みPCから算出
              </span>
            </div>

            <div className="summary-cell summary-cell--surplus-highlight">
              <span className="summary-label highlight-label">総余剰メモリ (割当可能枠)</span>
              <span className="summary-value highlight-val num">
                {summary.totalAllocatableMemoryGb}
                <small> GB</small>
              </span>
              <span className="summary-sub highlight-sub">安全マージン確保済み</span>
            </div>

            <div className="summary-source">
              <SourceTag source={source} />
            </div>
          </section>

          <div className="workspace">
            <div className="col-main">
              <section className="panel panel--topology" aria-labelledby="topology-heading">
                <header className="panel-head">
                  <h2 id="topology-heading">接続トポロジー & 資源マップ</h2>
                  <SourceTag source={source} />
                  <ul className="link-legend" aria-label="接続線の凡例">
                    <li>
                      <span className="link-swatch link-swatch--local" aria-hidden="true" />
                      自PC接続
                    </li>
                    <li>
                      <span className="link-swatch link-swatch--peer" aria-hidden="true" />
                      ピア間接続
                    </li>
                    <li>
                      <span className="link-swatch link-swatch--down" aria-hidden="true" />
                      切断
                    </li>
                  </ul>
                </header>
                <div className="topology-canvas">
                  <Topology
                    nodes={nodes}
                    links={links}
                    localNodeId={localNodeId}
                    selectedId={selectedNode.id}
                    onSelect={setSelectedId}
                  />
                </div>
                <p className="panel-foot">
                  PCを選択すると右側に余剰評価の詳細を表示します。線上の数値はこのPCから見た応答時間です。
                </p>
              </section>

              <section className="panel panel--nodes" aria-labelledby="nodes-heading">
                <header className="panel-head">
                  <h2 id="nodes-heading">PC一覧・負荷と余剰計算資源</h2>
                  <span className="panel-count num">{nodes.length} 台</span>
                  <SourceTag source={source} />
                </header>
                <NodeTable
                  nodes={nodes}
                  localNodeId={localNodeId}
                  selectedId={selectedNode.id}
                  source={source}
                  onSelect={setSelectedId}
                />
              </section>
            </div>

            <div className="col-side">
              <aside className="panel panel--detail" aria-label="選択中のPCの詳細">
                <NodeDetail
                  node={selectedNode}
                  isLocal={selectedNode.id === localNodeId}
                  source={selectedNode.dataSource}
                />
              </aside>

              <section className="panel panel--log" aria-labelledby="log-heading">
                <header className="panel-head">
                  <h2 id="log-heading">資源評価 & システムログ</h2>
                  <SourceTag source={source} />
                </header>
                <ol className="event-log">
                  {events.map((e, i) => (
                    <li key={`${e.time}-${i}`} className={`event event--${e.level}`}>
                      <time className="num">{e.time}</time>
                      <span className="event-node">{e.nodeName}</span>
                      <span className="event-msg">{e.message}</span>
                    </li>
                  ))}
                </ol>
              </section>
            </div>
          </div>
        </main>
      </div>

      <footer className="statusbar">
        <span className="sb-item">
          データ: <strong>{isSample ? 'サンプル' : '実測'}</strong>
        </span>
        <span className="sb-item">Local Agent: {agentConnected ? '接続済み' : '未接続'}</span>
        <span className="sb-item">Tailscale P2P同期: 準備中</span>
        <span className="sb-item">共通Job API: 未実装</span>
      </footer>
    </div>
  )
}

export default App
