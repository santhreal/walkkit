use walkkit::{WalkItem, Walker};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let walker = Walker::new()
        .add_root("./src")
        .with_parallelism(4)
        .respect_gitignore(true)
        .skip_binary(true);

    for item in walker.walk()? {
        match item {
            WalkItem::File(file) => println!("File: {} ({} bytes)", file.path.display(), file.size),
            WalkItem::Error(err) => eprintln!("Error walking {}: {}", err.path.display(), err),
        }
    }

    Ok(())
}
