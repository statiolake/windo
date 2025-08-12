# windo - WSL-Windows 相互互換性ツール

## 概要
WSL環境からWindows環境のツールをシームレスに実行するためのツールです。
例: `win npm run dev` でWindows版のnpmを実行

## 用途
- GUI プログラムやハードウェア直接操作が必要なWindows ネイティブバイナリの実行
- WSL 2をシェルとして使いつつ、Windows CLI プログラムとの連携開発

## 技術的背景

### Windows バイナリから Linux バイナリへの移行
- **従来**: Windows .exe として作成し、WSL から win.exe として実行
  - メリット: CreateProcess() API でバッチファイルがシームレス実行可能
  - デメリット: クロスコンパイル必要、環境変数扱いが困難

- **新方式**: Linux バイナリとして作成
  - メリット: ネイティブLinux環境、環境変数扱いが容易
  - デメリット: .bat/.cmd は cmd /c プレフィックスが必要、エスケープが複雑

### UNC パス対応
- バッチファイル実行時のみ UNC パス確認が必要 (cmd.exe が UNC パスで起動不可)
- WSL 環境では readlink の結果が `/mnt/{drive_letter}` 以外の場合を UNC パスと判断
- 代替手段: wslpath コマンドの利用も可能

## 実装方針
1. Linux バイナリとしてビルド
2. .exe ファイルは直接実行
3. .bat/.cmd ファイルは cmd /c でラップして実行
4. UNC パス検出ロジックをWSL環境向けに実装
5. 適切なエスケープ処理を実装