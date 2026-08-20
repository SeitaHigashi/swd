# SWD (Seita Windows Dashboard)

Windows 11 常駐デスクトップウィジェット。Wallpaper Engine の壁紙の上・デスクトップ
アイコンの下というレイヤーに、グラスモーフィズム調のシステム情報ダッシュボード
（時計 / CPU・メモリ（グラフ付き） / ネットワーク（グラフ付き） / 再生中メディア）
を表示する。

開発の経緯・非自明な判断の理由は [docs/history.md](docs/history.md) に、
コーディングエージェント向けの早見表は [CLAUDE.md](CLAUDE.md) にまとめてある
（いずれも英語）。

## セットアップ

### 必要なツール

| ツール | バージョン目安 | 備考 |
| --- | --- | --- |
| Node.js | 20 以上 | 開発確認時は v26 系を使用 |
| npm | 10 以上 | |
| Rust (stable, MSVC toolchain) | 1.75 以上 | `rustup default stable-x86_64-pc-windows-msvc` |
| Tauri CLI | `@tauri-apps/cli` v2 | `npm install` で devDependency として入る |

Windows 11 + WebView2 ランタイム（Windows 11 は標準搭載）が前提。**Windows専用**
（Win32 API に直接依存しているため、macOS/Linux では `window_layer` /
`hit_test` モジュールがビルドから除外され、デスクトップ背面固定とクリック
スルーが動作しない）。

### インストール

```bash
npm install
```

## 実行・ビルド

```bash
# 開発モード（ホットリロードではなく、Rust側の変更検知で自動再ビルド・再起動）
npm run dev

# リリースビルド
npm run build
```

`npm run dev` は `src-tauri/` 配下の変更を検知すると自動で再コンパイル・
再起動する。フロントエンド (`src/`) はバンドラーを使わない素の HTML/CSS/JS
構成のため、`main.js` 側の変更を反映するにはウィンドウの再読み込み（アプリ
再起動、または再ビルドトリガー）が必要な場合がある。

## アーキテクチャ概要

```
src-tauri/src/
├── lib.rs          # アプリ起動・モニタ選択・ウィンドウ初期化・モジュール配線
├── window_layer.rs # デスクトップ背面レイヤー固定（Windows専用）
├── hit_test.rs      # クリックスルー制御（Windows専用）
├── system_info.rs  # CPU/メモリ/ネットワーク情報の定期送信（全OS共通）
└── media.rs           # 再生中メディア情報・再生操作（Windows専用）

src/
├── index.html
├── style.css        # グラスモーフィズムのベーススタイル
├── main.js           # カードのドラッグ・位置保存・クリックスルー同期
└── modules/
    ├── clock.js
    ├── system-monitor.js  # CPU/メモリカード（sparkline.js を利用）
    ├── network.js           # 送受信速度カード（sparkline.js を利用）
    ├── media.js               # 再生中メディアカード
    ├── sparkline.js            # 折れ線グラフ描画の共通ロジック
    └── layout.js                # カード位置の localStorage 永続化
```

新しいカードを追加する場合は `src/modules/` に独立したモジュールを追加し、
`index.html` に `.glass-card` を1枚足し、`main.js` から呼び出すだけでよい
（クリックスルー・ドラッグ・位置保存は `.glass-card` クラスと `id` があれば
自動的に効く）。

### デスクトップ背面レイヤーの実装方式

`tauri-plugin-wallpaper`（壁紙と同じ位置に配置＝Wallpaper Engineと競合）や
`tauri-plugin-desktop-underlay`（正しいレイヤーだが全クリック操作が無効化
される）は要件（壁紙の上・アイコンの下・かつクリック可能）を満たせなかった
ため、**Win32 API を直接叩く自前実装**を採用している（Rainmeter の
"Send to Desktop" と同じ考え方）。

- ウィンドウは `SetParent` で WorkerW の子にはしない（子にすると入力操作が
  一切効かなくなる）。あくまで独立したトップレベルウィンドウのまま、
  `SetWindowPos` でデスクトップアイコンを持つウィンドウのすぐ後ろ（＝壁紙
  WorkerW のすぐ手前）に z-order だけ挿入している（`window_layer.rs`）。
- 3秒ごとにこの z-order 挿入をやり直すウォッチャースレッドを起動している。
  explorer.exe が WorkerW を再生成するタイミング（ディスプレイ設定変更や
  explorer 再起動など）で配置がずれるのを防ぐため。
- クリックスルーは「ウィンドウ全体を ignore_cursor_events にした状態から
  Webview 自身のマウスイベントで復帰する」方式だと、一度クリックスルーに
  すると入力イベントが来なくなり復帰できないデッドロックに陥る。そのため
  フロントエンドがカードの画面座標を Rust 側に送り（`set_hit_regions`
  コマンド）、Rust 側の別スレッドが `GetCursorPos` を独立にポーリングして
  カード領域との重なりだけで `ignore_cursor_events` を切り替えている
  （`hit_test.rs`）。

### 再生中メディアカードの仕組み

Windows の System Media Transport Controls（Win+G やロック画面のメディア
オーバーレイと同じ API）から現在再生中のタイトル・アーティスト・サムネイル・
再生状態を取得し、再生/一時停止・前後スキップを実行できる（`media.rs`）。
WinRT の呼び出しは呼び出し元スレッドで COM 初期化が必要なため、ポーリングと
操作コマンドの両方を専用の1スレッドに集約し、コマンドは `mpsc` チャンネル
経由でそのスレッドに渡している。

### 既知の制限

- **Win+D（デスクトップ表示）でウィジェットが最小化される**: `SetParent`
  を使わない方式のトレードオフとして、通常のアプリウィンドウと同様に
  Win+D で最小化される（デスクトップの一部として扱われていないため）。
  常時表示を維持したい場合は `WM_WINDOWPOSCHANGING` をサブクラス化して
  Win+D の移動リクエストを横取りする対応が別途必要（未実装）。
- **GPU使用率は未実装**: `sysinfo` crate は GPU 情報を扱わないため、CPU・
  メモリ・ネットワークのみ対応。実装する場合は PDH (`GPU Engine`
  パフォーマンスカウンター) や NVML 等ベンダー固有 SDK の追加調査が必要。
- **マルチモニタは「いちばん右のモニター」固定**: `src-tauri/src/lib.rs`
  の `select_target_monitor` 関数で選択ロジックを1箇所にまとめてあるので、
  プライマリモニタや特定インデックスへの変更は同関数を書き換えるだけで
  済む設計にしてある。
- **カード位置の保存先は WebView の `localStorage`**: アプリの
  ユーザーデータディレクトリに紐づいて永続化されるが、WebView のプロファ
  イルを消すと失われる。将来的に設定ファイル（JSON）へ移す場合は
  `src/modules/layout.js` の実装を差し替えるだけでよい。
