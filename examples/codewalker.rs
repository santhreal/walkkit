use walkkit::codewalker::{CodeWalker, WalkConfig};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = WalkConfig {
        skip_binary: true,
        respect_gitignore: true,
        ..WalkConfig::default()
    };

    let walker = CodeWalker::new("./src", config);
    let entries = walker.walk()?;

    println!("Scanned {} code files:", entries.len());
    for entry in entries {
        println!("  {} ({} bytes)", entry.path.display(), entry.size);
    }

    Ok(())
}
