# Record: vhs demo/omnicat-demo.tape
# Requires: brew install vhs && cargo build --release

Output demo/omnicat-demo.gif

Set Shell "zsh"
Set FontSize 16
Set Width 1200
Set Height 700
Set Padding 28
Set Framerate 12
Set PlaybackSpeed 1.0
Set Theme "Catppuccin Mocha"
Set WindowBar Colorful
Set BorderRadius 8

Hide
Type "cd /Users/s00d/Downloads/smartcat-main"
Enter
Type "cargo build --release -q"
Enter
Sleep 8s
Show

Type "echo 'omnicat — universal file preview'"
Enter
Sleep 1.2s

Type "clear"
Enter
Type "echo '▸ Markdown'"
Enter
Type "./target/release/omnicat demo/files/sample.md"
Enter
Sleep 4.5s

Type "clear"
Enter
Type "echo '▸ Rust syntax highlight'"
Enter
Type "./target/release/omnicat demo/files/sample.rs"
Enter
Sleep 4s

Type "clear"
Enter
Type "echo '▸ CSV table'"
Enter
Type "./target/release/omnicat demo/files/sample.csv"
Enter
Sleep 3.5s

Type "clear"
Enter
Type "echo '▸ Directory tree'"
Enter
Type "./target/release/omnicat demo/dir-tree"
Enter
Sleep 3s

Type "clear"
Enter
Type "echo '▸ Binary hex dump'"
Enter
Type "./target/release/omnicat demo/files/sample.bin"
Enter
Sleep 3s

Type "clear"
Enter
Type "echo '▸ omnicat --status'"
Enter
Type "./target/release/omnicat --status"
Enter
Sleep 4s
