# SpareLink

[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-2024_edition-orange.svg)](https://www.rust-lang.org/)
[![React](https://img.shields.io/badge/react-19-cyan.svg)](https://react.dev/)

**SpareLink** は、自宅や研究室、オフィスなどの複数台のPCをプライベートメッシュ（Tailscale、WireGuard、ローカルLAN）で相互に自動接続し、各PCの**「真の余剰計算資源（CPU・メモリ）」**をリアルタイムに評価・集約するP2P分散リソースコーディネーターです。

外部ソフトウェア（Blenderの分散レンダリング、VS Code拡張、CI/CD分散ビルド、バッチJobスケジューラなど）から WebSocket API 経由でクラスタ全体の余剰能力を問い合わせ、余っているマシンへタスクを動的配分するための基盤として設計されています。

---

## 主な特徴

- **実効余剰能力の動的評価（単なる「100% - 使用率」ではない）**:
  CPU使用率の空きだけでなく、各PCのアーキテクチャ・並列処理性能（起動時の高速ベンチマーク）、現在負荷、安全マージン（CPU 20%、メモリ 4GB等）を総合考慮し、**「ホストPCの操作快適性を損なわずに割り当て可能な実効ポイント（pt）」**を算出します。
- **高速CPUマイクロベンチマーク（頭打ちゼロ設計）**:
  割り算（div）のシリアル待機による頭打ちを排除し、命令レベル並列性（ILP）・32KBキャッシュ走査・FPU積和演算を組み合わせた独自ワークロードを搭載。約0.08秒で完了し、旧世代CPU（例: i7-10700K）と最新CPU（例: Ryzen 7 7800X3D）のアーキテクチャ差・IPCを素直にスコアへ反映します。
- **単独exe・ゼロ依存で動作（Windowsネイティブプローブ）**:
  エージェントは単一の実行ファイル（約2MB）として完結。サブPC側で Python や Node.js をインストールすることなく、Windows OS の Native API（`GlobalMemoryStatusEx`, `GetSystemTimes`）から直接負荷とメモリを自動取得します。
- **プラグアブルな通信方式（Tailscale / WireGuard / 固定IP）**:
  - **Tailscale**: 同一Tailnet内のオンライン端末をゼロコンフィグで自動検出。
  - **WireGuard / Static**: 設定ファイルや環境変数で指定したピアと対等に接続。
  - **LAN**: サブネット内の端末探索。
  通信層は抽象化されており、設定変更だけでシームレスに切り替え可能です。
- **オープンな拡張機能アーキテクチャ**:
  ローカルの WebSocket API（`ws://127.0.0.1:3001`）へ接続するだけで、クラスタ全体の余剰CPU・メモリ状況をストリーミング受信可能。外部ソフト側から容易に連携プラグインを作成できます。

---

## ゼロ設定ですぐ使える P2P 接続 (Tailscale 連携 & ガイド機能)

SpareLink は、ルーターのポート開放や固定IP設定などの複雑なネットワーク設定を不要にするため、**Tailscale** を標準トランスポートとして採用しています。

### 初回起動時の自動オンボーディング
`sparelink-agent.exe` をダブルクリックするだけで直ちに動作します。もしPCに Tailscale が入っていない場合やログインしていない場合でも、迷わず使えるよう以下の親切設計を備えています：

1. **自動検出 & ブラウザ起動**:
   - **未インストール時**: ターミナルに導入案内を表示し、自動的にデフォルトブラウザで Tailscale 公式ダウンロードページを開きます。
   - **未ログイン時**: ターミナルに認証案内を表示し、自動的にブラウザで Tailscale ログイン認証ページを開きます。
2. **Webダッシュボードでの視覚的サポート**:
   - ダッシュボード画面上部に Tailscale の接続状態バッジおよび、ワンクリックで認証ページへ飛べる案内カードが表示されます。
3. **シームレスな自動復旧**:
   - ブラウザ上で認証を完了させると、**エージェントを再起動することなくバックグラウンドで自動検知**し、直ちに同一アカウントの他PCとP2Pメッシュ接続を確立します。
4. **複数PCでの利用手順**:
   - メインPCともう1台のPCそれぞれで `sparelink-agent.exe` を起動し、同じ Tailscale アカウント（GoogleやGitHub等）でログインするだけで完了です。

```
SpareLink/
├── app/
│   ├── agent/
│   │   ├── rust/             # コアP2Pエージェントデーモン (Rust)
│   │   │   ├── src/
│   │   │   │   ├── bin/
│   │   │   │   │   └── bench_cli.rs   # 独立ベンチマークCLI (sparelink-bench)
│   │   │   │   ├── bench.rs       # 高速CPUマイクロベンチマーク
│   │   │   │   ├── config.rs      # 設定ファイルローダー (sparelink.toml)
│   │   │   │   ├── discovery/     # ピア探索プロバイダー (Tailscale / WireGuard / Static)
│   │   │   │   ├── evaluator.rs   # 余剰計算資源の算出ロジック
│   │   │   │   ├── native_probe.rs# Windows API直接監視プローブ (ゼロ依存)
│   │   │   │   ├── p2p/           # P2Pハートビート・状態同期マネージャー
│   │   │   │   └── main.rs        # WebSocketサーバー & メインループ
│   │   │   └── Cargo.toml
│   │   └── python/           # 任意で利用可能なPython監視スクリプト
│   └── front/                # WebダッシュボードUI (React 19 + TypeScript + Vite)
├── sparelink.toml            # 統合設定ファイル
├── requirements.txt          # Python用依存関係（Pythonプローブ使用時のみ）
└── README.md
```

---

## 設定ファイル (`sparelink.toml`)

動作設定はすべてルートまたは実行ファイルと同階層の `sparelink.toml` に一元化されています。設定ファイルが存在しない場合は自動的に安全なデフォルト値で動作します。

```toml
# SpareLink 統合設定ファイル (sparelink.toml)

[agent]
# ノード表示名（空文字の場合はホスト名を自動検出）
node_name = ""
# 待受ホストIPアドレス
host = "0.0.0.0"
# 待受ポート番号
port = 3001
# ローカル実測値の配信間隔 (ミリ秒)
broadcast_interval_ms = 2000

[discovery]
# ピア探索方式 ("tailscale" | "wireguard" | "static" | "lan")
transport = "tailscale"
# 外部ピア一覧JSONファイルのパス（wireguard/staticモード用、省略可）
# config_path = "wireguard_peers.json"
# ピアの自動スキャン間隔 (秒)
scan_interval_secs = 10
# P2Pハートビート（応答遅延・余剰資源プローブ）の間隔 (秒)
probe_interval_secs = 4
# 固定ピアの指定（例: ["100.64.0.2:3001", "192.168.1.50:3001"]）
static_peers = []

[evaluator]
# 余剰割当を0ptとして安全停止するCPU使用率閾値 (%)
high_load_threshold = 80.0
# ホストPCの操作快適性を保つためのCPU安全マージン (%)
safety_headroom = 20.0
# OS安定稼働のために常時確保する空き物理メモリ (GB)
min_memory_margin_gb = 4.0

[benchmark]
# 起動時にCPUベンチマークを自動実行するかどうか
enabled = true
# 1コアあたりの演算反復回数（デフォルト: 20000000）
iterations_per_core = 20000000
# スコア手動固定値（0 で自動計測）
manual_score = 0
```

### 環境変数による上書き
コンテナ環境やスクリプト実行時、環境変数で設定を一時上書きできます：

| 環境変数 | 説明 | デフォルト |
| :--- | :--- | :--- |
| `SPARELINK_PORT` | 待受ポート番号 | `3001` |
| `SPARELINK_TRANSPORT` | 通信方式 (`tailscale`, `wireguard`, `static`, `lan`) | `tailscale` |
| `SPARELINK_NODE_NAME` | ノード名の手動指定 | ホスト名 |
| `SPARELINK_CONFIG_PATH` | WireGuard用設定ファイルパス | なし |
| `SPARELINK_PEERS` | カンマ区切りの固定ピア一覧 (`"10.0.0.2:3001,10.0.0.3:3001"`) | なし |

---

## 他ソフト・外部ツールのための拡張機能開発ガイド (API仕様)

SpareLink は、外部ソフトウェア（ジョブディスパッチャ、レンダーキュー、ビルドツール等）がクラスタの余剰資源を自由に取得できるよう、ローカルの WebSocket エンドポイント（`ws://127.0.0.1:3001`）を開放しています。

接続すると、リアルタイムにクラスタ全体の余剰能力と全ピアの状態が JSON 形式で配信されます。

### 配信される JSON データ構造例
```json
{
  "pc_name": "DESKTOP-MAIN",
  "os": "Windows",
  "cpu_cores": 16,
  "cpu_percent": 18.2,
  "memory_percent": 41.5,
  "memory_total_gb": 64.0,
  "memory_available_gb": 37.4,
  "cpu_benchmark_score": 30351,
  "surplus": {
    "status": "evaluated",
    "confidence": 0.95,
    "allocatableCpuScore": 18750,
    "allocatableMemoryGb": 33.4,
    "algorithmVersion": "sparelink_v1",
    "statusNote": "Normal operation. Safety headroom: CPU 20%, RAM 4GB."
  },
  "peers": [
    {
      "id": "node:winserver",
      "name": "WinServer",
      "address": "100.126.20.110",
      "status": "online",
      "latencyMs": 14,
      "cpuCores": 32,
      "cpuPercent": 8.5,
      "memoryAvailableGb": 97.2,
      "cpuBenchmarkScore": 48200,
      "surplus": {
        "status": "evaluated",
        "allocatableCpuScore": 34460,
        "allocatableMemoryGb": 93.2
      },
      "transport": "Tailscale"
    }
  ]
}
```

### 拡張機能の実装例（Python による15行クライアント）
他ソフトのバックグラウンド処理やPythonスクリプトから、数行で余剰計算資源を取得できます：

```python
import asyncio
import json
import websockets

async def watch_sparelink():
    uri = "ws://127.0.0.1:3001"
    async with websockets.connect(uri) as ws:
        async for raw_message in ws:
            data = json.loads(raw_message)
            local_cpu = data["surplus"]["allocatableCpuScore"]
            local_mem = data["surplus"]["allocatableMemoryGb"]
            peers = [p for p in data.get("peers", []) if p["status"] == "online"]
            
            print(f"[SpareLink] 自PC余剰: CPU {local_cpu} pt, メモリ {local_mem:.1f} GB")
            print(f"[SpareLink] オンライン接続ピア: {len(peers)} 台")

asyncio.run(watch_sparelink())
```

---

## ビルドと起動方法

### 前提条件
- **Rust**: 1.80 以上 (2024 edition 対応)
- **Node.js**: 20 以上 (Webダッシュボード表示時のみ)

### 1. エージェントのビルドと起動
```bash
cd app/agent/rust
cargo build --release
```
ビルドされた単独バイナリは `app/agent/rust/target/release/sparelink-agent.exe` に生成されます。
（ビルド済みバイナリは GitHub Releases からもダウンロード可能です）

```bash
# 起動（同一階層の sparelink.toml を自動読込）
./sparelink-agent.exe
```

### 2. 独立ベンチマークCLIの利用（他システムでの単体流用）
`sparelink-bench.exe` は、SpareLink の通信やデーモン機能を起動せず、**「純粋なCPU性能計測のみ」を超高速（約0.07秒）で行う独立CLIツール**です（ファイルサイズ約250KB、外部依存DLLなし）。他システムやスクリプトから自由に再利用できます。

```bash
# 通常実行（全論理コア計測）
./sparelink-bench.exe

# 外部ツール連携用（JSON出力）
./sparelink-bench.exe --json
# -> {"cores":16,"elapsed_seconds":0.083,"mops":27106.4,"score":27106}

# シェルスクリプト・変数取得用（スコア数値のみ出力）
SCORE=$(./sparelink-bench.exe --score-only)
echo $SCORE  # -> 31058

# 特定コア数での限定測定
./sparelink-bench.exe -c 4
```

### 3. Webダッシュボードの起動
```bash
cd app/front
npm install
npm run dev
```
ブラウザで `http://localhost:5173` を開くと、トポロジー接続図・余剰計算資源メーター・ピア一覧がリアルタイムに表示されます。

---

## ライセンス

本プロジェクトは [MIT License](LICENSE) の下で公開されています。商用・非商用問わず自由に利用・改変・再配布が可能です。
