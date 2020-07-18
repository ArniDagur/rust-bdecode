use bdecode::bdecode;

fn main() {
    let node = bdecode(b"d1:ad1:bi1e1:c4:abcde1:di3ee").unwrap();
    println!(
        "Have dictionary of size {}",
        node.get_root().dict_size().unwrap()
    );
}
