# Claude Code プロジェクト情報

## プロジェクト概要
FlexUpdateMc - カスタム改変を保持しながらMinecraftワールドを異なるバージョン間で移行するためのRustベースのツール

## 開発環境
- 言語: Rust (Edition 2024)
- ビルドシステム: Cargo
- プラットフォーム: Linux

## プロジェクト構造
```
src/
├── lib.rs                    # ライブラリエントリーポイント
├── main.rs                   # メインエントリーポイント
├── model.rs                  # データモデル定義
├── service.rs                # サービスモジュール
├── service/
│   ├── chunk_migrator.rs     # チャンク移行トレイト
│   ├── flex_updater.rs       # メイン更新ロジック
│   ├── server_factory.rs     # サーバー作成ファクトリー
│   ├── server_handler.rs     # サーバー操作ハンドラー
│   └── world_handler.rs      # ワールドファイル操作
├── util.rs                   # ユーティリティモジュール
└── util/
    └── dir_overwrite.rs      # ディレクトリ上書き機能
```

## 主要な設計パターン
- トレイトベースの設計による拡張性
- ファクトリーパターンによるサーバーインスタンス作成
- 3つのチャンク比較による差分移行システム

## ビルド・テストコマンド
```bash
# ビルド
cargo build

# リリースビルド
cargo build --release

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