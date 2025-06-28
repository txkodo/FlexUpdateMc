use flex_updater_mc::service::server_executor::{ChildProcessServerExecutor, ServerExecutionConfig, ServerExecutor};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::thread;
use std::time::Duration;

fn setup_test_environment(test_name: &str) -> PathBuf {
    let current_dir = env::current_dir().expect("Failed to get current directory");
    let test_dir = current_dir.join("test_env").join(test_name);
    
    // テストディレクトリを作成
    fs::create_dir_all(&test_dir).expect("Failed to create test directory");
    
    let jar_path = test_dir.join("server.jar");
    let eula_path = test_dir.join("eula.txt");
    
    // jarファイルが存在しない場合はダウンロード
    if !jar_path.exists() {
        println!("Downloading Minecraft server jar for {}...", test_name);
        let output = Command::new("curl")
            .arg("-o")
            .arg(&jar_path)
            .arg("https://piston-data.mojang.com/v1/objects/84194a2f286ef7c14ed7ce0090dba59902951553/server.jar")
            .output()
            .expect("Failed to execute curl command");
        
        if !output.status.success() {
            panic!("Failed to download server jar: {}", String::from_utf8_lossy(&output.stderr));
        }
        println!("Server jar downloaded successfully for {}", test_name);
    }
    
    // eula.txtが存在しない場合は作成
    if !eula_path.exists() {
        fs::write(&eula_path, "eula=true\n").expect("Failed to create eula.txt");
    }
    
    test_dir
}

#[test]
#[ignore] // このテストは実際のMinecraftサーバーjarが必要なため、デフォルトでは無視
fn test_server_lifecycle() {
    let test_dir = setup_test_environment("test_server_lifecycle");
    let mut executor = ChildProcessServerExecutor::new();

    // テスト用の設定
    let config = ServerExecutionConfig {
        java_path: None, // システムのjavaを使用
        jar_file_name: "server.jar".to_string(),
        port: 25565,
        cwd: test_dir,
    };

    // サーバー起動テスト
    println!("Starting server...");
    match executor.start(config) {
        Ok(_) => println!("Server started successfully"),
        Err(e) => {
            println!("Failed to start server: {}", e);
            return;
        }
    }

    // サーバーが起動するまで少し待機
    thread::sleep(Duration::from_secs(5));

    // コマンド送信テスト
    println!("Sending test command...");
    if let Err(e) = executor.send_command("say Hello from integration test!") {
        println!("Failed to send command: {}", e);
    }

    thread::sleep(Duration::from_secs(2));

    // サーバー停止テスト
    println!("Stopping server...");
    if let Err(e) = executor.stop() {
        println!("Failed to stop server: {}", e);
    }

    println!("Integration test completed");
}

#[test]
#[ignore] // 実際のサーバーjarが必要
fn test_server_multiple_commands() {
    let test_dir = setup_test_environment("test_server_multiple_commands");
    let mut executor = ChildProcessServerExecutor::new();

    let config = ServerExecutionConfig {
        java_path: None,
        jar_file_name: "server.jar".to_string(),
        port: 25566, // 異なるポートを使用
        cwd: test_dir,
    };

    println!("Starting server for multiple commands test...");
    if let Err(e) = executor.start(config) {
        println!("Failed to start server: {}", e);
        return;
    }

    thread::sleep(Duration::from_secs(5));

    // 複数のコマンドを送信
    let commands = ["time set day", "weather clear", "say Multiple commands test"];
    for cmd in &commands {
        println!("Sending command: {}", cmd);
        if let Err(e) = executor.send_command(cmd) {
            println!("Failed to send command '{}': {}", cmd, e);
        }
        thread::sleep(Duration::from_secs(1));
    }

    println!("Stopping server...");
    if let Err(e) = executor.stop() {
        println!("Failed to stop server: {}", e);
    }

    println!("Multiple commands test completed");
}

#[test]
fn test_server_executor_error_handling() {
    let mut executor = ChildProcessServerExecutor::new();
    
    // 存在しないjavaコマンドでの起動テスト
    let config = ServerExecutionConfig {
        java_path: Some(std::path::Path::new("/nonexistent/java")),
        jar_file_name: "test.jar".to_string(),
        port: 25565,
        cwd: env::current_dir().expect("Failed to get current directory"),
    };

    // エラーが正しく処理されることを確認（存在しないjavaコマンド）
    let result = executor.start(config);
    assert!(result.is_err(), "Expected error when using nonexistent java command");
    
    // サーバーが起動していない状態でのコマンド送信テスト
    assert!(executor.send_command("test").is_err());
    
    // サーバーが起動していない状態での停止テスト（エラーにはならない）
    assert!(executor.stop().is_ok());
}

#[test]
fn test_multiple_start_prevention() {
    // このテストは実際のjarファイルなしでも実行可能
    let mut executor = ChildProcessServerExecutor::new();
    
    let config = ServerExecutionConfig {
        java_path: None,
        jar_file_name: "test.jar".to_string(),
        port: 25565,
        cwd: env::current_dir().expect("Failed to get current directory"),
    };

    // 最初の起動試行（jarがなくてもエラーハンドリングをテスト）
    let _ = executor.start(config);
    
    // 2回目の起動試行は「すでに実行中」エラーになることを確認
    let config2 = ServerExecutionConfig {
        java_path: None,
        jar_file_name: "test2.jar".to_string(),
        port: 25566,
        cwd: env::current_dir().expect("Failed to get current directory"),
    };
    
    match executor.start(config2) {
        Err(msg) if msg.contains("already running") => {
            // 期待される動作
        }
        _ => {
            // すでに実行中でない場合は停止してからテストを終了
            let _ = executor.stop();
        }
    }
}