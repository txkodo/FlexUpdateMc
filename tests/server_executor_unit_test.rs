use flex_updater_mc::service::server_executor::{MockServerExecutor, ServerExecutionConfig, ServerExecutor};
use std::env;

#[test]
fn test_mock_server_executor_basic_operations() {
    let mut executor = MockServerExecutor::new();
    
    // 初期状態の確認
    assert!(!executor.is_running());
    assert!(executor.commands_sent().is_empty());
    
    // 設定オブジェクトの作成
    let config = ServerExecutionConfig {
        java_path: None,
        jar_file_name: "test.jar".to_string(),
        port: 25565,
        cwd: env::current_dir().expect("Failed to get current directory"),
    };
    
    // サーバー起動テスト
    assert!(executor.start(config).is_ok());
    assert!(executor.is_running());
    
    // コマンド送信テスト
    assert!(executor.send_command("say Hello").is_ok());
    assert!(executor.send_command("time set day").is_ok());
    
    // 送信されたコマンドの確認
    let commands = executor.commands_sent();
    assert_eq!(commands.len(), 2);
    assert_eq!(commands[0], "say Hello");
    assert_eq!(commands[1], "time set day");
    
    // サーバー停止テスト
    assert!(executor.stop().is_ok());
    assert!(!executor.is_running());
}

#[test]
fn test_mock_server_executor_already_running_error() {
    let mut executor = MockServerExecutor::new();
    
    let config = ServerExecutionConfig {
        java_path: None,
        jar_file_name: "test.jar".to_string(),
        port: 25565,
        cwd: env::current_dir().expect("Failed to get current directory"),
    };
    
    // 最初の起動は成功
    assert!(executor.start(config).is_ok());
    assert!(executor.is_running());
    
    // 2回目の起動は失敗
    let config2 = ServerExecutionConfig {
        java_path: None,
        jar_file_name: "test2.jar".to_string(),
        port: 25566,
        cwd: env::current_dir().expect("Failed to get current directory"),
    };
    
    let result = executor.start(config2);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("already running"));
}

#[test]
fn test_mock_server_executor_command_without_running() {
    let mut executor = MockServerExecutor::new();
    
    // サーバーが起動していない状態でコマンド送信
    let result = executor.send_command("test command");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("not running"));
}

#[test]
fn test_mock_server_executor_with_start_failure() {
    let mut executor = MockServerExecutor::with_start_failure();
    
    let config = ServerExecutionConfig {
        java_path: None,
        jar_file_name: "test.jar".to_string(),
        port: 25565,
        cwd: env::current_dir().expect("Failed to get current directory"),
    };
    
    // 起動が失敗することを確認
    let result = executor.start(config);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Mock start failure"));
    assert!(!executor.is_running());
}

#[test]
fn test_mock_server_executor_with_stop_failure() {
    let mut executor = MockServerExecutor::with_stop_failure();
    
    let config = ServerExecutionConfig {
        java_path: None,
        jar_file_name: "test.jar".to_string(),
        port: 25565,
        cwd: env::current_dir().expect("Failed to get current directory"),
    };
    
    // 起動は成功するが停止で失敗
    assert!(executor.start(config).is_ok());
    assert!(executor.is_running());
    
    let result = executor.stop();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Mock stop failure"));
}

#[test]
fn test_mock_server_executor_with_command_failure() {
    let mut executor = MockServerExecutor::with_command_failure();
    
    let config = ServerExecutionConfig {
        java_path: None,
        jar_file_name: "test.jar".to_string(),
        port: 25565,
        cwd: env::current_dir().expect("Failed to get current directory"),
    };
    
    // 起動は成功するがコマンド送信で失敗
    assert!(executor.start(config).is_ok());
    assert!(executor.is_running());
    
    let result = executor.send_command("test command");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Mock command failure"));
}

#[test]
fn test_mock_server_executor_clear_commands() {
    let mut executor = MockServerExecutor::new();
    
    let config = ServerExecutionConfig {
        java_path: None,
        jar_file_name: "test.jar".to_string(),
        port: 25565,
        cwd: env::current_dir().expect("Failed to get current directory"),
    };
    
    assert!(executor.start(config).is_ok());
    
    // コマンドを送信
    assert!(executor.send_command("command1").is_ok());
    assert!(executor.send_command("command2").is_ok());
    assert_eq!(executor.commands_sent().len(), 2);
    
    // コマンド履歴をクリア
    executor.clear_commands();
    assert!(executor.commands_sent().is_empty());
    
    // クリア後も新しいコマンドは記録される
    assert!(executor.send_command("command3").is_ok());
    assert_eq!(executor.commands_sent().len(), 1);
    assert_eq!(executor.commands_sent()[0], "command3");
}

#[test]
fn test_mock_server_executor_default() {
    let executor = MockServerExecutor::default();
    assert!(!executor.is_running());
    assert!(executor.commands_sent().is_empty());
}