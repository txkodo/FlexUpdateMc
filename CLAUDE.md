# Claude Code プロジェクト情報

## プロジェクト概要
FlexUpdateMc - カスタム改変を保持しながらMinecraftワールドを異なるバージョン間で移行するためのRustベースのツール

## 開発環境
- 言語: Rust (Edition 2024)
- ビルドシステム: Cargo (ワークスペース構成)
- プラットフォーム: Linux

## プロジェクト構造
Cargoワークスペースとして構成：
```
FlexUpdateMc/
├── Cargo.toml                # ワークスペース設定
├── bin/                      # メインのFlexUpdateMcアプリケーション
│   ├── Cargo.toml
│   ├── src/
│   │   ├── lib.rs            # ライブラリエントリーポイント
│   │   ├── main.rs           # メインエントリーポイント
│   │   ├── model.rs          # データモデル定義
│   │   ├── service.rs        # サービスモジュール
│   │   ├── service/
│   │   │   ├── bot_handler.rs        # ボット操作ハンドラー
│   │   │   ├── chunk_migrator.rs     # チャンク移行トレイト
│   │   │   ├── flex_updater.rs       # メイン更新ロジック
│   │   │   ├── server_executor.rs    # サーバー実行制御
│   │   │   └── server_handler.rs     # サーバー操作ハンドラー
│   │   ├── util.rs           # ユーティリティモジュール
│   │   └── util/
│   │       └── dir_overwrite.rs      # ディレクトリ上書き機能
│   └── tests/                # 統合テスト
└── bot/                      # Minecraftボット関連機能
    ├── Cargo.toml
    └── src/
        └── main.rs
```

## 主要な設計パターン
- トレイトベースの設計による拡張性
- ファクトリーパターンによるサーバーインスタンス作成
- 3つのチャンク比較による差分移行システム

## ビルド・テストコマンド
```bash
# 全体をビルド
cargo build

# リリースビルド
cargo build --release

# 特定のクレートをビルド
cargo build -p flex_updater_mc
cargo build -p bot

# テスト実行
cargo test

# フォーマット
cargo fmt

# Lint
cargo clippy
```

## 開発メモ
- 現在は基本的なアーキテクチャの実装段階
- 具体的なMinecraftデータ処理（NBT、リージョンファイル）は未実装
- サーバー統合機能も今後の開発予定
- 応答は日本語で行う

## 依存関係
現在は標準ライブラリのみ使用。今後追加予定：
- NBTデータパーサー
- リージョンファイル（.mca）処理
- Minecraftサーバー統合ライブラリ

## 統合テスト
単体テストが作りづらい環境のため、実際にjavaやbotを起動しての統合テストをメインに開発する

統合テストを実行する際、`test_env/テスト名`のディレクトリを使用し、リポジトリが散らからないようにすること