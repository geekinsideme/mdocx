# mdocx

`mdocx` は、Markdown と Word ドキュメント（DOCX）を双方向に相互変換するための純粋な Rust 製 CLI アプリケーションです。

## 主な機能

- **双方向変換**: `.md` から `.docx`、または `.docx` から `.md` への相互変換。
- **自動フォーマット検出**: 入力ファイルの拡張子に基づいて適切な変換を自動的に判定。
- **更新日時の引き継ぎ**: 変換後、入力ファイルの更新日時を出力ファイルへコピー。
- **幅広い記法サポート**:
  - 見出し（H1 〜 H6）
  - 段落および文字装飾（**太字**、*斜体*、~~打ち消し線~~）
  - インラインコードおよびコードブロック（等幅フォント、背景網掛け）
  - 引用ブロック（インデントと斜体）
  - リスト（ネスト対応した箇条書きおよび番号付きリスト）
  - ハイパーリンク
  - GFM テーブル（ヘッダーと各セルの構造化）
- **クロスプラットフォーム対応**: Windows, macOS, Linux で動作。
- **自動リリース**: タグ（`v*`）プッシュ時に GitHub Actions が自動で Windows x64（`.exe`）および Linux x64（`tar.gz`）をビルドして GitHub Releases へ投稿。

## インストールとビルド

Rust のビルドツールである `cargo` が必要です。

```bash
# クローン後にビルド
cargo build --release

# 生成されたバイナリの確認
./target/release/mdocx --version
```

## 使い方

### 基本コマンド

最も単純な使用例では、入力ファイルのみを指定します。出力ファイル名は自動的に拡張子を変更して生成されます。

```bash
# markdown から docx への変換（output.docx が作成されます）
mdocx input.md

# docx から markdown への変換（input.md が作成されます）
mdocx input.docx
```

### 明示的な出力先の指定

`-o` / `--out` は**単一の出力ファイル**を指定します。

```bash
mdocx input.md -o custom_output.docx
```

複数入力・ワイルドカード・ディレクトリ指定時に `-o` を使うと、結果は1つの出力ファイルにまとめられます。
各入力ファイルの境界には次のヘッダーが入ります。

```text
###  orgfilepath
```

```bash
mdocx a.md b.md -o merged.docx
```

`-d` / `--directory` は**ファイルごとの出力ディレクトリ**を指定します。
このときは入力側の相対ディレクトリ構造を保って出力されます。

```bash
mdocx a.md b.md -d out_dir

# 例: src/sub/a.c -> out_dir/src/sub/a.docx
mdocx src -f c -r -d out_dir
```

### フォーマットの明示的指定

拡張子が特殊な場合など、`-f`/`--from` または `-t`/`--to` フラグを使用して変換元/変換先のフォーマットを明示できます。

```bash
mdocx input_file_no_ext -f md -t docx
```

`-f` / `--from` は複数指定できます（特にワイルドカード・ディレクトリ入力時のフィルタ用途）。

- `-f md` / `-f markdown` は Markdown 扱い
- `-f docx` は DOCX 扱い
- それ以外（例: `-f txt`, `-f log`, `-f c`, `-f h`, `-f rs` など）は **PlainText（.txt 相当）** として扱います

```bash
# .c と .h を対象にする
mdocx src -f c -f h
```

### 複数ファイルの一括変換

複数ファイルを引数で渡すと、すべて変換します。

```bash
mdocx a.docx b.docx c.docx
```

### Windows のワイルドカード指定

ワイルドカードを使う場合は、`-f` / `--from` で対象フィルタを指定してください（複数指定可）。

```bash
mdocx "*.docx" -f docx
mdocx "logs/*.log" -f log
mdocx "src/*" -f c -f h
```

### ディレクトリ指定

ディレクトリを指定した場合、その配下の `-f` / `--from` に該当するファイルを処理します（複数指定可）。

```bash
# 直下のみ処理
mdocx docs -f docx

# .c と .h を直下から処理
mdocx src -f c -f h

# 再帰的に処理
mdocx docs -f docx -r
```

### 既存拡張子の後ろに変換先拡張子を追加

`-a` / `--apend-suffix` を指定すると、出力先を省略した場合に元の拡張子を保持したまま変換先拡張子を後ろへ追加します。

```bash
# a.docx.md が作成されます
mdocx -a a.docx

# note.md.docx が作成されます
mdocx -a note.md
```

### 更新日時チェックで変換をスキップ

`-c` / `--check-timestamp` を指定すると、入力ファイルと出力ファイルの更新日時が一致している場合、変換をスキップします。

```bash
# input.docx と input.md の更新日時が一致していればスキップ
mdocx -c input.docx

# -a と併用可（output は a.docx.md）
mdocx -a -c a.docx
```

## 開発とテストの実行

ユニットテストおよび統合テストを以下のコマンドで実行できます：

```bash
cargo test
```

## ライセンス

[MIT LICENSE](LICENSE)
