with open("src/walker.rs", "r") as f:
    text = f.read()

# Fix walk_single_thread_with_options
text = text.replace("for root in &roots {", "for root in roots {")
text = text.replace("queue.push_back((root.clone(), 0));", "queue.push_back((root.clone(), 0));\n") # wait, better just replace root.clone() with root
text = text.replace("queue.push_back((root.clone(), 0));", "queue.push_back((root.clone(), 0));") # Wait, if we iterate by value, `root` is owned!
