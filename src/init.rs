use anyhow::{bail, Result};

pub fn print_init(shell: &str) -> Result<()> {
    let snippet = match shell {
        "zsh" | "bash" => {
            "cat() { if command -v omnicat >/dev/null 2>&1; then command omnicat \"$@\"; else command cat \"$@\"; fi; }"
        }
        "powershell" => "function cat { omnicat @args }",
        other => bail!("unsupported shell: {other}"),
    };
    println!("{snippet}");
    Ok(())
}
