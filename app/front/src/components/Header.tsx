import type { PeerNode, TailscaleTelemetry } from '../types'

interface HeaderProps {
  localNode?: PeerNode
  agentConnected: boolean
  tailscale?: TailscaleTelemetry | null
  capturedAt?: string | null
  onClusterUpdate: () => void
  isUpdating: boolean
}

export function Header({
  localNode,
  agentConnected,
  tailscale,
  capturedAt,
  onClusterUpdate,
  isUpdating,
}: HeaderProps) {
  return (
    <header className="titlebar">
      <div className="brand">
        <span className="brand-mark" aria-hidden="true" />
        <span className="brand-name">SpareLink</span>
        <span className="brand-version">v0.1.8</span>
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
          <span className="ts-value num">{capturedAt ?? '—'}</span>
        </span>
        {agentConnected && (
          <button
            type="button"
            className="update-all-btn"
            onClick={onClusterUpdate}
            disabled={isUpdating}
            title="クラスタ内の全PCへ最新バイナリの自動更新・再起動シグナルを一括送信します"
          >
            {isUpdating ? '更新シグナル送信中…' : '⚡ 全PCを一括自動更新'}
          </button>
        )}
      </div>
    </header>
  )
}
