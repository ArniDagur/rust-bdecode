use bdecode::bdecode;

fn main() {
    let node = bdecode(b"de").unwrap();
    println!(
        "Have dictionary of size {}",
        node.get_root().dict_size().unwrap()
    );
}
