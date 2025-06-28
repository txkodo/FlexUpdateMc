# 統合テスト実行手順

## 必要な準備

### 1. Minecraftサーバーjarファイルの準備
```bash
# プロジェクトルートに移動
cd /home/txkodo/proj/txkodo/FlexUpdateMc

# Minecraft サーバーjarをダウンロード（例：1.20.1）
wget https://piston-data.mojang.com/v1/objects/84194a2f286ef7c14ed7ce0090dba59902951553/server.jar

# またはcurlを使用
curl -o server.jar https://piston-data.mojang.com/v1/objects/84194a2f286ef7c14ed7ce0090dba59902951553/server.jar
```

### 2. EULAの同意
```bash
# server.jarを一度実行してeula.txtを生成
java -jar server.jar nogui

# eula.txtを編集してEULAに同意
echo "eula=true" > eula.txt
```

## テスト実行

### 1. 基本的なエラーハンドリングテスト（jarファイル不要）
```bash
cargo test test_server_executor_error_handling
cargo test test_multiple_start_prevention
```

### 2. 実際のサーバー起動テスト（jarファイル必要）
```bash
# ignoreされたテストを実行
cargo test test_server_lifecycle -- --ignored

# または、すべてのテストを実行
cargo test -- --ignored
```

### 3. テストの詳細出力
```bash
# 詳細なログ出力でテスト実行
cargo test test_server_lifecycle -- --ignored --nocapture
```

## 注意事項

- `test_server_lifecycle` は実際のMinecraftサーバーjarファイルが必要
- テスト中にサーバーが25565ポートを使用するため、他のMinecraftサーバーが起動していないことを確認
- テストは自動的にサーバーを停止しますが、異常終了した場合は手動でプロセスを終了してください

## トラブルシューティング

### Javaが見つからない場合
```bash
# Javaのインストール確認
java -version

# パッケージマネージャーでJavaをインストール（Ubuntu/Debian）
sudo apt update
sudo apt install default-jre
```

### ポートが使用中の場合
```bash
# 25565ポートを使用しているプロセスを確認
sudo netstat -tulpn | grep 25565

# または
sudo ss -tulpn | grep 25565
```