use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=../bot");
    
    let out_dir = env::var("OUT_DIR").unwrap();
    
    // ターゲット環境のWindows用実行ファイル拡張子
    let target = env::var("TARGET").unwrap_or_else(|_| env::var("HOST").unwrap());
    let exe_suffix = if target.contains("windows") { ".exe" } else { "" };
    let bot_binary_name = format!("bot{}", exe_suffix);
    let bot_binary_path = Path::new(&out_dir).join(&bot_binary_name);
    
    // 現在のプロファイル（debug/release）を取得
    let profile = env::var("PROFILE").unwrap();
    
    // botクレートをビルド
    let mut build_args = vec!["build", "-p", "bot"];
    if profile == "release" {
        build_args.push("--release");
    }
    // クロスコンパイル時はターゲットを指定
    if env::var("TARGET").is_ok() && env::var("TARGET").unwrap() != env::var("HOST").unwrap() {
        build_args.push("--target");
        build_args.push(&target);
    }
    
    let output = Command::new("cargo")
        .args(&build_args)
        .current_dir("..")
        .output()
        .expect("Failed to build bot crate");
        
    if !output.status.success() {
        panic!("Failed to build bot crate: {}", String::from_utf8_lossy(&output.stderr));
    }
    
    // ビルドされたbotバイナリをOUT_DIRにコピー
    let bot_source = if env::var("TARGET").is_ok() && env::var("TARGET").unwrap() != env::var("HOST").unwrap() {
        Path::new("..").join("target").join(&target).join(&profile).join(&bot_binary_name)
    } else {
        Path::new("..").join("target").join(&profile).join(&bot_binary_name)
    };
    if bot_source.exists() {
        fs::copy(&bot_source, &bot_binary_path)
            .expect("Failed to copy bot binary");
    } else {
        panic!("Bot binary not found at {:?}", bot_source);
    }
    
    println!("cargo:rustc-env=BOT_BINARY_PATH={}", bot_binary_path.display());
}