# Record from repo root: cargo build --release && vhs demo/omnicat-demo.tape
# Commands are typed while hidden — GIF shows output, not keystrokes.

Output demo/omnicat-demo.gif

Set Shell "zsh"
Set FontSize 15
Set Width 1120
Set Height 660
Set Padding 24
Set Framerate 15
Set PlaybackSpeed 1.0
Set Theme "Catppuccin Mocha"
Set WindowBar Colorful
Set BorderRadius 8

# Ensure binary exists (hidden; skip if already built).
Hide
Type "test -x ./target/release/omnicat || cargo build --release -q"
Enter
Sleep 3s

# Markdown
Hide
Type "clear"
Enter
Type "./target/release/omnicat demo/files/sample.md"
Enter
Show
Sleep 5s

# Rust syntax highlight
Hide
Type "clear"
Enter
Type "./target/release/omnicat demo/files/sample.rs"
Enter
Show
Sleep 4.5s

# CSV table
Hide
Type "clear"
Enter
Type "./target/release/omnicat demo/files/sample.csv"
Enter
Show
Sleep 4s

# Database query (streaming SQL on dump)
Hide
Type "clear"
Enter
Type "./target/release/omnicat db demo/db/sample.sql --query 'SELECT id FROM users LIMIT 5'"
Enter
Show
Sleep 5s

# Log analysis
Hide
Type "clear"
Enter
Type "./target/release/omnicat log demo/log/json.log --stats"
Enter
Show
Sleep 4s

# Directory tree
Hide
Type "clear"
Enter
Type "./target/release/omnicat demo/dir-tree"
Enter
Show
Sleep 3.5s

# Handler overview
Hide
Type "clear"
Enter
Type "./target/release/omnicat --status"
Enter
Show
Sleep 3s
